/**
 * Binding-level regression for `fillForm` against a real AcroForm fixture.
 *
 * The Rust integration tests already cover the core engine; this file drives
 * `fillForm` through the public SDK with a hand-assembled PDF that carries a
 * text field, a checkbox, a choice and a bare button. It asserts that:
 *   - filled fields are actually updated (reopen + re-read the field values),
 *   - the `missing` list contains names that do not match any terminal field,
 *   - empty / blank payloads surface as `session/argument-invalid`,
 *   - a document without an AcroForm still succeeds with every name missing.
 */

import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { OpenPDF, OpenPdfError } from '../src/index.js';
import { nativeBinding } from '../src/native.js';

const installed = (() => {
    try {
        return nativeBinding() !== undefined;
    } catch {
        return false;
    }
})();
const withoutAddon = installed ? false : 'a built addon is not installed';

/** Mirrors the Rust `assemble` helper: one object per entry, classic xref. */
function assemble(objects: readonly string[]): Uint8Array {
    const encoder = new TextEncoder();
    const chunks: Uint8Array[] = [encoder.encode('%PDF-1.7\n')];
    const offsets: number[] = [];
    for (const body of objects) {
        offsets.push(chunks.reduce((sum, chunk) => sum + chunk.length, 0));
        chunks.push(encoder.encode(`${offsets.length} 0 obj\n${body}\nendobj\n`));
    }
    const xref = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
    chunks.push(encoder.encode(`xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`));
    for (const offset of offsets) {
        chunks.push(encoder.encode(`${String(offset).padStart(10, '0')} 00000 n \n`));
    }
    chunks.push(
        encoder.encode(`trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`),
    );
    const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
    const out = new Uint8Array(total);
    let cursor = 0;
    for (const chunk of chunks) {
        out.set(chunk, cursor);
        cursor += chunk.length;
    }
    return out;
}

/** A one-page form with a nested text field, two buttons and a choice field. */
function fillableFormBytes(): Uint8Array {
    return assemble([
        '<< /Type /Catalog /Pages 2 0 R /AcroForm 8 0 R >>',
        '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
        '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [5 0 R 6 0 R] >>',
        '<< /T (person) /FT /Tx /Kids [5 0 R] >>',
        '<< /T (name) /Parent 4 0 R /V (Alice) >>',
        '<< /T (agree) /FT /Btn /V /Off /AP << /N << /Off 10 0 R /Yes 9 0 R >> >> >>',
        '<< /T (color) /FT /Ch /Opt [(red) (green)] >>',
        '<< /Fields [4 0 R 6 0 R 7 0 R 12 0 R] /DA (/Helv 0 Tf 0 g) /XFA 11 0 R >>',
        '<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length 0 >>\nstream\n\nendstream',
        '<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length 0 >>\nstream\n\nendstream',
        '<< /Foo (bar) >>',
        '<< /T (plain) /FT /Btn /V /Off >>',
    ]);
}

test('the hand-assembled fixture exposes the expected field names and initial values', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-form-'));
    try {
        const source = path.join(dir, 'form.pdf');
        await writeFile(source, fillableFormBytes());
        const report = OpenPDF.inspect(source);
        assert.equal(report.pages, 1);
        const fields = report.formFields.map((field) => [field.name, field.value] as const);
        assert.deepEqual(fields, [
            ['person.name', 'Alice'],
            ['agree', 'Off'],
            ['color', null],
            ['plain', 'Off'],
        ]);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('fillForm writes the requested values, reopens and confirms them, and lists unknown names in missing', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-form-'));
    try {
        const source = path.join(dir, 'form.pdf');
        await writeFile(source, fillableFormBytes());
        const out = path.join(dir, 'filled.pdf');

        const report = OpenPDF.fillForm(source, out, {
            'person.name': 'Bob Smith',
            agree: true,
            color: 'green',
            absent: 'nothing',
        });
        assert.deepEqual([...report.filled].sort(), ['agree', 'color', 'person.name']);
        assert.deepEqual([...report.missing].sort(), ['absent']);
        assert.equal(report.pages, 1);

        const reopened = OpenPDF.inspect(out);
        const fields = reopened.formFields.map((field) => [field.name, field.value] as const);
        assert.deepEqual(fields, [
            ['person.name', 'Bob Smith'],
            ['agree', 'Yes'],
            ['color', 'green'],
            ['plain', 'Off'],
        ]);

        const bytes = await readFile(out, 'latin1');
        assert.ok(bytes.includes('/NeedAppearances true'), 'NeedAppearances flag missing');
        assert.ok(bytes.includes('/XFA'), 'the existing XFA key was dropped');
        assert.ok(!bytes.includes('(Alice)'), 'the old text value was kept');
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('fillForm rejects empty payloads and blank field names with session/argument-invalid', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-form-'));
    try {
        const source = path.join(dir, 'form.pdf');
        await writeFile(source, fillableFormBytes());
        const out = path.join(dir, 'rejected.pdf');

        for (const bad of [{} as Record<string, string | boolean>, { '   ': 'x' }]) {
            assert.throws(
                () => OpenPDF.fillForm(source, out, bad),
                (error: unknown) => {
                    assert.ok(error instanceof OpenPdfError);
                    assert.equal(error.code, 'session/argument-invalid');
                    return true;
                },
            );
        }
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('fillForm against a document without an AcroForm reports every name as missing', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-form-'));
    try {
        const source = path.join(dir, 'plain.pdf');
        await OpenPDF.create({ out: source, pages: ['Hello'], overwrite: true });
        const out = path.join(dir, 'out.pdf');

        const report = OpenPDF.fillForm(source, out, { name: 'x' });
        assert.deepEqual([...report.missing].sort(), ['name']);
        assert.deepEqual([...report.filled].sort(), []);
        assert.equal(report.pages, 1);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});
