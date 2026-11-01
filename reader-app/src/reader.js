// Injected into every chapter by justread (see protocol.rs).
//
// The chapter lives in an iframe on the epub:// origin, so the app window
// can't reach into it. This script talks to the app with postMessage instead:
// it reports keys, in-book link clicks, scrolling and when the page is ready,
// and it scrolls to a fragment when the app asks.
(() => {
  const send = (msg) => parent.postMessage({ justread: msg }, "*");
  const root = document.documentElement;
  const fraction = () => {
    const max = root.scrollHeight - innerHeight;
    return max > 0 ? Math.min(1, scrollY / max) : 1;
  };

  document.addEventListener("keydown", (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      send({ kind: "key", key: e.key });
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
    if (!cmd || cmd.kind !== "scroll_to") return;
    const id = cmd.fragment;
    const target = id && (document.getElementById(id) || document.getElementsByName(id)[0]);
    if (target) target.scrollIntoView();
    else scrollTo(0, 0);
  });

  addEventListener("load", () => send({ kind: "ready", path: location.pathname }));
})();
