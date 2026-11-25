/**
 * Smoke tests for the declared surface.
 *
 * They assert shell semantics only: one stable code when the addon is absent,
 * a format registry for the PDF family, and synchronous argument contracts.
 * Native PDF behaviour is covered by the Rust integration tests.
 */

import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
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
        () => OpenPDF.outline('input.pdf'),
        () => OpenPDF.metadata('input.pdf'),
        () => OpenPDF.merge(['a.pdf', 'b.pdf'], 'out.pdf'),
        () => OpenPDF.split('input.pdf', '1-2', 'out'),
        () => OpenPDF.splitEvery('input.pdf', 2, 'out'),
        () => OpenPDF.arrange('input.pdf', 'out.pdf', { order: [1] }),
        () => OpenPDF.deletePages('input.pdf', '1', 'out.pdf'),
        () => OpenPDF.insertPages('input.pdf', 'extra.pdf', 1, 'out.pdf'),
        () => OpenPDF.replacePages('input.pdf', '1', 'extra.pdf', 'out.pdf'),
        () => OpenPDF.reversePages('input.pdf', 'out.pdf'),
        () => OpenPDF.rotatePages('input.pdf', '1', 90, 'out.pdf'),
        () => OpenPDF.setPageBoxes('input.pdf', '1', [0, 0, 10, 10], 'out.pdf'),
        () => OpenPDF.setMetadata('input.pdf', 'out.pdf', { title: 'x' }),
        () => OpenPDF.setBookmarks('input.pdf', 'out.pdf', [{ title: 'Intro', page: 1 }]),
        () => OpenPDF.fillForm('input.pdf', 'out.pdf', { name: 'x' }),
        () => OpenPDF.create({ out: 'new.pdf', pages: ['hello'] }),
        () => OpenPDF.extractText('input.pdf'),
        () => OpenPDF.replaceText('input.pdf', 'draft', 'final', { out: 'out.pdf' }),
        () => OpenPDF.extractImages('input.pdf', 'images'),
        () => OpenPDF.listDestinations('input.pdf'),
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
        'binding/failure',
    ]);
    assert.equal(new OpenPdfError('io/exists', 'exists').code, 'io/exists');
    assert.equal(OpenPdfError.prototype instanceof Error, true);
});

