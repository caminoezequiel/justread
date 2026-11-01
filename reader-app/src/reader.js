// Injected into every chapter by justread (see protocol.rs).
//
// The chapter lives in an iframe on the epub:// origin, so the app window
// can't reach into it. This script talks to the app with postMessage instead:
// it turns pages, asks the app to change chapter at either end, reports
// in-book link clicks, scrolling and when the page is ready, and scrolls to a
// fragment (or the end) when the app asks.
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

  document.addEventListener("keydown", (e) => {
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

  let timer = null;
  addEventListener("scroll", () => {
    clearTimeout(timer);
    timer = setTimeout(
      () => send({ kind: "scroll", path: location.pathname, fraction: fraction() }),
      150,
    );
  }, { passive: true });

  addEventListener("message", (e) => {
    const cmd = e.source === parent && e.data && e.data.justread;
    if (!cmd) return;
    if (cmd.kind === "page") {
      // Arrow keys pressed while the app window (not the chapter) had focus.
      turnPage(cmd.forward);
    } else if (cmd.kind === "scroll_to") {
      if (cmd.end) {
        scrollTo(0, root.scrollHeight);
        return;
      }
      const id = cmd.fragment;
      const target = id && (document.getElementById(id) || document.getElementsByName(id)[0]);
      if (target) target.scrollIntoView();
      else scrollTo(0, 0);
    }
  });

  addEventListener("load", () => send({ kind: "ready", path: location.pathname }));
})();
