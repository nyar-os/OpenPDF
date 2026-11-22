/**
 * Smoke tests for the declared surface.
 *
 * They assert shell semantics only: one stable code when the addon is absent,
 * a format registry for the PDF family, and synchronous argument contracts.
 * Native PDF behaviour is covered by the Rust integration tests.
 */

import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { OpenPDF, OpenPdfError, documentFormats, errorCodes, formatFromPath, formatInfo, isPdfPath } from '../src/index.js';
import { nativeBinding } from '../src/native.js';
import { expandPageList, formatPageList } from '../src/service.js';

const installed = (() => {
    try {
        return nativeBinding() !== undefined;
    } catch {
        return false;
    }
})();
const withAddon = installed ? 'a built addon is installed' : false;
const withoutAddon = installed ? false : 'a built addon is not installed';

test('format registry classifies PDF', () => {
    assert.deepEqual([...documentFormats], ['pdf']);
    assert.equal(formatInfo('pdf').family, 'fixed');
    assert.equal(formatInfo('pdf').markup, 'fixed');
    assert.equal(formatInfo('pdf').opensAs, 'fixed-document');
    assert.equal(formatFromPath('Report.PDF'), 'pdf');
    assert.equal(formatFromPath('report.txt'), undefined);
    assert.equal(formatFromPath('no-extension'), undefined);
    assert.equal(isPdfPath('deck.pdf'), true);
    assert.equal(isPdfPath('deck.docx'), false);
});

test('page lists expand to ascending unique pages', () => {
    assert.deepEqual(expandPageList('1-3,5', 5), [1, 2, 3, 5]);
    assert.deepEqual(expandPageList('5,1,5', 5), [1, 5]);
    assert.equal(formatPageList([2, 3, 4, 6]), '2-4,6');
});

test('page lists reject empty, reversed and out-of-range bounds', () => {
    assert.throws(() => expandPageList('', 5), /must list/);
    assert.throws(() => expandPageList('0', 5), /not a 1-based page range/);
    assert.throws(() => expandPageList('3-1', 5), /not a 1-based page range/);
    assert.throws(() => expandPageList('6', 5), /past the document/);
    assert.throws(() => expandPageList('1-x', 5), /not a page number or range/);
});

test('without an installed addon every engine call reports capability/not-implemented', { skip: withAddon }, () => {
    const calls: (() => unknown)[] = [
        () => OpenPDF.capabilities(),
        () => OpenPDF.inspect('input.pdf'),
        () => OpenPDF.merge(['a.pdf', 'b.pdf'], 'out.pdf'),
        () => OpenPDF.split('input.pdf', '1-2', 'out'),
        () => OpenPDF.splitEvery('input.pdf', 2, 'out'),
        () => OpenPDF.arrange('input.pdf', 'out.pdf', { order: [1] }),
        () => OpenPDF.deletePages('input.pdf', '1', 'out.pdf'),
        () => OpenPDF.rotatePages('input.pdf', '1', 90, 'out.pdf'),
        () => OpenPDF.setPageBoxes('input.pdf', '1', [0, 0, 10, 10], 'out.pdf'),
        () => OpenPDF.setMetadata('input.pdf', 'out.pdf', { title: 'x' }),
        () => OpenPDF.setBookmarks('input.pdf', 'out.pdf', [{ title: 'Intro', page: 1 }]),
        () => OpenPDF.fillForm('input.pdf', 'out.pdf', { name: 'x' }),
        () => OpenPDF.create({ out: 'new.pdf', pages: ['hello'] }),
        () => OpenPDF.extractText('input.pdf'),
        () => OpenPDF.extractImages('input.pdf', 'images'),
    ];
    for (const call of calls) {
        assert.throws(call, (error: unknown) => {
            assert.ok(error instanceof OpenPdfError);
            assert.equal(error.code, 'capability/not-implemented');
            assert.ok(error.details.feature);
            return true;
        });
    }
});

test('the stable error code set is exported', () => {
    assert.deepEqual([...errorCodes], [
        'session/argument-invalid',
        'format/invalid',
        'format/unsupported',
        'io/read',
        'io/write',
        'io/exists',
        'capability/not-implemented',
    ]);
    assert.equal(new OpenPdfError('io/exists', 'exists').code, 'io/exists');
    assert.equal(OpenPdfError.prototype instanceof Error, true);
});

test('deletePages drops the selected pages', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'source.pdf');
        const out = path.join(dir, 'kept.pdf');
        await OpenPDF.create({ out: source, pages: ['One', 'Two', 'Three'], overwrite: true });
        const report = await OpenPDF.deletePages(source, '2', out);
        assert.equal(report.pages, 2);
        assert.equal(report.removed, 1);
        assert.ok(report.bytes > 0);
        assert.equal(OpenPDF.inspect(out).pages, 2);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('rotatePages adds rotation to the selected pages', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'source.pdf');
        const out = path.join(dir, 'rotated.pdf');
        await OpenPDF.create({ out: source, pages: ['One', 'Two'], overwrite: true });
        const report = await OpenPDF.rotatePages(source, '1', 90, out);
        assert.equal(report.pages, 2);
        assert.equal(report.rotated, 1);
        const sizes = OpenPDF.inspect(out).pageSizes;
        assert.equal(sizes[0].rotation, 90);
        assert.equal(sizes[1].rotation, 0);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('setPageBoxes writes a CropBox on the selected pages', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'source.pdf');
        const out = path.join(dir, 'cropped.pdf');
        await OpenPDF.create({ out: source, pages: ['One', 'Two'], overwrite: true });
        const report = await OpenPDF.setPageBoxes(source, '1', [10, 20, 300, 400], out);
        assert.equal(report.pages, 2);
        assert.equal(report.cropped, 1);
        assert.ok(report.bytes > 0);
        const sizes = OpenPDF.inspect(out).pageSizes;
        assert.equal(sizes[0].widthPt, 612);
        assert.equal(sizes[0].heightPt, 792);
        assert.ok((await readFile(out, 'latin1')).includes('/CropBox'));
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});
