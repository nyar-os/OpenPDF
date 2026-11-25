/**
 * Error contract checks against a stub binding.
 *
 * The stub replaces the native display text on purpose: the SDK must still
 * branch on the machine-readable `code`, never on prose. Unknown codes, missing
 * translations, broken payloads and the legacy `code: message` adapter all run
 * through the same path.
 */

import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import { isOpenPdfError, setErrorLocale, type OpenPdfError } from '../src/errors.js';
import * as service from '../src/service.js';

const dir = mkdtempSync(join(tmpdir(), 'open-pdf-stub-'));
const stubPath = join(dir, 'stub-binding.cjs');
writeFileSync(
    stubPath,
    [
        'function failure() {',
        '    const next = JSON.parse(process.env.OPEN_PDF_STUB || "{}");',
        '    const error = new Error(next.message || "the native text was replaced");',
        '    if (next.code !== undefined) { error.code = next.code; }',
        '    if (next.detail !== undefined) { error.detail = next.detail; }',
        '    return error;',
        '}',
        'module.exports = {',
        '    version() { return "0.0.0-stub"; },',
        '    pdf_inspect() { throw failure(); },',
        '};',
    ].join('\n'),
);
process.env.OPEN_PDF_NATIVE = stubPath;

function capture(): OpenPdfError {
    try {
        service.inspect('missing.pdf');
    } catch (error) {
        assert.ok(isOpenPdfError(error), 'every service failure is an OpenPdfError');
        return error;
    }
    return assert.fail('the stub always throws') as never;
}

test('structured binding failures keep their verifiable code', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({
        message: 'REPLACED NATIVE TEXT',
        code: 'io/read',
        detail: { params: { path: 'missing.pdf', kind: 'NotFound' } },
    });
    const error = capture();
    assert.equal(error.code, 'io/read');
    assert.deepEqual(error.params, { path: 'missing.pdf', kind: 'NotFound' });
    assert.equal(error.message, 'could not read `missing.pdf` (NotFound)', 'the product copy replaces the native text');
});

test('unknown codes are preserved instead of being misclassified', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({ message: 'native text kept', code: 'weird/code' });
    const error = capture();
    assert.equal(error.code, 'weird/code');
    assert.equal(error.message, 'native text kept', 'a missing translation falls back to the binding text');
});

test('a broken params payload degrades to an empty parameter map', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({ message: 'disk said no', code: 'io/exists', detail: { params: 'not-an-object' } });
    const error = capture();
    assert.equal(error.code, 'io/exists');
    assert.deepEqual(error.params, {});
    assert.equal(error.message, 'disk said no', 'an unrenderable template keeps the binding text');
});

test('io/exists renders localized copy from the typed `path` param', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({
        message: 'REPLACED NATIVE TEXT',
        code: 'io/exists',
        detail: { params: { path: 'out.pdf' } },
    });
    const error = capture();
    assert.equal(error.code, 'io/exists');
    assert.deepEqual(error.params, { path: 'out.pdf' });
    assert.equal(error.message, '`out.pdf` already exists; pass overwrite to replace it');
});

test('session/argument-invalid renders the typed `reason`', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({
        message: 'REPLACED NATIVE TEXT',
        code: 'session/argument-invalid',
        detail: { params: { reason: 'page range `999` is outside the 3-page document', pages: '999' } },
    });
    const error = capture();
    assert.equal(error.code, 'session/argument-invalid');
    assert.deepEqual(error.params, { reason: 'page range `999` is outside the 3-page document', pages: '999' });
    assert.equal(error.message, 'the request was rejected: page range `999` is outside the 3-page document');
});

test('a legacy `code: message` text is no longer decoded into a code', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({ message: 'format/unsupported: old binding text' });
    const error = capture();
    assert.equal(error.code, 'binding/failure', 'the message is never parsed for a code');
    assert.equal(error.message, 'format/unsupported: old binding text');
});

test('a payload with an extra key is withheld', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({
        message: 'disk said no',
        code: 'io/exists',
        detail: { params: { path: 'out.pdf', overwritten: true } },
    });
    const error = capture();
    assert.equal(error.code, 'io/exists');
    assert.deepEqual(error.params, {});
    assert.equal(error.failure, undefined);
    assert.equal(error.message, 'disk said no');
});

test('a payload missing a required key is withheld', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({
        message: 'disk said no',
        code: 'io/read',
        detail: { params: { path: 'input.pdf' } },
    });
    const error = capture();
    assert.equal(error.code, 'io/read');
    assert.deepEqual(error.params, {});
    assert.equal(error.failure, undefined);
});

test('a payload with a mistyped key is withheld', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({
        message: 'disk said no',
        code: 'io/read',
        detail: { params: { path: 'input.pdf', kind: 7 } },
    });
    const error = capture();
    assert.equal(error.code, 'io/read');
    assert.deepEqual(error.params, {});
    assert.equal(error.failure, undefined);
});

test('a valid payload exposes the machine branch next to the copy', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({
        message: 'REPLACED NATIVE TEXT',
        code: 'io/exists',
        detail: { params: { path: 'out.pdf' } },
    });
    const error = capture();
    assert.deepEqual(error.failure, { code: 'io/exists', params: { path: 'out.pdf' } });
});

test('a failure without any code surfaces as binding/failure', () => {
    process.env.OPEN_PDF_STUB = JSON.stringify({ message: 'boom' });
    const error = capture();
    assert.equal(error.code, 'binding/failure');
    assert.equal(error.message, 'boom');
});

test('zh-CN rendering changes only the copy, never the machine fields', () => {
    setErrorLocale('zh-CN');
    try {
        process.env.OPEN_PDF_STUB = JSON.stringify({
            message: 'REPLACED NATIVE TEXT',
            code: 'io/read',
            detail: { params: { path: 'missing.pdf', kind: 'NotFound' } },
        });
        const error = capture();
        assert.equal(error.code, 'io/read');
        assert.deepEqual(error.params, { path: 'missing.pdf', kind: 'NotFound' });
        assert.equal(error.message, '无法读取 `missing.pdf`（NotFound）');
    } finally {
        setErrorLocale('en');
    }
});
