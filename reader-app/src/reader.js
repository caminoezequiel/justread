// Injected into every chapter by justread (see protocol.rs).
//
// The chapter lives in an iframe on the epub:// origin, so the app window
// can't reach into it. This script talks to the app with postMessage instead:
// it turns pages, asks the app to change chapter at either end, reports
// in-book link clicks, scrolling and when the page is ready, scrolls to a
// fragment, the end, or a saved position when the app asks, and swaps in a
// new reader stylesheet when preferences change.
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
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return null;
    if (e.key === "+" || e.key === "=") return "bigger";
    if (e.key === "-" || e.key === "_") return "smaller";
    if (e.key === "0") return "reset_size";
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

  // Links within the book go through the app, which keeps its state in sync.
  // Other links (http:, mailto:) keep their default behaviour.
  document.addEventListener("click", (e) => {
    const a = e.target.closest && e.target.closest("a[href]");
    if (!a) return;
    const url = new URL(a.getAttribute("href"), location.href);
    if (url.protocol !== location.protocol || url.host !== location.host) return;
    e.preventDefault();
    send({ kind: "link", path: url.pathname, hash: url.hash });
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

  addEventListener("message", (e) => {
    const cmd = e.source === parent && e.data && e.data.justread;
    if (!cmd) return;
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
