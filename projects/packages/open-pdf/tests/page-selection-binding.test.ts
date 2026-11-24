/**
 * Binding-level regression for the optional page-selection slot.
 *
 * The `58ef2b1` fix promises that `null` AND `undefined` (and a fully omitted
 * argument) all mean "no selection" inside the Node-API layer, so a real
 * binding never answers `napi status 3` or a bogus `format/invalid` for an
 * argument problem. Every success here is verified by reopening the output
 * PDF and checking the ACTUAL page order, not just the report.
 */

import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { OpenPDF, OpenPdfError } from '../src/index.js';
import { nativeBinding } from '../src/native.js';

const native = (() => {
    try {
        return nativeBinding();
    } catch {
        return undefined;
    }
})();
const withAddon = native === undefined ? 'a built addon is not installed' : false;

/** Asserts the markers appear in the markdown in exactly this relative order. */
function assertOrder(markdown: string, markers: readonly string[]): void {
    let previous = -1;
    for (const marker of markers) {
        const at = markdown.indexOf(marker);
        assert.ok(at > previous, `expected \`${marker}\` after the previous marker in:\n${markdown}`);
        previous = at;
    }
}

/** SDK-level rejection: stable argument code, never a binding-status or format error. */
function expectArgumentInvalid(call: () => unknown): void {
    assert.throws(call, (error: unknown) => {
        assert.ok(error instanceof OpenPdfError, `expected an OpenPdfError, got ${String(error)}`);
        assert.equal(error.code, 'session/argument-invalid', error.message);
        assert.ok(!error.message.includes('napi status'), error.message);
        return true;
    });
}

async function fixture(): Promise<{ dir: string; source: string; insert: string }> {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-selection-'));
    const source = path.join(dir, 'source.pdf');
    const insert = path.join(dir, 'insert.pdf');
    await OpenPDF.create({ out: source, pages: ['One', 'Two', 'Three', 'Four'], overwrite: true });
    await OpenPDF.create({ out: insert, pages: ['Alpha', 'Beta', 'Gamma'], overwrite: true });
    return { dir, source, insert };
}

