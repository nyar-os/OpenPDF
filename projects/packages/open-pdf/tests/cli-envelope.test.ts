/**
 * CLI machine-envelope contract.
 *
 * Spawns the built CLI with `--json` and asserts the `wae.cli-result/v0`
 * envelope on stdout, stable error codes (never `commander::action_failed`
 * for SDK failures) and the documented exit codes.
 */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { nativeBinding } from '../src/native.js';

const CLI = fileURLToPath(new URL('../../dist/cli.js', import.meta.url));
const installed = nativeBinding() !== undefined;

interface Envelope {
    schema: string;
    ok: boolean;
    data?: unknown;
    error?: { code: string; message: string };
}

function runCli(args: string[], cwd: string): { status: number | null; stdout: string; envelope: Envelope } {
    const result = spawnSync(process.execPath, [CLI, ...args], { cwd, encoding: 'utf8' });
    assert.equal(result.error, undefined, `spawn failed: ${result.error?.message}`);
    const lines = result.stdout.trimEnd().split(/\r?\n/).filter((line) => line.length > 0);
    assert.equal(lines.length, 1, `expected exactly one stdout line, got ${JSON.stringify(lines)}`);
    const envelope = JSON.parse(lines[0] ?? '') as Envelope;
    return { status: result.status, stdout: result.stdout, envelope };
}

function scratchDir(): string {
    return fs.mkdtempSync(path.join(os.tmpdir(), 'open-pdf-cli-'));
}

test('`--json` success path emits the cli-result/v0 envelope', { skip: !installed }, (t) => {
    const dir = scratchDir();
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    const run = runCli(['create', '-o', 'src.pdf', '--page', 'One', '--page', 'Two', '--page', 'Three', '--json'], dir);
    assert.equal(run.status, 0);
    assert.equal(run.envelope.schema, 'wae.cli-result/v0');
    assert.equal(run.envelope.ok, true);
    const data = run.envelope.data as { path: string; pages: number; bytes: number };
    assert.equal(data.path, 'src.pdf');
    assert.equal(data.pages, 3);
    assert.ok(data.bytes > 0);
});

test('`replace-text` rewrites the selection and rejects an empty search text', { skip: !installed }, (t) => {
    const dir = scratchDir();
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    runCli(['create', '-o', 'src.pdf', '--page', 'DRAFT one', '--page', 'DRAFT two', '--json'], dir);
    const run = runCli(['replace-text', 'src.pdf', 'DRAFT ', '-o', 'clean.pdf', '--json'], dir);
    assert.equal(run.status, 0);
    assert.equal(run.envelope.ok, true);
    const data = run.envelope.data as { replaced: number; pagesChanged: number; losses: unknown[]; fidelity: unknown[] };
    assert.equal(data.replaced, 2);
    assert.equal(data.pagesChanged, 2);
    assert.deepEqual(data.losses, []);
    assert.deepEqual(data.fidelity, []);

    const empty = runCli(['replace-text', 'src.pdf', '', '-o', 'never.pdf', '--json'], dir);
    assert.equal(empty.status, 1);
    assert.equal(empty.envelope.error?.code, 'open-pdf.cli.replace-args');
    assert.equal(fs.existsSync(path.join(dir, 'never.pdf')), false);
});

test('a missing file relays the stable `io/read` code', { skip: !installed }, (t) => {
    const dir = scratchDir();
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    const run = runCli(['inspect', 'absent.pdf', '--json', '--locale', 'en-US'], dir);
    assert.equal(run.status, 1);
    assert.equal(run.envelope.ok, false);
    assert.equal(run.envelope.error?.code, 'io/read');
    assert.ok((run.envelope.error?.message ?? '').length > 0);
});

test('a page range beyond the document relays `session/argument-invalid`', { skip: !installed }, (t) => {
    const dir = scratchDir();
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    runCli(['create', '-o', 'src.pdf', '--page', 'One', '--page', 'Two', '--page', 'Three', '--json'], dir);
    const run = runCli(['delete-pages', 'src.pdf', '--pages', '999', '-o', 'out.pdf', '--json'], dir);
    assert.equal(run.status, 1);
    assert.equal(run.envelope.ok, false);
    assert.equal(run.envelope.error?.code, 'session/argument-invalid');
    const selection = runCli(['extract-text', 'src.pdf', '--pages', '999', '--json', '--locale', 'en-US'], dir);
    assert.equal(selection.envelope.error?.code, 'session/argument-invalid');
    assert.match(
        selection.envelope.error?.message ?? '',
        /the request was rejected: page range `999`/,
        'the typed reason renders through the envelope',
    );
});

test('an existing target without `--overwrite` relays `io/exists`', { skip: !installed }, (t) => {
    const dir = scratchDir();
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    runCli(['create', '-o', 'src.pdf', '--page', 'One', '--page', 'Two', '--page', 'Three', '--json'], dir);
    const blocked = runCli(['delete-pages', 'src.pdf', '--pages', '1', '-o', 'src.pdf', '--json'], dir);
    assert.equal(blocked.status, 1);
    assert.equal(blocked.envelope.error?.code, 'io/exists');
    assert.match(blocked.envelope.error?.message ?? '', /src\.pdf/, 'the blocked path reaches the envelope');
    const forced = runCli(['delete-pages', 'src.pdf', '--pages', '1', '-o', 'src.pdf', '--overwrite', '--json'], dir);
    assert.equal(forced.status, 0);
    assert.equal(forced.envelope.ok, true);
});

test('CLI validation failures carry `open-pdf.cli.*` codes and exit 1', (t) => {
    const dir = scratchDir();
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    const missing = runCli(['inspect', '--json', '--locale', 'en-US'], dir);
    assert.equal(missing.status, 1);
    assert.equal(missing.envelope.error?.code, 'open-pdf.cli.input-required');
    const malformed = runCli(['delete-pages', 'absent.pdf', '--pages', '1-2-3', '-o', 'out.pdf', '--json'], dir);
    assert.equal(malformed.status, 1);
    assert.equal(malformed.envelope.error?.code, 'open-pdf.cli.pages-args');
});

test('parse failures exit 2 with a `commander::` code', (t) => {
    const dir = scratchDir();
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    const run = runCli(['--bogus'], dir);
    assert.equal(run.status, 2);
    assert.equal(run.envelope.ok, false);
    assert.equal(run.envelope.error?.code, 'commander::unknown_command');
});

test('human mode never prints the envelope on stdout', (t) => {
    const dir = scratchDir();
    t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
    const result = spawnSync(process.execPath, [CLI, 'inspect', '--locale', 'en-US'], { cwd: dir, encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.ok(!result.stdout.includes('wae.cli-result/v0'));
});
