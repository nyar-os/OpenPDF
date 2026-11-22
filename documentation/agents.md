# OpenPDF agent reference

Concise command, flag, report and failure reference for agents driving
`open-pdf`. Every flag below is declared in `src/cli.ts`; report fields come
from `src/types.ts`.

## Discovery

Start with the capability matrix instead of assuming support:

```bash
open-pdf capabilities
```

The matrix covers `engine`, the PDF format entry (`structures`, `filters` and
image colour spaces), `operations`, `checks` and `renderers`. Every entry is
`supported`, `partial` or `unsupported`. Notable current values: `extractText`
is `partial`, all `renderers` are `unsupported`, and
`formats.pdf.structures.encrypted` is `unsupported`.

Use `open-pdf <command> --help` for the localized help text of one command.

## Usage lines

```text
open-pdf capabilities
open-pdf inspect <file> | --stdin
open-pdf merge <file...> -o <out> [--title <text>] [--overwrite]
open-pdf split <file> -o <out-dir> (--ranges <list> | --every <count>)
open-pdf arrange <file> -o <out> [--order <list>] [--remove <list>] [--rotate <degrees>]
open-pdf delete-pages <file> --pages <list> -o <out> [--overwrite]
open-pdf extract-pages <file> --pages <list> -o <out> [--overwrite]
open-pdf rotate <file> --pages <list> --degrees <90|180|270> -o <out> [--overwrite]
open-pdf set-page-boxes <file> --pages <list> --box <l,b,r,t> -o <out> [--overwrite]
open-pdf set-metadata <file> -o <out> [--title <text>] [--author <text>] [--subject <text>] [--keywords <text>] [--creator <text>] [--producer <text>]
open-pdf bookmarks <file> -o <out> --bookmark "Title:Page" [--bookmark "Title:Page"...] [--depth <level>]
open-pdf fill-form <file> -o <out> --set Name=Value [--set Name=Value...]
open-pdf create -o <out> --page <text> [--page <text>...] [--title <text>] [--page-size <size>] [--font <name>] [--font-size <pt>] [--margin <pt>] [--align <align>] [--author <text>] [--subject <text>] [--keywords <text>] [--creator <text>] [--page-numbers] [--overwrite]
open-pdf stamp <file> -o <out> --text <text> [--position <position>] [--font <name>] [--font-size <pt>] [--rotation <degrees>] [--opacity <value>] [--color <hex>] [--pages <list>]
open-pdf extract-text <file> [--pages <list>] | --stdin
open-pdf search <file> <query> [--limit <n>] [--pages <list>]
open-pdf extract-images <file> [--pages <list>] | --stdin -o <out-dir> [--name <stem>]
```

`-o` is `--out` everywhere except `split` and `extract-images`, where it is
`--out-dir`.

## Flag values

- `<list>` page ranges are 1-based and comma-separated, for example `1-3,5`.
  Each part is `N` or `N-M` with `N <= M`. Out-of-range bounds are rejected; no
  operation truncates silently.
- `--every` must be a positive integer.
- `--order` and `--remove` are comma-separated page numbers. When both are
  given, removal runs first and `--order` must list every surviving page once.
- `--rotate` on `arrange` and `--degrees` on `rotate` are clockwise and must be
  `90`, `180` or `270`. The CLI rejects other degrees on `rotate`; pass only
  supported values to `arrange`.
- `--box` is exactly four finite numbers, `left,bottom,right,top` in points. It
  writes `/CropBox` on the selected pages and leaves `/MediaBox` unchanged.
- `--bookmark` is `Title:Page`, split at the last colon, so titles may contain
  colons. At least one entry is required. `--depth` is a non-negative integer
  applied to every entry in the call; the default is `0`.
- `--set` is `Name=Value`, split at the first `=`, so values may contain `=`.
  At least one entry is required. The exact values `true` and `false` become
  booleans; everything else is a string.
- `--page` on `create` is repeatable and at least one is required. A `|` inside
  one `--page` value becomes a newline; there is no escape for a literal pipe.
- `--page-size` accepts `a3`, `a4`, `a5`, `letter`, `legal`, `tabloid` or
  `WIDTHxHEIGHT`. The default is Letter.
- `--font` accepts `helvetica`, `helvetica-bold`, `times`, `times-bold`,
  `courier` or `courier-bold`.
- `--font-size` and `--margin` are numbers. Body font size is `6..144`.
- `--align` is `left`, `center` or `right`.
- `--position` on `stamp` accepts `center` (default), `top`/`header` or
  `bottom`/`footer`.
- Stamp `--rotation` is `-180..180` and applies to a centered stamp. `--opacity`
  is `0..1`, defaulting to `0.3` centered and `0.6` otherwise. `--color` is
  `#RGB` or `#RRGGBB`. `{page}` and `{pages}` are substituted per page.
  `--pages` defaults to every page.
- `--title` on `merge` replaces `/Title` in the output information dictionary.
- `set-metadata` keeps a field's source value when the flag is omitted; an
  empty string removes the field.
- `--name` on `extract-images --stdin` sets the output file stem and defaults to
  `stdin`; it is ignored for path input.
- `--pages` on `extract-images` keeps only the selected pages; images are named
  after their source page and `--pages` is rejected with `--stdin`.

## Reports

Default output is human-readable. The hidden global `--json` switch writes
exactly one `wae.cli-result/v0` envelope to stdout:

```json
{ "schema": "wae.cli-result/v0", "ok": true, "data": {} }
```

Failures use `{ "schema": "wae.cli-result/v0", "ok": false, "error": { "code": "...", "message": "..." } }`.

Report shapes:

