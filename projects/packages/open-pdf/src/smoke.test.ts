/**
 * Smoke tests for the declared surface.
 *
 * They assert shell semantics only: one stable code when the addon is absent,
 * a format registry for the PDF family, and synchronous argument contracts.
 * Native PDF behaviour is covered by the Rust integration tests.
 */

import assert from 'node:assert/strict';
import test from 'node:test';

import { OpenPDF, OpenPdfError, documentFormats, errorCodes, formatFromPath, formatInfo, isPdfPath } from './index.js';
import { nativeBinding } from './native.js';

const installed = (() => {
    try {
        return nativeBinding() !== undefined;
    } catch {
        return false;
    }
})();
const withAddon = installed ? 'a built addon is installed' : false;

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

test('without an installed addon every engine call reports capability/not-implemented', { skip: withAddon }, () => {
    const calls: (() => unknown)[] = [
        () => OpenPDF.capabilities(),
        () => OpenPDF.inspect('input.pdf'),
        () => OpenPDF.merge(['a.pdf', 'b.pdf'], 'out.pdf'),
        () => OpenPDF.split('input.pdf', '1-2', 'out'),
        () => OpenPDF.splitEvery('input.pdf', 2, 'out'),
        () => OpenPDF.arrange('input.pdf', 'out.pdf', { order: [1] }),
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
