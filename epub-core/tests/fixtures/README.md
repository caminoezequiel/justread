# Test fixtures

## Real books (public domain, from Project Gutenberg)

| File | Gutenberg ebook | Why it's here |
|---|---|---|
| `frankenstein-epub3.epub` | [#84](https://www.gutenberg.org/ebooks/84), "EPUB3" download | EPUB3 with a `nav.xhtml` TOC |
| `alice-epub2.epub` | [#11](https://www.gutenberg.org/ebooks/11), "EPUB (older E-readers)" download | EPUB2 with only a `toc.ncx` TOC |
| `peter-rabbit-images.epub` | [#14838](https://www.gutenberg.org/ebooks/14838), "EPUB3 (with images)" download | ~29 JPEG/PNG illustrations referenced by relative `<img src>` |

Downloaded 2026-10-31. Tests assert exact counts and labels from these files,
so re-downloading newer versions may require updating the tests.

## Hand-made books

Built from the folders in `hand-made/` by running `hand-made/build.sh`
(needs the `zip` command). Edit the folder, rerun the script, and commit both.

| File | Problem |
|---|---|
| `broken.epub` | Opens, with recoverable problems: wrong `mimetype`, percent-encoded and `..` hrefs, a manifest item missing from the zip, hrefs escaping the archive or pointing to a remote URL, unknown spine idrefs, a nav document that is not well-formed XML (falls back to the NCX). See comments in `hand-made/broken/OPS/package.opf`. |
| `empty-nav.epub` | Valid XML nav with no entries; the TOC must come from the NCX. |
| `no-container.epub` | No `META-INF/container.xml` (fatal). |
| `bad-opf.epub` | OPF is not well-formed XML (fatal). |
| `empty-spine.epub` | Spine refers only to unknown ids (fatal). |
| `not-a-zip.epub` | A text file (fatal). |
