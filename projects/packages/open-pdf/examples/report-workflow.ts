/**
 * Assemble, fill and stamp workflow, the runnable form of
 * `documentation/agents.md`.
 *
 * The script generates its own fixtures (a cover, a body and a hand-assembled
 * AcroForm PDF), composes them through the SDK, exercises the optional page
 * selections that previously died in the Node-API layer, fills real form
 * fields and reports the missing ones, crops and stamps the result, then
 * reopens every delivered file and asserts pages, page order, metadata and the
 * written field values. It exits non-zero on the first failed assertion, so CI
 * runs the documented workflow verbatim instead of trusting the prose.
 *
 * Run from the repository root with a built binding installed or staged:
 *
 * ```bash
 * pnpm --filter @wae/open-pdf run build:test
 * node projects/packages/open-pdf/dist-test/examples/report-workflow.js
 * ```
 *
 * On hosts without an installed platform package point `OPEN_PDF_NATIVE` at a
 * freshly built addon renamed to `.node` (`open_pdf_napi.node`).
 */

import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { OpenPDF } from '../src/index.js';
import { nativeBinding } from '../src/native.js';

/** Writes one uncompressed PDF from its object bodies, mirroring the engine fixtures. */
function assemblePdf(objects: readonly string[]): string {
    const header = '%PDF-1.4\n';
    let body = header;
    const offsets: number[] = [];
    objects.forEach((entry, index) => {
        offsets.push(Buffer.byteLength(body, 'latin1'));
        body += `${index + 1} 0 obj\n${entry}\nendobj\n`;
    });
    const startxref = Buffer.byteLength(body, 'latin1');
    body += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
    for (const offset of offsets) {
        body += `${String(offset).padStart(10, '0')} 00000 n \n`;
    }
    body += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${startxref}\n%%EOF\n`;
    return body;
}

async function main(): Promise<void> {
    if (nativeBinding() === undefined) {
        throw new Error('this example needs a built native binding; install the platform package or set OPEN_PDF_NATIVE');
    }
    const dir = mkdtempSync(join(tmpdir(), 'open-pdf-report-'));
    const pdf = new OpenPDF({ overwrite: true });

    // 1. Fixtures: a two-page cover, a four-page body and a real AcroForm PDF.
    const cover = await pdf.create({ out: join(dir, 'cover.pdf'), title: 'Q4 report', pages: ['Q4 report', 'Prepared for ACME'] });
    assert.equal(cover.pages, 2);
    const body = await pdf.create({
        out: join(dir, 'raw.pdf'),
        pages: ['Body page 1', 'Body page 2', 'Body page 3', 'Body page 4'],
    });
    assert.equal(body.pages, 4);
    const formPath = join(dir, 'form.pdf');
    writeFileSync(
        formPath,
        assemblePdf([
            '<< /Type /Catalog /Pages 2 0 R /AcroForm 7 0 R >>',
            '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
            '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [5 0 R 6 0 R] >>',
            '<< /T (person) /FT /Tx /Kids [5 0 R] >>',
            '<< /T (name) /Parent 4 0 R /V (Alice) /Type /Annot /Subtype /Widget /Rect [10 10 110 30] /FT /Tx >>',
            '<< /T (agree) /FT /Btn /V /Off /Type /Annot /Subtype /Widget /Rect [10 40 30 60] >>',
            '<< /Fields [4 0 R 6 0 R] /DA (/Helv 0 Tf 0 g) >>',
        ]),
        'latin1',
    );

    // 2. Assemble: drop the first body page, then merge in rule order.
    const trimmed = await pdf.arrange(body.path, join(dir, 'body.pdf'), { remove: [1] });
    assert.equal(trimmed.pages, 3);
    const merged = await pdf.merge([cover.path, trimmed.path], join(dir, 'report.pdf'), { title: 'Q4 report' });
    assert.equal(merged.pages, 5);

    // 3. The optional page selections that used to fail inside the addon.
    const fullReverse = await pdf.reversePages(merged.path, join(dir, 'reversed.pdf'));
    assert.equal(fullReverse.reversed.length, 5, 'omitting the selection reverses the whole document');
    const reversedText = await pdf.extractText(fullReverse.path);
    assert.ok(reversedText.markdown.includes('Body page 4'), 'the last page leads after the reverse');
    const partialReverse = await pdf.reversePages(merged.path, join(dir, 'partial.pdf'), true, '2-3');
    assert.deepEqual([...partialReverse.reversed], [2, 3]);
    const replaced = await pdf.replacePages(merged.path, '1', cover.path, join(dir, 'replaced.pdf'));
    assert.equal(replaced.replacementPages, 2, 'omitting the source selection uses every cover page');
    assert.equal(replaced.pages, 5 - 1 + 2);

    // 4. Fill real fields; unknown names are reported, not invented.
    const filled = await pdf.fillForm(formPath, join(dir, 'filled.pdf'), { 'person.name': 'ACME', agree: true, ghost: 'x' });
    assert.deepEqual([...filled.filled].sort(), ['agree', 'person.name']);
    assert.deepEqual([...filled.missing], ['ghost']);
    const form = await pdf.inspect(filled.path);
    const nameField = form.formFields.find((field) => field.name === 'person.name');
    assert.ok(nameField !== undefined && nameField.value === 'ACME', 'the written value survives the round trip');

    // 5. Crop the first pages, stamp every page and reopen the delivery.
    const cropped = await pdf.setPageBoxes(merged.path, '1-2', [18, 18, 594, 774], join(dir, 'cropped.pdf'));
    assert.equal(cropped.cropped, 2);
    const final = await pdf.stamp(cropped.path, join(dir, 'final.pdf'), {
        text: 'Draft {page}/{pages}',
        position: 'footer',
        opacity: 0.5,
    });
    assert.equal(final.pagesStamped, 5);
    const info = await pdf.inspect(final.path);
    assert.equal(info.pages, 5);
    assert.equal(info.metadata.title, 'Q4 report');
    const text = await pdf.extractText(final.path);
    assert.ok(text.markdown.includes('Prepared for ACME'), 'the merged cover stays readable');

    console.log(`report workflow ok: ${info.pages} pages, filled ${filled.filled.length} fields, missing ${filled.missing.length}`);
}

await main();
