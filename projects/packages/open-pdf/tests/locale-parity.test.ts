/**
 * Locale parity contract.
 *
 * `en-US` and `zh-CN` must render the same machine codes and params for the
 * same failure; only the human copy may differ. Unknown codes keep their
 * machine code and fall back to the binding's own text in every locale.
 */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import {
    errorCodes,
    OpenPdfError,
    renderErrorCopy,
    setErrorLocale,
    type OpenPdfErrorCode,
    type OpenPdfErrorParams,
} from '../src/index.js';

const CLI = fileURLToPath(new URL('../../dist/cli.js', import.meta.url));

/** Params that let every stable code render fully in both locales. */
const sampleParams: Record<OpenPdfErrorCode, OpenPdfErrorParams> = {
    'session/argument-invalid': { reason: 'page 999 is beyond the last page' },
    'format/invalid': { reason: 'the xref table is damaged' },
    'format/unsupported': { reason: 'the encrypted PDF stream is not supported' },
    'io/read': { path: 'input.pdf', kind: 'NotFound' },
    'io/write': { path: 'output.pdf' },
    'io/exists': { path: 'output.pdf' },
    'capability/not-implemented': { feature: 'pdf_inspect' },
    'binding/failure': {},
};

test('every stable code renders locale-specific copy with identical params', () => {
    for (const code of errorCodes) {
        setErrorLocale('en');
        const en = renderErrorCopy(code, sampleParams[code], 'binding text');
        setErrorLocale('zh-CN');
        const zh = renderErrorCopy(code, sampleParams[code], 'binding text');
        assert.notEqual(en, 'binding text', `${code}: en copy must render from the catalog`);
        assert.notEqual(zh, 'binding text', `${code}: zh copy must render from the catalog`);
        assert.notEqual(en, zh, `${code}: rendered copy must differ between locales`);
        const enError = new OpenPdfError(code, en, { params: sampleParams[code] });
        const zhError = new OpenPdfError(code, zh, { params: sampleParams[code] });
        assert.equal(enError.code, zhError.code, `${code}: machine code must not depend on locale`);
        assert.deepEqual(enError.params, zhError.params, `${code}: params must not depend on locale`);
    }
    setErrorLocale('en');
});

test('an unknown code keeps its machine code and falls back in both locales', () => {
    setErrorLocale('en');
    const en = renderErrorCopy('weird/code', { reason: 'x' }, 'binding text');
    setErrorLocale('zh-CN');
    const zh = renderErrorCopy('weird/code', { reason: 'x' }, 'binding text');
    assert.equal(en, 'binding text');
    assert.equal(zh, 'binding text');
    const error = new OpenPdfError('weird/code' as OpenPdfErrorCode, 'binding text');
    assert.equal(error.code, 'weird/code');
    setErrorLocale('en');
});

test('machine branch and params stay stable while locale switches concurrently', async () => {
    const code = 'io/read' as const;
    const params = { path: 'input.pdf', kind: 'NotFound' };
    const observed = await Promise.all(
        Array.from({ length: 40 }, async (_unused, index) => {
            await Promise.resolve();
            setErrorLocale(index % 2 === 0 ? 'en-US' : 'zh-CN');
            const error = new OpenPdfError(code, renderErrorCopy(code, params, 'binding text'), { params });
            return { code: error.code, params: error.params };
        }),
    );
    for (const result of observed) {
        assert.equal(result.code, 'io/read', 'the machine code never depends on the locale');
        assert.deepEqual(result.params, params, 'the machine params never depend on the locale');
    }
    setErrorLocale('en');
});

interface Envelope {
    schema: string;
    ok: boolean;
    error?: { code: string; message: string };
}

function runCli(args: string[], cwd: string, env?: Record<string, string>): { status: number | null; envelope: Envelope } {
    const result = spawnSync(process.execPath, [CLI, ...args], {
        cwd,
        encoding: 'utf8',
        env: env === undefined ? process.env : { ...process.env, ...env },
    });
    assert.equal(result.error, undefined, `spawn failed: ${result.error?.message}`);
    const lines = result.stdout.trimEnd().split(/\r?\n/).filter((line) => line.length > 0);
    assert.equal(lines.length, 1, `expected one stdout line, got ${JSON.stringify(lines)}`);
    return { status: result.status, envelope: JSON.parse(lines[0] ?? '') as Envelope };
}

test('the CLI envelope keeps codes and exit codes identical across locales', (t) => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'open-pdf-parity-'));
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    const en = runCli(['inspect', '--json', '--locale', 'en-US'], dir);
    const zh = runCli(['inspect', '--json', '--locale', 'zh-CN'], dir);
    assert.equal(en.status, 1);
    assert.equal(zh.status, en.status);
    assert.equal(en.envelope.schema, 'wae.cli-result/v0');
    assert.equal(zh.envelope.schema, 'wae.cli-result/v0');
    assert.equal(zh.envelope.error?.code, en.envelope.error?.code);
    assert.equal(en.envelope.error?.code, 'open-pdf.cli.input-required');
    assert.notEqual(zh.envelope.error?.message, en.envelope.error?.message);
});

test('SDK failures relay locale copy through the envelope', (t) => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'open-pdf-parity-'));
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    const stub = path.join(dir, 'binding.cjs');
    fs.writeFileSync(
        stub,
        `module.exports = {
    pdf_inspect() {
        const error = new Error('could not read \`stub.pdf\`: NotFound');
        error.code = 'io/read';
        error.detail = { params: { path: 'stub.pdf', kind: 'NotFound' } };
        throw error;
    },
};
`,
    );
    const env = { OPEN_PDF_NATIVE: stub };
    const en = runCli(['inspect', 'stub.pdf', '--json', '--locale', 'en-US'], dir, env);
    const zh = runCli(['inspect', 'stub.pdf', '--json', '--locale', 'zh-CN'], dir, env);
    assert.equal(en.status, 1);
    assert.equal(zh.status, en.status);
    assert.equal(en.envelope.error?.code, 'io/read');
    assert.equal(zh.envelope.error?.code, en.envelope.error?.code);
    assert.notEqual(zh.envelope.error?.message, en.envelope.error?.message);
    assert.match(en.envelope.error?.message ?? '', /could not read/);
    assert.match(zh.envelope.error?.message ?? '', /无法读取/);
});