test('replacePages treats an omitted selection as absent and uses every insert page in order', { skip: withAddon }, async () => {
    const { dir, source, insert } = await fixture();
    try {
        const out = path.join(dir, 'omitted.pdf');
        const report = OpenPDF.replacePages(source, '2', insert, out);
        assert.deepEqual(report.selectedPages, [1, 2, 3]);
        assert.equal(report.pages, 6);
        assert.equal(OpenPDF.inspect(out).pages, 6);
        assertOrder(OpenPDF.extractText(out).markdown, ['One', 'Alpha', 'Beta', 'Gamma', 'Three', 'Four']);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('replacePages treats an explicit null selection as absent', { skip: withAddon }, async () => {
    const { dir, source, insert } = await fixture();
    try {
        const out = path.join(dir, 'null.pdf');
        const report = OpenPDF.replacePages(source, '2', insert, out, false, null);
        assert.deepEqual(report.selectedPages, [1, 2, 3]);
        assertOrder(OpenPDF.extractText(out).markdown, ['One', 'Alpha', 'Beta', 'Gamma', 'Three', 'Four']);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('replacePages treats an explicit undefined selection as absent', { skip: withAddon }, async () => {
    const { dir, source, insert } = await fixture();
    try {
        const out = path.join(dir, 'undefined.pdf');
        const report = OpenPDF.replacePages(source, '2', insert, out, false, undefined);
        assert.deepEqual(report.selectedPages, [1, 2, 3]);
        assertOrder(OpenPDF.extractText(out).markdown, ['One', 'Alpha', 'Beta', 'Gamma', 'Three', 'Four']);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('the binding answers a fully omitted selected argument as absent, not napi status 3', { skip: withAddon }, async () => {
    const { dir, source, insert } = await fixture();
    try {
        const out = path.join(dir, 'raw.pdf');
        // Six declared slots, only five arguments: Node-API reports `undefined`.
        const rawReplace = native!.pdf_replace_pages as unknown as (...args: unknown[]) => string;
        const rawReverse = native!.pdf_reverse_pages as unknown as (...args: unknown[]) => string;
        const report = JSON.parse(rawReplace(source, '2', insert, out, false));
        assert.deepEqual(report.selectedPages, [1, 2, 3]);
        assertOrder(OpenPDF.extractText(out).markdown, ['One', 'Alpha', 'Beta', 'Gamma', 'Three', 'Four']);

        const reversed = path.join(dir, 'raw-reverse.pdf');
        const reverseReport = JSON.parse(rawReverse(source, reversed, false));
        assert.deepEqual(reverseReport.reversed, [1, 2, 3, 4]);
        assertOrder(OpenPDF.extractText(reversed).markdown, ['Four', 'Three', 'Two', 'One']);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('replacePages honours the explicit selection order 3,1 in the real output', { skip: withAddon }, async () => {
    const { dir, source, insert } = await fixture();
    try {
        const out = path.join(dir, 'order.pdf');
        const report = OpenPDF.replacePages(source, '2', insert, out, false, '3,1');
        assert.deepEqual(report.selectedPages, [3, 1]);
        assert.equal(report.pages, 5);
        assertOrder(OpenPDF.extractText(out).markdown, ['One', 'Gamma', 'Alpha', 'Three', 'Four']);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('replacePages rejects duplicate pages in the selection and in the target list', { skip: withAddon }, async () => {
    const { dir, source, insert } = await fixture();
    try {
        expectArgumentInvalid(() => OpenPDF.replacePages(source, '2,2', insert, path.join(dir, 'dup-target.pdf')));
        expectArgumentInvalid(() => OpenPDF.replacePages(source, '2', insert, path.join(dir, 'dup-selected.pdf'), false, '2,2'));
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('replacePages rejects empty, zero, out-of-range and non-numeric page lists with a stable code', { skip: withAddon }, async () => {
    const { dir, source, insert } = await fixture();
    try {
        for (const bad of ['', '0', '9', 'x']) {
            expectArgumentInvalid(() => OpenPDF.replacePages(source, bad, insert, path.join(dir, `bad-${bad || 'empty'}.pdf`)));
        }
        expectArgumentInvalid(() => OpenPDF.replacePages(source, '2', insert, path.join(dir, 'bad-selected.pdf'), false, '0'));
        expectArgumentInvalid(() => OpenPDF.replacePages(source, '2', insert, path.join(dir, 'bad-selected.pdf'), false, '9'));
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('reversePages treats null and undefined selections as absent and reverses every page', { skip: withAddon }, async () => {
    const { dir, source } = await fixture();
    try {
        for (const absent of [null, undefined] as const) {
            const out = path.join(dir, `reverse-${String(absent)}.pdf`);
            const report = OpenPDF.reversePages(source, out, false, absent);
            assert.deepEqual(report.reversed, [1, 2, 3, 4]);
            assert.equal(OpenPDF.inspect(out).pages, 4);
            assertOrder(OpenPDF.extractText(out).markdown, ['Four', 'Three', 'Two', 'One']);
        }
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('reversePages honours an explicit selection and keeps the other pages in place', { skip: withAddon }, async () => {
    const { dir, source } = await fixture();
    try {
        const out = path.join(dir, 'reverse-selected.pdf');
        const report = OpenPDF.reversePages(source, out, false, '2,4');
        assert.deepEqual(report.reversed, [2, 4]);
        assertOrder(OpenPDF.extractText(out).markdown, ['One', 'Four', 'Three', 'Two']);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('reversePages rejects duplicate, zero, out-of-range and non-numeric selections', { skip: withAddon }, async () => {
    const { dir, source } = await fixture();
    try {
        for (const bad of ['2,2', '0', '9', 'x', '3-1']) {
            expectArgumentInvalid(() => OpenPDF.reversePages(source, path.join(dir, `bad-${bad}.pdf`), false, bad));
        }
        // The binding contract maps an empty selection string to "absent",
        // exactly like `null` and `undefined`, so it reverses every page.
        const empty = path.join(dir, 'reverse-empty.pdf');
        const report = OpenPDF.reversePages(source, empty, false, '');
        assert.deepEqual(report.reversed, [1, 2, 3, 4]);
        assertOrder(OpenPDF.extractText(empty).markdown, ['Four', 'Three', 'Two', 'One']);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});
