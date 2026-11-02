// The PDF page of justread: renders a PDF with PDF.js as one scrolling
// document, so the chapter script (reader.js) handles scrolling, page turns,
// internal links, Back/Forward and tap-to-translate just as it does for EPUB
// chapters. Pages are placed (sized) up front and rendered as they come near
// the screen.
import * as pdfjsLib from "/__justread/pdf.min.mjs";

pdfjsLib.GlobalWorkerOptions.workerSrc = "/__justread/pdf.worker.min.mjs";

// The page's id on the chapter: a TOC entry with `#page-N` scrolls to it.
const pageId = (number) => `page-${number}`;
const MAX_WIDTH = 900; // CSS pixels: wide enough to read, narrow enough to hold a page
const GUTTER = 16;

const send = (msg) => parent.postMessage({ justread: msg }, "*");

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

// Draw one page (canvas, text layer, links) into its placeholder.
const renderPage = async (doc, page, box, scale) => {
  const viewport = page.getViewport({ scale });
  const dpr = window.devicePixelRatio || 1;
  const canvas = document.createElement("canvas");
  canvas.width = Math.floor(viewport.width * dpr);
  canvas.height = Math.floor(viewport.height * dpr);
  canvas.style.width = `${viewport.width}px`;
  canvas.style.height = `${viewport.height}px`;
  box.append(canvas);
  const transform = dpr === 1 ? null : [dpr, 0, 0, dpr, 0, 0];
  await page.render({ canvasContext: canvas.getContext("2d"), viewport, transform }).promise;

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

const main = async () => {
  const root = document.getElementById("pages");
  const doc = await pdfjsLib.getDocument({ url: "document.pdf" }).promise;

  // The outline first: the app reads the sections when the page is ready.
  send({ kind: "outline", entries: await outline(doc, await doc.getOutline()) });

  const width = Math.min(MAX_WIDTH, root.clientWidth - 2 * GUTTER);
  const pages = [];
  for (let number = 1; number <= doc.numPages; number++) {
    const page = await doc.getPage(number);
    const base = page.getViewport({ scale: 1 });
    const scale = width / base.width;
    const box = document.createElement("div");
    box.className = "page";
    box.id = pageId(number);
    box.style.width = `${width}px`;
    box.style.height = `${base.height * scale}px`;
    root.append(box);
    pages.push({ page, box, scale, rendered: false });
  }

  // Render pages as they approach the screen, not all up front.
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        const item = pages.find((p) => p.box === entry.target);
        if (!item || item.rendered) continue;
        item.rendered = true;
        renderPage(doc, item.page, item.box, item.scale).catch((e) => console.error(e));
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
