// The PDF page of justread: renders a PDF with PDF.js as one scrolling
// document, so the chapter script (reader.js) handles scrolling, page turns,
// internal links, Back/Forward and tap-to-translate just as it does for EPUB
// chapters. Pages are placed (sized) up front and rendered as they come near
// the screen. The app's text size is the zoom (`zoom` command).
import * as pdfjsLib from "/__justread/pdf.min.mjs";

pdfjsLib.GlobalWorkerOptions.workerSrc = "/__justread/pdf.worker.min.mjs";

// The page's id on the chapter: a TOC entry with `#page-N` scrolls to it.
const pageId = (number) => `page-${number}`;
const MAX_WIDTH = 900; // CSS pixels at 100%: wide enough to read, narrow enough to hold a page
const GUTTER = 16;

const send = (msg) => parent.postMessage({ justread: msg }, "*");

// Filled in by `main`: one item per page, see `layout`.
let doc = null;
let observer = null;
let fitWidth = 0; // CSS pixels of a page at 100% zoom
let zoom = 1;
const pages = [];
let shownPage = 0; // the last page reported to the app (1-based)

// Page index (0-based) a destination points at, or null.
const pageOf = async (doc, dest) => {
  try {
    if (typeof dest === "string") dest = await doc.getDestination(dest);
    if (!Array.isArray(dest) || !dest.length) return null;
    const target = dest[0];
    if (typeof target === "number") return target;
    return target ? await doc.getPageIndex(target) : null;
  } catch {
    return null;
  }
};

// The outline as the app's TOC: `page` is the 0-based page, null if unknown.
const outline = async (doc, items) => {
  if (!items) return [];
  const out = [];
  for (const item of items) {
    out.push({
      title: item.title,
      page: item.dest ? await pageOf(doc, item.dest) : null,
      items: await outline(doc, item.items),
    });
  }
  return out;
};

// Links on a page: internal ones become `#page-N` anchors, which reader.js
// reports to the app like any in-book link.
const addLinks = async (doc, page, viewport, layer) => {
  for (const annot of await page.getAnnotations()) {
    if (annot.subtype !== "Link") continue;
    let href = null;
    if (annot.dest) {
      const target = await pageOf(doc, annot.dest);
      if (target !== null) href = `#${pageId(target + 1)}`;
    } else if (annot.url) {
      href = annot.url;
    }
    if (!href) continue;
    const [x1, y1, x2, y2] = viewport.convertToViewportRectangle(annot.rect);
    const a = document.createElement("a");
    a.href = href;
    if (!href.startsWith("#")) a.target = "_blank";
    a.style.left = `${Math.min(x1, x2)}px`;
    a.style.top = `${Math.min(y1, y2)}px`;
    a.style.width = `${Math.abs(x2 - x1)}px`;
    a.style.height = `${Math.abs(y2 - y1)}px`;
    layer.append(a);
  }
};

// Draw one page (canvas, text layer, links) into its box. Each layer is
// added only if the page wasn't zoomed (and so re-laid out) meanwhile.
const renderPage = async (item) => {
  const { page, box } = item;
  const version = item.version;
  const current = () => item.version === version;
  const viewport = page.getViewport({ scale: item.scale });
  const dpr = window.devicePixelRatio || 1;
  const canvas = document.createElement("canvas");
  canvas.width = Math.floor(viewport.width * dpr);
  canvas.height = Math.floor(viewport.height * dpr);
  canvas.style.width = `${viewport.width}px`;
  canvas.style.height = `${viewport.height}px`;
  const transform = dpr === 1 ? null : [dpr, 0, 0, dpr, 0, 0];
  await page.render({ canvasContext: canvas.getContext("2d"), viewport, transform }).promise;
  if (!current()) return;
  box.append(canvas);

  const text = document.createElement("div");
  text.className = "textLayer";
  box.append(text);
  await new pdfjsLib.TextLayer({
    textContentSource: page.streamTextContent(),
    container: text,
    viewport,
  }).render();
  // PDF.js doesn't separate the text items of one line from the next with
  // spaces, so "run" + "out" would read as "runout" for translation.
  for (const span of [...text.children]) {
    span.after(" ");
  }

  const links = document.createElement("div");
  links.className = "links";
  box.append(links);
  await addLinks(doc, page, viewport, links);
};

// Size every page for the current zoom; the pages are then rendered again
// as they come near the screen.
const layout = () => {
  for (const item of pages) {
    item.scale = (fitWidth * zoom) / item.base.width;
    item.box.style.width = `${item.base.width * item.scale}px`;
    item.box.style.height = `${item.base.height * item.scale}px`;
  }
};

// Zoom to `scale` (1 = fit to width), keeping the reading position: the same
// fraction of the document stays at the top of the window.
const setZoom = (scale) => {
  if (!pages.length) {
    zoom = scale;
    return;
  }
  const max = document.documentElement.scrollHeight - innerHeight;
  const fraction = max > 0 ? scrollY / max : 0;
  zoom = scale;
  layout();
  for (const item of pages) {
    item.version++; // drops any render still running at the old size
    item.rendered = false;
    item.box.replaceChildren();
    // Observing again makes the observer report the page's current position.
    observer.unobserve(item.box);
    observer.observe(item.box);
  }
  const newMax = document.documentElement.scrollHeight - innerHeight;
  scrollTo(0, fraction * newMax);
  reportPage();
};

// The page at the middle of the window (1-based), sent when it changes. The
// last page whose top is above the middle is the one being read.
const reportPage = () => {
  let number = 1;
  for (let index = 0; index < pages.length; index++) {
    if (pages[index].box.getBoundingClientRect().top > innerHeight / 2) break;
    number = index + 1;
  }
  if (number === shownPage) return;
  shownPage = number;
  send({ kind: "page", path: location.pathname, number, count: pages.length });
};

// Scrolling fires many events; check once per frame.
let pageFrame = 0;
addEventListener("scroll", () => {
  if (pageFrame) return;
  pageFrame = requestAnimationFrame(() => {
    pageFrame = 0;
    reportPage();
  });
});

addEventListener("message", (e) => {
  const cmd = e.source === parent && e.data && e.data.justread;
  if (cmd && cmd.kind === "zoom") setZoom(cmd.scale);
});

const main = async () => {
  const root = document.getElementById("pages");
  doc = await pdfjsLib.getDocument({ url: "document.pdf" }).promise;

  // The outline first: the app reads the sections when the page is ready.
  send({ kind: "outline", entries: await outline(doc, await doc.getOutline()) });

  fitWidth = Math.min(MAX_WIDTH, root.clientWidth - 2 * GUTTER);
  for (let number = 1; number <= doc.numPages; number++) {
    const page = await doc.getPage(number);
    const box = document.createElement("div");
    box.className = "page";
    box.id = pageId(number);
    root.append(box);
    pages.push({ page, box, base: page.getViewport({ scale: 1 }), scale: 1, rendered: false, version: 0 });
  }
  layout();
  reportPage();

  // Render pages as they approach the screen, not all up front.
  observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        const item = pages.find((p) => p.box === entry.target);
        if (!item || item.rendered) continue;
        item.rendered = true;
        renderPage(item).catch((e) => console.error(e));
      }
    },
    { rootMargin: "150% 0px" },
  );
  for (const { box } of pages) observer.observe(box);

  send({ kind: "ready", path: location.pathname });
};

main().catch((error) => {
  const message = document.createElement("p");
  message.style.padding = "2rem";
  message.textContent = `This PDF could not be opened: ${error.message || error}`;
  document.body.replaceChildren(message);
  send({ kind: "ready", path: location.pathname });
});
