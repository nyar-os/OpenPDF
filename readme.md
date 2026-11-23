# OpenPDF

![PDF 1.7](https://img.shields.io/badge/format-PDF%201.7-blue)
![Merge Split Create](https://img.shields.io/badge/merge%20%7C%20split%20%7C%20create-built--in-brightgreen)
![No external PDF library](https://img.shields.io/badge/PDF%20library-none-lightgrey)
![License MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue)

[Package](./projects/packages/open-pdf/README.md) · [Agent reference](./documentation/agents.md) · [License](./License.md)

## 💡 What is OpenPDF?

OpenPDF is a command-line tool and a small TypeScript SDK for practical PDF
surgery: combine documents, cut them into the pages you need, reorder or rotate
pages, rewrite metadata and outlines, fill AcroForm fields, stamp text, extract
text and embedded images, and create simple new PDFs from text. It operates on
the pages that already exist instead of re-rendering them, and every operation is
a reproducible command that can run by hand, inside a script, or under an agent.

It is not a browser-based PDF viewer or editor, and it is not a replacement for
Acrobat: there is no page canvas, no annotation UI and no page rasterization. It
is the tool you reach for when a PDF is data you need to inspect, split, combine,
label or read.

## 🚀 Getting started

### 1. Agent / prompt (recommended)

Give your coding agent the task and the package name; it can install the CLI and
run the operations for you.

```text
Install @wae/open-pdf in this project. Inspect every PDF in ./in, merge them into
./out/combined.pdf in filename order, add an outline entry for each source file,
then split the result every 10 pages into ./out/chunks/.
```

Then confirm what the installed build supports:

```bash
npx open-pdf capabilities
```

### 2. Manual install

Requires Node.js 18 or newer.

```bash
npm install @wae/open-pdf
npx open-pdf inspect input.pdf
npx open-pdf merge part-1.pdf part-2.pdf -o combined.pdf
```

The [package README](./projects/packages/open-pdf/README.md) shows the same
operations through the SDK, and the [agent reference](./documentation/agents.md)
lists every command, flag and report shape.

## ✨ Highlights

- **Page-level editing, not re-rendering.** Merge, split, keep or delete ranges,
  duplicate or insert pages from another document, add blank divider pages,
  reorder, rotate and crop existing pages; page content and resources are
  copied, not rasterized.
- **Inspect what is really there.** Version, page count and geometry, document
  metadata, bookmarks and AcroForm fields, including files that use
  cross-reference streams and object streams.
- **Text and image extraction.** Decode Flate, LZW, ASCIIHex, ASCII85 and
  RunLength streams for text; extract only the pages you ask for; search the
  extracted text line by line; write embedded JPEGs unchanged and encode 8-bit
  gray or RGB images as PNG. Every skipped part is reported.
- **Metadata and outlines.** Read the information dictionary, rewrite title,
  author, subject, keywords, creator and producer, replace the outline tree,
  and fill form values while copying every page.
- **Text stamps.** Paint a watermark, header or footer on every page or a page
  range, with font, size, rotation, opacity and colour; `{page}` and `{pages}`
  are substituted per page.
- **Create from text.** Paper size, base-14 fonts, margins, alignment, automatic
  wrapping, pagination and optional `page / total` footers.
- **Pipes and bytes.** Readers accept standard input and in-memory PDF bytes, so
  a document from another process needs no temporary file.
- **Honest failures.** Unsupported requests and skipped decodes carry stable
  codes instead of silently producing a damaged document.

## 📊 How OpenPDF compares

| | OpenPDF | Desktop PDF suites | Browser PDF editors |
|---|---|---|---|
| Runs in a script, CLI or agent | Yes | Limited | No |
| Page-level edits without re-rendering | Yes | Varies | Varies |
| Metadata, outlines and form fields as data | Yes | Through the UI | Through the UI |
| Create, stamp and fill forms programmatically | Yes | GUI-first | GUI-first |
| Render pages to an image | No | Yes | Yes |
| Open encrypted PDFs | No | Yes | Sometimes |

## 🧪 Examples

Each task maps to one command. Replace the sample names with your own files.

| Task | Command |
|---|---|
| Inspect a document | `open-pdf inspect input.pdf` |
| Read document metadata | `open-pdf metadata report.pdf` |
| Merge in argument order | `open-pdf merge part-1.pdf part-2.pdf -o combined.pdf` |
| Split by page ranges | `open-pdf split combined.pdf --ranges 1-5,10 --out-dir pages/` |
| Split every N pages | `open-pdf split combined.pdf --every 10 --out-dir pages/` |
| Keep only selected pages | `open-pdf extract-pages combined.pdf --pages 1-3,5 -o excerpt.pdf` |
| Delete selected pages | `open-pdf delete-pages combined.pdf --pages 2,7-9 -o trimmed.pdf` |
| Insert pages from another PDF | `open-pdf insert-pages combined.pdf --insert extra.pdf --after 3 -o spliced.pdf` |
| Insert blank divider pages | `open-pdf insert-blank-pages combined.pdf --after 3 --count 2 -o spaced.pdf` |
| Replace selected pages | `open-pdf replace-pages combined.pdf --pages 2,7-9 --insert extra.pdf -o swapped.pdf` |
| Reverse the page order | `open-pdf reverse-pages combined.pdf -o flipped.pdf` |
| Reorder, remove and rotate | `open-pdf arrange combined.pdf -o ordered.pdf --order 3,1,2 --remove 4 --rotate 90` |
| Rotate selected pages | `open-pdf rotate combined.pdf --pages 1-2 --degrees 180 -o rotated.pdf` |
| Set a CropBox | `open-pdf set-page-boxes combined.pdf --pages 1 --box 18,36,594,756 -o cropped.pdf` |
| Rewrite metadata | `open-pdf set-metadata combined.pdf -o tagged.pdf --title "Quarterly report"` |
| Replace the outline | `open-pdf bookmarks combined.pdf -o outlined.pdf --bookmark "Intro:1" --bookmark "Results:3"` |
| Fill form fields | `open-pdf fill-form form.pdf -o filled.pdf --set "name=Ada" --set "approved=true"` |
| Create a document | `open-pdf create -o notes.pdf --title "Notes" --page "Line one\|Line two"` |
| Stamp every page | `open-pdf stamp combined.pdf -o draft.pdf --text "DRAFT {page}/{pages}" --position center` |
| Extract text | `open-pdf extract-text combined.pdf` |
| Extract selected pages | `open-pdf extract-text report.pdf --pages 1-3,5` |
| Search extracted text | `open-pdf search contract.pdf "payment" --limit 20` |
| Extract images | `open-pdf extract-images combined.pdf -o images/` |
| Read from standard input | `cat input.pdf \| open-pdf inspect --stdin` |

More detail: [package README](./projects/packages/open-pdf/README.md) ·
[agent reference](./documentation/agents.md).

## 📜 License

Mozilla Public License 2.0. See [License.md](./License.md).
