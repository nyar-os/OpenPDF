# @wae/open-pdf

A session-style TypeScript API for page-level PDF work, with the `open-pdf` CLI
wired to the same methods.

## API surface

- **Session and inspection** — `new OpenPDF({ locale, overwrite })`, `capabilities()`, `inspect()`, `outline()`, `metadata()`
- **Page assembly** — `merge()`, `split()`, `splitEvery()`, `arrange()`, `deletePages()`, `duplicatePages()`, `insertPages()`, `replacePages()`, `reversePages()`, `keepPages()`
- **Page edits** — `rotatePages()`, `setPageBoxes()`
- **Metadata, outlines and forms** — `setMetadata()`, `setBookmarks()`, `fillForm()`
- **Creation and stamping** — `create()`, `stamp()`
- **Extraction** — `extractText(path, { pages })`, `search(path, query, { limit, pages, matchCase, wholeWord, skip })`, `extractImages(path, outDir, { pages })`
- **Byte input** — `inspectBytes()`, `extractTextBytes()`, `extractImagesBytes()`

Each operation returns a report object; static compatibility wrappers mirror the
session methods. Failures carry a stable `code` such as `io/exists` or
`session/argument-invalid`.

## CLI quick start

```bash
open-pdf create -o cover.pdf --title Cover --page "Hello" --overwrite
open-pdf inspect cover.pdf
open-pdf open cover.pdf
```

## Example

```ts
import { OpenPDF } from '@wae/open-pdf';

const pdf = new OpenPDF({ overwrite: true });
const created = await pdf.create({ out: 'cover.pdf', title: 'Cover', pages: ['Hello'] });
const report = await pdf.merge([created.path, 'body.pdf'], 'combined.pdf');
const info = await pdf.inspect(report.path);

console.log(info.pages, info.metadata.title);
```

## Related packages

The platform addon is resolved automatically from
`@wae/open-pdf-<platform>-<arch>`, so there is no separate import. Set
`OPEN_PDF_NATIVE` to load a locally built `.node` file instead.

See the [project README](../../../readme.md) for the product overview.
