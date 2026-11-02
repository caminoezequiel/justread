// Injected into every chapter by justread (see protocol.rs).
//
// The chapter lives in an iframe on the epub:// origin, so the app window
// can't reach into it. This script talks to the app with postMessage instead:
// it turns pages, asks the app to change chapter at either end, reports
// in-book link clicks, scrolling and when the page is ready, scrolls to a
// fragment, the end, or a saved position when the app asks, swaps in a
// new reader stylesheet when preferences change, and sends clicked words and
// selections for translation.
(() => {
  const send = (msg) => parent.postMessage({ justread: msg }, "*");
  const root = document.documentElement;
  const maxScroll = () => root.scrollHeight - innerHeight;
  const fraction = () => {
    const max = maxScroll();
    return max > 0 ? Math.min(1, scrollY / max) : 1;
  };

  // One "page" is a screenful, minus a little overlap so the reader keeps
  // their place. At the end (or start) of the chapter, ask the app to turn
  // to the next (or previous) chapter instead.
  const turnPage = (forward) => {
    const overlap = Math.min(48, innerHeight * 0.1);
    if (forward && scrollY >= maxScroll() - 2) {
      send({ kind: "turn", forward: true });
    } else if (!forward && scrollY <= 2) {
      send({ kind: "turn", forward: false });
    } else {
      scrollBy({ top: (forward ? 1 : -1) * (innerHeight - overlap), behavior: "instant" });
    }
  };

  // Same shortcuts as the app window (bridge.rs), handled by the app.
  const shortcut = (e) => {
    if (e.key === "Escape") return "escape";
    if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
      if (e.key === "ArrowLeft") return "back";
      if (e.key === "ArrowRight") return "forward";
    }
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return null;
    if (e.key === "+" || e.key === "=") return "bigger";
    if (e.key === "-" || e.key === "_") return "smaller";
    if (e.key === "0") return "reset_size";
    if (e.key === "f" || e.key === "F") return "find";
    return null;
  };

  document.addEventListener("keydown", (e) => {
    const action = shortcut(e);
    if (action) {
      e.preventDefault();
      send({ kind: "shortcut", action });
      return;
    }
    if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      turnPage(e.key === "ArrowRight");
    }
  });

  // The mouse's side buttons (back/forward), where the webview reports them.
  addEventListener("mouseup", (e) => {
    if (e.button !== 3 && e.button !== 4) return;
    e.preventDefault();
    send({ kind: "shortcut", action: e.button === 3 ? "back" : "forward" });
  });

  // Links within the book go through the app, which keeps its state in sync
  // and remembers where the link was clicked (the exact scroll position: the
  // debounced scroll report may not have been sent yet), for going back.
  // Other links (http:, mailto:) keep their default behaviour.
  document.addEventListener("click", (e) => {
    const a = e.target.closest && e.target.closest("a[href]");
    if (!a) {
      lookupAt(e);
      return;
    }
    const url = new URL(a.getAttribute("href"), location.href);
    if (url.protocol !== location.protocol || url.host !== location.host) return;
    e.preventDefault();
    send({
      kind: "link",
      path: url.pathname,
      hash: url.hash,
      from: location.pathname,
      fraction: fraction(),
      section: currentSection(),
    });
  });

  // TOC sections in this document (ids, sent by the app once loaded). The
  // current one is the last heading scrolled past the top third of the screen.
  let sections = [];
  const currentSection = () => {
    let current = null;
    let best = -Infinity;
    for (const id of sections) {
      const el = document.getElementById(id) || document.getElementsByName(id)[0];
      if (!el) continue;
      const top = el.getBoundingClientRect().top;
      if (top <= innerHeight * 0.3 && top > best) {
        best = top;
        current = id;
      }
    }
    return current;
  };
  const reportScroll = () =>
    send({ kind: "scroll", path: location.pathname, fraction: fraction(), section: currentSection() });

  let timer = null;
  addEventListener("scroll", () => {
    clearTimeout(timer);
    timer = setTimeout(reportScroll, 150);
  }, { passive: true });

  // --- Tap to translate -------------------------------------------------
  // A click on a word, or a mouse selection, sends the text with the rest of
  // its sentence (`before`/`after`) and where it is on screen; the app shows
  // the translation and asks for the text to be marked (`mark_lookup`).
  const WORD_CHAR = /[\p{L}\p{M}\p{N}'’-]/u;
  const EDGE = /['’-]/;
  const BLOCK = "p, li, blockquote, dd, dt, td, th, h1, h2, h3, h4, h5, h6, figcaption, pre";
  let lookupRange = null;
  const squash = (text) => text.replace(/\s+/g, " ");

  // The text position under the pointer (WebKit/Chromium, then Firefox's API).
  const caretAt = (x, y) => {
    if (document.caretRangeFromPoint) {
      const r = document.caretRangeFromPoint(x, y);
      return r && [r.startContainer, r.startOffset];
    }
    if (document.caretPositionFromPoint) {
      const p = document.caretPositionFromPoint(x, y);
      return p && [p.offsetNode, p.offset];
    }
    return null;
  };
  const wordAt = (x, y) => {
    const caret = caretAt(x, y);
    if (!caret || caret[0].nodeType !== Node.TEXT_NODE) return null;
    const [node, offset] = caret;
    const text = node.data;
    let start = offset;
    let end = offset;
    while (start > 0 && WORD_CHAR.test(text[start - 1])) start--;
    while (end < text.length && WORD_CHAR.test(text[end])) end++;
    while (start < end && EDGE.test(text[start])) start++;
    while (end > start && EDGE.test(text[end - 1])) end--;
    if (start === end) return null;
    const range = document.createRange();
    range.setStart(node, start);
    range.setEnd(node, end);
    // The caret snaps to the nearest text even from the margin: only a click
    // on the word itself counts.
    const hit = [...range.getClientRects()].some(
      (r) => x >= r.left - 1 && x <= r.right + 1 && y >= r.top - 1 && y <= r.bottom + 1,
    );
    return hit ? range : null;
  };

  // The sentence around `range`, within its paragraph (or similar block).
  const sentenceAround = (range) => {
    const parent = range.startContainer.parentElement || document.body;
    const block = parent.closest(BLOCK) || parent;
    const lead = document.createRange();
    lead.selectNodeContents(block);
    lead.setEnd(range.startContainer, range.startOffset);
    const text = block.textContent;
    const start = lead.toString().length;
    const end = Math.min(text.length, start + range.toString().length);
    let from = 0;
    let to = text.length;
    if (window.Intl && Intl.Segmenter) {
      for (const s of new Intl.Segmenter("en", { granularity: "sentence" }).segment(text)) {
        const sEnd = s.index + s.segment.length;
        if (sEnd <= start) from = sEnd;
        if (s.index >= end) {
          to = s.index;
          break;
        }
      }
    } else {
      const stops = /[.!?]+["”’)\]]*\s/g;
      for (let m = stops.exec(text); m; m = stops.exec(text)) {
        const stop = m.index + m[0].length;
        if (stop <= start) from = stop;
        else if (m.index >= end) {
          to = stop;
          break;
        }
      }
    }
    return {
      before: squash(text.slice(from, start)).trimStart(),
      after: squash(text.slice(end, to)).trimEnd(),
    };
  };

  // A selection that starts or ends inside a word takes the whole word.
  const toWholeWords = (range) => {
    const start = range.startContainer;
    if (start.nodeType === Node.TEXT_NODE) {
      let i = range.startOffset;
      while (i > 0 && WORD_CHAR.test(start.data[i - 1])) i--;
      range.setStart(start, i);
    }
    const end = range.endContainer;
    if (end.nodeType === Node.TEXT_NODE) {
      let i = range.endOffset;
      while (i < end.data.length && WORD_CHAR.test(end.data[i])) i++;
      range.setEnd(end, i);
    }
    return range;
  };

  const sendLookup = (range) => {
    const text = squash(range.toString()).trim();
    if (!text || text.length > 1000) return;
    // Already shown (a selection can end with several mouseups).
    if (
      lookupRange &&
      range.compareBoundaryPoints(Range.START_TO_START, lookupRange) === 0 &&
      range.compareBoundaryPoints(Range.END_TO_END, lookupRange) === 0
    ) {
      return;
    }
    lookupRange = range;
    const r = range.getBoundingClientRect();
    send({
      kind: "lookup",
      text,
      ...sentenceAround(range),
      rect: { left: r.left, top: r.top, right: r.right, bottom: r.bottom },
    });
  };

  const lookupAt = (e) => {
    // A click that ends a selection: handled by the mouseup below.
    if (!getSelection().isCollapsed || e.button !== 0) return;
    const range = wordAt(e.clientX, e.clientY);
    if (range) sendLookup(range);
    else send({ kind: "dismiss" });
  };
  addEventListener("mouseup", (e) => {
    if (e.button !== 0) return;
    // After the browser has updated the selection.
    setTimeout(() => {
      const selection = getSelection();
      if (!selection.isCollapsed && selection.rangeCount) {
        sendLookup(toWholeWords(selection.getRangeAt(0).cloneRange()));
      }
    }, 0);
  });

  const markLookup = () => {
    if (lookupRange && window.CSS && CSS.highlights && window.Highlight) {
      CSS.highlights.set("jr-lookup", new Highlight(lookupRange));
    }
  };
  const clearLookup = () => {
    if (window.CSS && CSS.highlights) CSS.highlights.delete("jr-lookup");
    lookupRange = null;
  };

  // --- Search -----------------------------------------------------------
  // Must match epub-core/src/search.rs exactly: text of <body> minus
  // <script>/<style>, whitespace runs folded to one space, each character
  // lowercased when that stays one character, non-overlapping matches.
  const SPACE = /[ \t\n\r\f\u00a0\u2000-\u200a\u3000]/;
  const foldChar = (ch) => {
    const lower = ch.toLowerCase();
    return [...lower].length === 1 ? lower : ch;
  };
  const fold = (text, onChar) => {
    // onChar(folded, utf16Offset, utf16Length) per kept character.
    let offset = 0;
    for (const ch of text) {
      if (SPACE.test(ch)) {
        if (!fold.inSpace) onChar(" ", offset, ch.length);
        fold.inSpace = true;
      } else {
        onChar(foldChar(ch), offset, ch.length);
        fold.inSpace = false;
      }
      offset += ch.length;
    }
  };
  const foldQuery = (query) => {
    const chars = [];
    fold.inSpace = false;
    fold(query, (c) => chars.push(c));
    while (chars[0] === " ") chars.shift();
    while (chars[chars.length - 1] === " ") chars.pop();
    return chars;
  };
  const pageText = () => {
    const chars = [];
    const where = []; // where[i] = [text node, offset, length] of chars[i]
    if (!document.body) return { chars, where };
    const walker = document.createTreeWalker(
      document.body,
      NodeFilter.SHOW_TEXT | NodeFilter.SHOW_CDATA_SECTION,
      {
        acceptNode: (n) =>
          n.parentElement && n.parentElement.closest("script, style")
            ? NodeFilter.FILTER_REJECT
            : NodeFilter.FILTER_ACCEPT,
      },
    );
    fold.inSpace = false;
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      fold(node.data, (c, offset, length) => {
        chars.push(c);
        where.push([node, offset, length]);
      });
    }
    return { chars, where };
  };
  const findRange = (query, occurrence) => {
    const needle = foldQuery(query);
    const { chars, where } = pageText();
    if (!needle.length) return null;
    let count = 0;
    let i = 0;
    while (i + needle.length <= chars.length) {
      let same = true;
      for (let j = 0; j < needle.length; j++) {
        if (chars[i + j] !== needle[j]) { same = false; break; }
      }
      if (!same) { i++; continue; }
      if (count === occurrence) {
        const [startNode, startOffset] = where[i];
        const [endNode, endOffset, endLength] = where[i + needle.length - 1];
        const range = document.createRange();
        range.setStart(startNode, startOffset);
        range.setEnd(endNode, endOffset + endLength);
        return range;
      }
      count++;
      i += needle.length;
    }
    return null;
  };
  const clearFind = () => {
    if (window.CSS && CSS.highlights) CSS.highlights.delete("jr-search");
    else getSelection().removeAllRanges();
  };
  const showFind = (query, occurrence) => {
    clearFind();
    const range = findRange(query, occurrence);
    if (!range) {
      send({ kind: "find_failed" });
      return;
    }
    if (window.CSS && CSS.highlights && window.Highlight) {
      CSS.highlights.set("jr-search", new Highlight(range));
    } else {
      const selection = getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    }
    scrollBy(0, range.getBoundingClientRect().top - innerHeight / 3);
  };

  addEventListener("message", (e) => {
    const cmd = e.source === parent && e.data && e.data.justread;
    if (!cmd) return;
    if (cmd.kind === "find") {
      showFind(cmd.query, cmd.occurrence);
      return;
    }
    if (cmd.kind === "clear_find") {
      clearFind();
      return;
    }
    if (cmd.kind === "mark_lookup") {
      markLookup();
      return;
    }
    if (cmd.kind === "clear_lookup") {
      clearLookup();
      return;
    }
    if (cmd.kind === "track_sections") {
      sections = cmd.ids;
      reportScroll();
    } else if (cmd.kind === "page") {
      // Arrow keys pressed while the app window (not the chapter) had focus.
      turnPage(cmd.forward);
    } else if (cmd.kind === "restyle") {
      // Preferences changed: load the new stylesheet next to the old one,
      // and only when it has loaded drop the old one and return to the same
      // place in the text (the layout changes, so pixels would drift).
      const old = document.querySelector('link[href^="/__justread/reader.css"]');
      const keep = fraction();
      const link = document.createElement("link");
      link.rel = "stylesheet";
      link.href = "/__justread/reader.css?v=" + cmd.version;
      link.onload = () => {
        if (old) old.remove();
        scrollTo(0, keep * maxScroll());
      };
      if (old) old.after(link);
      else (document.head || root).appendChild(link);
    } else if (cmd.kind === "scroll_to_end") {
      scrollTo(0, root.scrollHeight);
    } else if (cmd.kind === "scroll_to_fraction") {
      scrollTo(0, cmd.fraction * maxScroll());
    } else if (cmd.kind === "scroll_to") {
      const id = cmd.fragment;
      const target = id && (document.getElementById(id) || document.getElementsByName(id)[0]);
      if (target) target.scrollIntoView();
      else scrollTo(0, 0);
    }
  });

  addEventListener("load", () => send({ kind: "ready", path: location.pathname }));
})();
