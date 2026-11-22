# @wae/open-pdf-darwin-x64

Prebuilt macOS x64 native addon for [`@wae/open-pdf`](../open-pdf/README.md). **Not a public import target.** It is installed automatically as an optional dependency on matching `os` and `cpu`; application code imports `@wae/open-pdf`.

## Example

```ts
import { OpenPDF } from '@wae/open-pdf';

const pdf = new OpenPDF();
await pdf.merge(['cover.pdf', 'body.pdf'], 'combined.pdf');
```
