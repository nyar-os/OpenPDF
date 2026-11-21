# OpenPDF

![PDF 1.7](https://img.shields.io/badge/format-PDF%201.7-blue)
![Merge Split Create](https://img.shields.io/badge/merge%20%7C%20split%20%7C%20create-built--in-brightgreen)
![No PDF library](https://img.shields.io/badge/PDF%20library-none-lightgrey)

Generate, merge, split and inspect PDF files from one command line or one small
SDK. The built-in engine parses the page tree, metadata, outlines, classic and
stream-based cross-reference tables, object streams and Flate-compressed streams
itself, and writes standard PDF 1.7 files — no external PDF library, no Adobe
Acrobat, no system dependency.

## ✨ What you can do

- Create a text PDF from scratch with A4 pages and Helvetica text.
- Merge any number of PDFs and keep their page order.
- Split by 1-based ranges such as `1-5,10,15-20`, or into fixed-size chunks.
- Read version, page count, page geometry, document metadata, bookmarks and
  AcroForm fields from classic PDFs as well as PDFs that use cross-reference and
  object streams.
- Extract text from Flate, LZW, ASCIIHex, ASCII85 and RunLength streams and
  report every skipped part.
- Extract embedded images: JPEG payloads are written unchanged and 8-bit
  DeviceGray/DeviceRGB streams become PNG (with `/SMask` alpha).

## 🚀 Usage

```bash
# Read version, pages, metadata, page sizes and outlines
open-pdf inspect input.pdf

# Merge files in argument order
open-pdf merge cover.pdf chapter-1.pdf chapter-2.pdf -o combined.pdf

# Split into page-range files: input-p1-5.pdf, input-p10.pdf, ...
open-pdf split input.pdf --ranges 1-5,10 --out-dir pages/

# Split into consecutive files of at most 2 pages: input-p1-2.pdf, input-p3-4.pdf, ...
open-pdf split input.pdf --every 2 --out-dir chunks/

# Create two A4 pages; | separates lines inside one page
open-pdf create --out notes.pdf --title "Meeting notes" \
  --page "Agenda|1. Budget|2. Timeline" \
  --page "Decisions"

# Extract text and list extraction losses
open-pdf extract-text notes.pdf

# Extract embedded images as JPG/PNG into a directory
open-pdf extract-images notes.pdf -o images/
```

The same operations are available as an imperative SDK:

```ts
import { OpenPDF } from '@wae/open-pdf';

const info = OpenPDF.inspect('input.pdf');
console.log(info.version, info.pages, info.metadata.title);

OpenPDF.merge(['a.pdf', 'b.pdf'], 'combined.pdf', { title: 'Combined' });
OpenPDF.split('combined.pdf', '1-2,3', 'pages');
OpenPDF.splitEvery('combined.pdf', 2, 'chunks');
OpenPDF.create({ out: 'hello.pdf', title: 'Hello', pages: ['line one\nline two'] });

const text = OpenPDF.extractText('hello.pdf');
console.log(text.markdown, text.losses);

const images = OpenPDF.extractImages('hello.pdf', 'images');
console.log(images);
```

`open-pdf capabilities` prints the honest support matrix, including what is
partial and what is not supported at all.

## 🧩 How it works

```mermaid
flowchart LR
    A[PDF files] --> B[Parser<br/>classic xref, xref streams, object streams]
    B --> C{Operation}
    C -->|inspect| D[Metadata, geometry and outline report]
    C -->|merge / split| E[Page copier<br/>shared resources deduplicated]
    C -->|create| F[Text page composer]
    E --> G[Standard PDF 1.7 writer]
    F --> G
    B --> H[Filter decoder<br/>Flate, LZW, ASCIIHex, ASCII85, RunLength]
    H --> I[Text extraction]
    H --> J[Image extraction<br/>JPEG passthrough and PNG encoding]
```

## 🧱 What it does NOT do

- **No encrypted PDFs** — files with `/Encrypt`, in a classic trailer or an
  xref stream, are rejected and never decrypted.
- **No image codecs** — `/DCTDecode` is copied as a JPEG only when its payload
  already starts with the SOI marker; `/JBIG2Decode`, `/CCITTFaxDecode` and
  `/JPXDecode` are reported as losses. PNG output covers 8-bit DeviceGray and
  DeviceRGB only.
- **No rendering or rasterization** — no page preview, SVG or print output; PNG
  files are re-encoded embedded images, not rendered pages.
- **No repair of incremental updates beyond a linear scan** — malformed or
  truncated input returns a stable error code instead of a best-effort document.
- **No form filling, JavaScript or digital signatures** — AcroForm fields are
  read for inspection only.
- **No font embedding** — created documents use the built-in Helvetica font and
  WinAnsi-compatible text.