test('binding failures surface their real code, never a format/invalid catch-all', { skip: withoutAddon }, () => {
    const dir = mkdtempSync(path.join(tmpdir(), 'open-pdf-'));
    try {
        // A missing file is an `io/read`, not the old `format/invalid` fallback.
        assert.throws(
            () => OpenPDF.inspect(path.join(dir, 'absent.pdf')),
            (error: unknown) => {
                assert.ok(error instanceof OpenPdfError);
                assert.equal(error.code, 'io/read');
                assert.notEqual(error.code, 'format/invalid');
                return true;
            },
        );

        // A rejected argument is `session/argument-invalid`, again not format/invalid.
        const source = path.join(dir, 'source.pdf');
        OpenPDF.create({ out: source, pages: ['One', 'Two'], overwrite: true });
        assert.throws(
            () => OpenPDF.rotatePages(source, '1', 45, path.join(dir, 'rotated.pdf')),
            (error: unknown) => {
                assert.ok(error instanceof OpenPdfError);
                assert.equal(error.code, 'session/argument-invalid');
                assert.notEqual(error.code, 'format/invalid');
                return true;
            },
        );
    } finally {
        rmSync(dir, { recursive: true, force: true });
    }
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

test('insertPages places the other document after the selected page', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'source.pdf');
        const extra = path.join(dir, 'extra.pdf');
        const out = path.join(dir, 'inserted.pdf');
        await OpenPDF.create({ out: source, pages: ['One', 'Two', 'Three'], overwrite: true });
        await OpenPDF.create({ out: extra, pages: ['Alpha'], overwrite: true });
        const report = await OpenPDF.insertPages(source, extra, 1, out);
        assert.equal(report.pages, 4);
        assert.equal(report.sourcePages, 3);
        assert.equal(report.insertedPages, 1);
        assert.equal(report.after, 1);
        assert.equal(OpenPDF.inspect(out).pages, 4);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('replacePages splices the other document into the selection', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'source.pdf');
        const extra = path.join(dir, 'extra.pdf');
        const out = path.join(dir, 'replaced.pdf');
        await OpenPDF.create({ out: source, pages: ['One', 'Two', 'Three'], overwrite: true });
        await OpenPDF.create({ out: extra, pages: ['Alpha'], overwrite: true });
        const report = await OpenPDF.replacePages(source, '2', extra, out);
        assert.equal(report.pages, 3);
        assert.deepEqual(report.replaced, [2]);
        assert.equal(report.replacementPages, 1);
        assert.equal(OpenPDF.inspect(out).pages, 3);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('replaceText rewrites the selection and reports what it could not address', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'draft.pdf');
        const out = path.join(dir, 'clean.pdf');
        await OpenPDF.create({ out: source, pages: ['DRAFT copy', 'DRAFT copy'], overwrite: true });
        const report = await OpenPDF.replaceText(source, 'DRAFT ', '', { pages: '2', out });
        assert.equal(report.pages, 2);
        assert.equal(report.pagesChanged, 1);
        assert.equal(report.replaced, 1);
        assert.deepEqual(report.replacements, [{ page: 2, replaced: 1 }]);
        assert.deepEqual(report.losses, []);
        assert.ok(report.bytes > 0);
        assert.match(OpenPDF.extractText(out).markdown, /DRAFT copy/);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('replaceText rejects an empty search text before touching the output', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'draft.pdf');
        const out = path.join(dir, 'never.pdf');
        await OpenPDF.create({ out: source, pages: ['One'], overwrite: true });
        assert.throws(
            () => OpenPDF.replaceText(source, '', 'x', { out }),
            (error: unknown) => {
                assert.ok(error instanceof OpenPdfError);
                assert.equal(error.code, 'session/argument-invalid');
                return true;
            },
        );
        assert.equal(existsSync(out), false);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('reversePages flips the page order', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'source.pdf');
        const out = path.join(dir, 'reversed.pdf');
        await OpenPDF.create({ out: source, pages: ['One', 'Two'], overwrite: true });
        const report = await OpenPDF.reversePages(source, out);
        assert.equal(report.pages, 2);
        assert.equal(report.sourcePages, 2);
        const text = OpenPDF.extractText(out).markdown;
        assert.ok(text.indexOf('Two') < text.indexOf('One'), text);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('page operations honour the selected slot and reject illegal selections', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'source.pdf');
        const insert = path.join(dir, 'insert.pdf');
        await OpenPDF.create({ out: source, pages: ['One', 'Two', 'Three', 'Four'], overwrite: true });
        await OpenPDF.create({ out: insert, pages: ['Alpha', 'Beta'], overwrite: true });

        // Omitting the selection (the SDK default `null`) reverses every page.
        const reversedAll = path.join(dir, 'reverse-all.pdf');
        const all = OpenPDF.reversePages(source, reversedAll);
        assert.deepEqual(all.reversed, [1, 2, 3, 4]);
        const allText = OpenPDF.extractText(reversedAll).markdown;
        assert.ok(allText.indexOf('Four') < allText.indexOf('One'), allText);

        // An explicit selection reverses only those pages; the rest keep their position.
        const reversedSome = path.join(dir, 'reverse-some.pdf');
        const some = OpenPDF.reversePages(source, reversedSome, false, '1,3');
        assert.deepEqual(some.reversed, [1, 3]);
        const someText = OpenPDF.extractText(reversedSome).markdown;
        assert.ok(
            someText.indexOf('Three') < someText.indexOf('Two') &&
            someText.indexOf('Two') < someText.indexOf('One') &&
            someText.indexOf('One') < someText.indexOf('Four'),
            someText,
        );

        // Duplicate, out-of-range and reversed selections are illegal arguments, not format errors.
        for (const bad of ['2,2', '9', '3-1']) {
            assert.throws(
                () => OpenPDF.reversePages(source, path.join(dir, 'bad.pdf'), false, bad),
                (error: unknown) => error instanceof OpenPdfError && error.code === 'session/argument-invalid',
            );
        }

        // replacePages uses the insert pages in the explicit selection order.
        const replaced = path.join(dir, 'replace-selected.pdf');
        const report = OpenPDF.replacePages(source, '2', insert, replaced, false, '2,1');
        assert.deepEqual(report.selectedPages, [2, 1]);
        assert.equal(report.pages, 5);
        const replacedText = OpenPDF.extractText(replaced).markdown;
        assert.ok(
            replacedText.indexOf('One') < replacedText.indexOf('Beta') &&
            replacedText.indexOf('Beta') < replacedText.indexOf('Alpha') &&
            replacedText.indexOf('Alpha') < replacedText.indexOf('Three'),
            replacedText,
        );
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

test('metadata reads the information dictionary', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'source.pdf');
        await OpenPDF.create({ out: source, pages: ['One'], title: 'Quarterly deck', overwrite: true });
        const report = OpenPDF.metadata(source);
        assert.equal(report.pages, 1);
        assert.equal(report.metadata.title, 'Quarterly deck');
        assert.equal(report.metadata.producer, 'OpenPDF');
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

test('outline drops entries deeper than the requested depth', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'source.pdf');
        const out = path.join(dir, 'bookmarked.pdf');
        await OpenPDF.create({ out: source, pages: ['One', 'Two'], overwrite: true });
        await OpenPDF.setBookmarks(source, out, [
            { title: 'Top', page: 1 },
            { title: 'Nested', page: 2, depth: 1 },
        ]);
        const full = OpenPDF.outline(out);
        assert.equal(full.count, 2);
        assert.equal(full.maxDepth, 2);
        const shallow = OpenPDF.outline(out, { depth: 1 });
        assert.equal(shallow.count, 1);
        assert.equal(shallow.maxDepth, 1);
        assert.deepEqual(shallow.bookmarks.map((bookmark) => bookmark.title), ['Top']);
        assert.throws(() => OpenPDF.outline(out, { depth: 0 }), RangeError);
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});