| Command | Shape |
|---|---|
| `capabilities` | `{ engine, formats, operations, checks, renderers }` |
| `inspect` | `{ label, version, pages, encrypted, metadata, pageSizes, bookmarks, formFields, warnings }` |
| `merge` | `{ path, pages, bytes }` |
| `split` | `{ documents: [{ path, pages }] }` |
| `arrange` | `{ path, pages }` |
| `delete-pages` | `{ path, pages, removed, bytes }` |
| `extract-pages` | `{ path, pages, kept, removed, bytes }` |
| `rotate` | `{ path, pages, rotated, bytes }` |
| `set-page-boxes` | `{ path, pages, cropped, bytes }` |
| `set-metadata` | `{ path, pages, metadata }` |
| `bookmarks` | `{ path, pages, bookmarks }` |
| `fill-form` | `{ path, filled, missing, pages }` |
| `create` | `{ path, pages, bytes }` |
| `stamp` | `{ path, pages, pagesStamped, bytes }` |
| `extract-text` | `{ pages, sourcePages?, selected?, markdown, losses: [{ code, page, message }] }` |
| `search` | `{ path, query, limit, pages, sourcePages?, selected?, lines, truncated, hits, losses }` |
| `extract-images` | `[{ page, index, name, width, height, filter, path, loss? }]` (`page` is the source page when `--pages` is used) |

Nested values:

- `metadata`: `title`, `author`, `subject`, `keywords`, `creator`, `producer`,
  `creationDate`, `modDate`; each is a string or `null`.
- `pageSizes[]`: `{ widthPt, heightPt, rotation }`.
- `bookmarks[]` from `inspect`: `{ title, depth }`.
- `formFields[]`: `{ name, type, value, readOnly, required }`, where `name` is
  the fully qualified `/T` chain and `type` is `Tx`, `Btn`, `Ch`, `Sig` or
  `null`.
- Extracted images with an empty `path` were skipped; `loss` explains why.
- `search` hits are `{ line, column, start, end, match, text }`. `line` and
  `column` are one-based, `start` and `end` are byte offsets within `text`, and
  matching is case-sensitive on the extracted Markdown. `truncated` is `true`
  when more matches exist beyond `limit` (default 100, clamped to 1-500).
  `--pages` projects the selection the same way as `extract-text --pages`, so
  `hits` report lines within that projection while `sourcePages` and `selected`
  name the source pages.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. |
| `1` | Command failure, including validation performed inside a handler. |
| `2` | Parse failure: unknown command or unknown option. Missing required flags are validated by the command and exit `1`. |
| `3` | Capability failure (interactive prompt unavailable). Not used by these commands. |
| `4` | Canceled prompt. Not used by these commands. |
| `5` | Action required. Not used by these commands. |

## Stable error codes

SDK operations throw `OpenPdfError` with one of these codes; the CLI relays the
same code in JSON mode and in its human diagnostic:

| Code | Typical cause |
|---|---|
| `session/argument-invalid` | Malformed page list, page number or operation argument. |
| `format/invalid` | Input is not a readable PDF. |
| `format/unsupported` | Format family is not supported. |
| `io/read` | Input could not be read. |
| `io/write` | Output could not be written. |
| `io/exists` | Output exists and overwrite was not allowed. |
| `capability/not-implemented` | The native addon is absent or the operation is not implemented. |

CLI-side validation uses `open-pdf.cli.*` diagnostic codes and exits `1`:
`input-required` (path and `--stdin` combined, or neither), `stdin-empty`,
`inputs-required`, `out-required`, `file-required`, `split-args`,
`arrange-args`, `pages-args`, `degrees-args`, `box-args`, `bookmark-args`,
`set-args`, `page-required`, `create-style`, `stamp-args` and
`image-dir-required`.

## Standard input

Only `inspect`, `extract-text` and `extract-images` accept `--stdin`. A path and
`--stdin` are mutually exclusive. Standard input is read to the end as raw PDF
bytes; empty input fails with `open-pdf.cli.stdin-empty`. Writers and
transformations never read from stdin, and `split` always reads a path.
`extract-text --pages` needs a path: standard input cannot be paginated.

## Overwrite rules

`--overwrite` exists only on `merge`, `delete-pages`, `extract-pages`, `rotate`,
`set-page-boxes` and `create`. The default is off. When a guarded output already
exists and overwrite is not allowed, the operation fails with `io/exists`;
nothing is replaced in place.

`split`, `arrange`, `set-metadata`, `bookmarks`, `fill-form` and `stamp` expose
no overwrite flag. In the SDK, `new OpenPDF({ overwrite: true })` supplies the
default for `merge`, `deletePages`, `keepPages`, `rotatePages`, `setPageBoxes`
and `create`; the remaining methods do not accept an overwrite argument.

## Guardrails

- Encrypted PDFs are rejected; no decryption path exists.
- There is no page rendering, rasterization or SVG output. PNG files produced
  by `extract-images` are re-encoded embedded images, not rendered pages.
- JPEG payloads are written unchanged only when they begin with the SOI marker;
  `/JBIG2Decode`, `/CCITTFaxDecode` and `/JPXDecode` are reported as losses.
- PNG encoding covers 8-bit DeviceGray and DeviceRGB only.
- Created documents use base-14 fonts; custom font embedding is not available.
- Forms are read for inspection and text/boolean values can be filled. There is
  no JavaScript execution and no digital signature support.
- Text extraction is `partial`: always read `losses` and report them rather than
  presenting the output as complete.
- Malformed or truncated input returns a stable error code; there is no
  best-effort repair beyond a linear scan.
- Check `capabilities` before relying on an operation, and treat every reported
  `loss` or `warning` as part of the result.