/** Mirrors the Rust `assemble` helper: sequential objects plus a classic xref. */
function assemblePdf(objects: string[]): Buffer {
    let output = '%PDF-1.7\n';
    const offsets: number[] = [];
    objects.forEach((body, index) => {
        offsets.push(Buffer.byteLength(output, 'latin1'));
        output += `${index + 1} 0 obj\n${body}\nendobj\n`;
    });
    const xref = Buffer.byteLength(output, 'latin1');
    output += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
    for (const offset of offsets) {
        output += `${String(offset).padStart(10, '0')} 00000 n \n`;
    }
    output += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
    return Buffer.from(output, 'latin1');
}

test('listDestinations wraps the named-destination array into a report', { skip: withoutAddon }, async () => {
    const dir = await mkdtemp(path.join(tmpdir(), 'open-pdf-'));
    try {
        const source = path.join(dir, 'source.pdf');
        await OpenPDF.create({ out: source, pages: ['One', 'Two'], overwrite: true });
        const empty = OpenPDF.listDestinations(source);
        assert.equal(empty.count, 0);
        assert.deepEqual(empty.destinations, []);

        const dests = path.join(dir, 'dests.pdf');
        writeFileSync(dests, assemblePdf([
            '<< /Type /Catalog /Pages 2 0 R /Dests 5 0 R >>',
            '<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>',
            '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >>',
            '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >>',
            '<< /Names [ (Intro) [3 0 R /Fit] (ChapterTwo) 4 0 R (Ghost) 9 0 R ] >>',
        ]));
        const report = OpenPDF.listDestinations(dests);
        assert.equal(report.count, 2);
        assert.deepEqual(report.destinations, [
            { name: 'ChapterTwo', page: 2 },
            { name: 'Intro', page: 1 },
        ]);

        assert.throws(
            () => OpenPDF.listDestinations(path.join(dir, 'absent.pdf')),
            (error: unknown) => error instanceof OpenPdfError && error.code === 'io/read',
        );
    } finally {
        await rm(dir, { recursive: true, force: true });
    }
});
