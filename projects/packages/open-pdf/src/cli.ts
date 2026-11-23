#!/usr/bin/env node
/**
 * Command-line assembly only.
 *
 * Every command maps options onto one method of `new OpenPDF()`; PDF semantics
 * stay native. Help text and errors come from `@wae/commander`, so the CLI does
 * not grow its own parser.
 */

import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { createCli, type ParsedOptions } from '@wae/commander';

import { OpenPDF } from './index.js';
import type {
    Bookmark,
    CropBox,
    ExtractReport,
    ExtractedImage,
    FormValue,
    PdfCapabilities,
    PdfInspectReport,
    SplitReport,
} from './types.js';

const here = path.dirname(fileURLToPath(import.meta.url));

const cli = createCli('open-pdf').locales(path.join(here, '..', 'locales')).intro('cli.intro');

/** One session per process: the CLI is a consumer of the public API. */
const pdf = new OpenPDF();

export { cli };

const text = (options: ParsedOptions, key: string): string | undefined =>
    typeof options[key] === 'string' ? (options[key] as string) : undefined;

const flag = (options: ParsedOptions, key: string): boolean => options[key] === true || options[key] === 'true';

/** Reads an optional decimal option. `null` means the text is malformed. */
const decimal = (options: ParsedOptions, key: string): number | undefined | null => {
    const value = text(options, key);
    if (value === undefined) {
        return undefined;
    }
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
};

const list = (options: ParsedOptions, key: string): string[] => {
    const value = options[key];
    if (Array.isArray(value)) {
        return value;
    }
    if (typeof value === 'string') {
        return [value];
    }
    return [];
};

/** Parses a comma-separated integer list. `null` means the text is malformed. */
const integers = (options: ParsedOptions, key: string): number[] | undefined | null => {
    const value = text(options, key);
    if (value === undefined) {
        return undefined;
    }
    const parts = value.split(',').map((part) => part.trim()).filter((part) => part.length > 0);
    if (parts.length === 0) {
        return null;
    }
    const parsed = parts.map(Number);
    return parsed.every((entry) => Number.isSafeInteger(entry)) ? parsed : null;
};

const input = (options: ParsedOptions): string | undefined => options._?.[0];

/** Reads standard input as raw PDF bytes. `undefined` means the read failed. */
async function stdinBytes(): Promise<Uint8Array | undefined> {
    try {
        const chunks: Buffer[] = [];
        for await (const chunk of process.stdin) {
            chunks.push(chunk as Buffer);
        }
        return Buffer.concat(chunks);
    } catch {
        return undefined;
    }
}

/** Parses a 1-based page range list such as `1-3,5`. `null` means malformed. */
const pageRanges = (options: ParsedOptions, key: string): string | undefined | null => {
    const value = text(options, key);
    if (value === undefined) {
        return undefined;
    }
    const parts = value.split(',').map((part) => part.trim()).filter((part) => part.length > 0);
    if (parts.length === 0) {
        return null;
    }
    for (const part of parts) {
        const bounds = part.split('-');
        if (bounds.length > 2 || !bounds.every((bound) => /^\d+$/.test(bound))) {
            return null;
        }
        const start = Number(bounds[0]);
        const end = bounds.length === 1 ? start : Number(bounds[1]);
        if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 1 || end < start) {
            return null;
        }
    }
    return parts.join(',');
};

/** Parses `l,b,r,t` crop corners. `null` means malformed. */
const cropBox = (options: ParsedOptions, key: string): CropBox | undefined | null => {
    const value = text(options, key);
    if (value === undefined) {
        return undefined;
    }
    const parts = value.split(',').map((part) => part.trim()).filter((part) => part.length > 0);
    if (parts.length !== 4) {
        return null;
    }
    const parsed = parts.map(Number);
    if (!parsed.every((entry) => Number.isFinite(entry))) {
        return null;
    }
    return [parsed[0], parsed[1], parsed[2], parsed[3]];
};

function describeCapabilities(matrix: PdfCapabilities): string {
    const pdf = matrix.formats.pdf;
    const operations = Object.entries(matrix.operations)
        .map(([name, level]) => `${name}=${level}`)
        .join(', ');
    const checks = Object.entries(matrix.checks)
        .map(([name, level]) => `${name}=${level}`)
        .join(', ');
    const renderers = Object.entries(matrix.renderers)
        .map(([name, level]) => `${name}=${level}`)
        .join(', ');
    const structures = Object.entries(pdf.structures)
        .map(([name, level]) => `${name}=${level}`)
        .join(', ');
    const filters = Object.entries(pdf.filters)
        .map(([name, level]) => `${name}=${level}`)
        .join(', ');
    return [
        `engine: ${matrix.engine.package} ${matrix.engine.version}`,
        `pdf: import=${pdf.import} extract=${pdf.extract} export=${pdf.export}`,
        `structures: ${structures}`,
        `filters: ${filters}`,
        `operations: ${operations}`,
        `checks: ${checks}`,
        `renderers: ${renderers}`,
    ].join('\n');
}

function describeInspect(report: PdfInspectReport): string {
    const lines = [
        `${report.label} version=${report.version} pages=${report.pages} encrypted=${report.encrypted}`,
        `title: ${report.metadata.title ?? '(none)'}`,
        `author: ${report.metadata.author ?? '(none)'}`,
        `subject: ${report.metadata.subject ?? '(none)'}`,
        `page sizes: ${report.pageSizes.map((page) => `${page.widthPt}x${page.heightPt}${page.rotation === 0 ? '' : ` rot=${page.rotation}`}`).join(', ')}`,
    ];
    if (report.bookmarks.length > 0) {
        lines.push('bookmarks:');
        for (const bookmark of report.bookmarks) {
            lines.push(`${'  '.repeat(Math.max(0, bookmark.depth - 1))}${bookmark.title}`);
        }
    }
    for (const field of report.formFields) {
        const flags = [field.readOnly ? 'readonly' : '', field.required ? 'required' : ''].filter(Boolean).join(' ');
        lines.push(`field ${field.name} type=${field.type ?? '(none)'} value=${field.value ?? '(none)'}${flags ? ` ${flags}` : ''}`);
    }
    for (const warning of report.warnings) {
        lines.push(`warning: ${warning}`);
    }
    return lines.join('\n');
}

cli
    .command('open', 'cli.cmd.open')
    .option('--print', 'cli.opt.print')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        const opener = process.platform === 'win32' ? 'cmd /C start' : process.platform === 'darwin' ? 'open' : 'xdg-open';
        if (flag(options, 'print')) {
            ctx.emitResult({ path: file, opener, opened: false }, `would open ${file}`);
            return 0;
        }
        if (!fs.existsSync(file)) {
            ctx.fail('open-pdf.cli.file-not-found', { message: `${file} does not exist` });
            return 1;
        }
        const result =
            process.platform === 'win32'
                ? spawnSync('cmd', ['/C', 'start', '', file], { stdio: 'ignore' })
                : spawnSync(opener, [file], { stdio: 'ignore' });
        if (result.error || result.status !== 0) {
            const detail = result.error?.message ?? `exit ${result.status ?? 'unknown'}`;
            ctx.fail('open-pdf.cli.open-failed', { message: `cannot open ${file} with ${opener}: ${detail}` });
            return 1;
        }
        ctx.emitResult({ path: file, opener, opened: true }, `opened ${file}`);
        return 0;
    });

cli.command('capabilities', 'cli.cmd.capabilities').actionCtx(async (_options, _args, ctx) => {
    const matrix = await pdf.capabilities();
    ctx.emitResult(matrix, describeCapabilities(matrix));
    return 0;
});

cli
    .command('inspect', 'cli.cmd.inspect')
    .option('--stdin', 'cli.opt.stdin')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const useStdin = flag(options, 'stdin');
        if ((file === undefined) === !useStdin) {
            ctx.fail('open-pdf.cli.input-required', { message: 'pass one PDF path or `--stdin`, not both' });
            return 2;
        }
        let report: PdfInspectReport;
        if (useStdin) {
            const bytes = await stdinBytes();
            if (bytes === undefined || bytes.length === 0) {
                ctx.fail('open-pdf.cli.stdin-empty', { message: 'no PDF bytes arrived on standard input' });
                return 2;
            }
            report = await pdf.inspectBytes(bytes);
        }
        else {
            report = await pdf.inspect(file as string);
        }
        ctx.emitResult(report, describeInspect(report));
        return 0;
    });

cli
    .command('outline', 'cli.cmd.outline')
    .option('--depth <n>', 'cli.opt.outline-depth')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        const depth = decimal(options, 'depth');
        if (depth === null || (depth !== undefined && (!Number.isInteger(depth) || depth < 1))) {
            ctx.fail('open-pdf.cli.depth-invalid', { message: '`--depth` must be a positive integer' });
            return 2;
        }
        const report = await pdf.outline(file, { depth });
        const human = report.count === 0
            ? `${report.path} pages=${report.pages} (no outline)`
            : [
                  `${report.path} pages=${report.pages} entries=${report.count} maxDepth=${report.maxDepth}`,
                  ...report.bookmarks.map((bookmark) => `${'  '.repeat(Math.max(0, bookmark.depth - 1))}${bookmark.title}`),
              ].join('\n');
        ctx.emitResult(report, human);
        return 0;
    });

cli
    .command('merge', 'cli.cmd.merge')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--title <title>', 'cli.opt.title')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const inputs = options._ ?? [];
        const out = text(options, 'out');
        if (inputs.length === 0) {
            ctx.fail('open-pdf.cli.inputs-required', { message: 'at least one input PDF is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        const report = await pdf.merge(inputs, out, {
            overwrite: flag(options, 'overwrite'),
            title: text(options, 'title'),
        });
        ctx.emitResult(
            report,
            `merged ${inputs.length} file(s) -> ${report.path} pages=${report.pages} bytes=${report.bytes}`,
        );
        return 0;
    });

cli
    .command('split', 'cli.cmd.split')
    .option('--ranges <list>', 'cli.opt.ranges')
    .option('--every <count>', 'cli.opt.every')
    .option('-o, --out-dir <dir>', 'cli.opt.out-dir')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const ranges = text(options, 'ranges');
        const every = text(options, 'every');
        const outDir = text(options, 'out-dir');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (outDir === undefined) {
            ctx.fail('open-pdf.cli.split-args', {
                message: '`--out-dir <dir>` is required',
            });
            return 2;
        }
        if ((ranges === undefined) === (every === undefined)) {
            ctx.fail('open-pdf.cli.split-args', {
                message: 'exactly one of `--ranges <list>` and `--every <count>` is required',
            });
            return 2;
        }
        let report: SplitReport;
        if (every !== undefined) {
            const count = Number(every);
            if (!Number.isSafeInteger(count) || count < 1) {
                ctx.fail('open-pdf.cli.split-args', { message: '`--every <count>` must be a positive integer' });
                return 2;
            }
            report = await pdf.splitEvery(file, count, outDir);
        } else {
            report = await pdf.split(file, ranges as string, outDir);
        }
        ctx.emitResult(
            report,
            report.documents.map((part) => `${part.path} pages=${part.pages}`).join('\n'),
        );
        return 0;
    });

cli
    .command('arrange', 'cli.cmd.arrange')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--order <list>', 'cli.opt.order')
    .option('--remove <list>', 'cli.opt.remove')
    .option('--rotate <degrees>', 'cli.opt.rotate')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        const order = integers(options, 'order');
        const remove = integers(options, 'remove');
        if (order === null || remove === null) {
            ctx.fail('open-pdf.cli.arrange-args', {
                message: '`--order` and `--remove` must be comma-separated page numbers',
            });
            return 2;
        }
        const rotateText = text(options, 'rotate');
        let rotate: number | undefined;
        if (rotateText !== undefined) {
            rotate = Number(rotateText);
            if (!Number.isSafeInteger(rotate)) {
                ctx.fail('open-pdf.cli.arrange-args', { message: '`--rotate <degrees>` must be an integer' });
                return 2;
            }
        }
        const report = await pdf.arrange(file, out, { order, rotate, remove });
        ctx.emitResult(report, `arranged ${report.path} pages=${report.pages}`);
        return 0;
    });

cli
    .command('delete-pages', 'cli.cmd.delete-pages')
    .option('--pages <list>', 'cli.opt.pages')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        const pages = pageRanges(options, 'pages');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        if (pages === undefined || pages === null) {
            ctx.fail('open-pdf.cli.pages-args', {
                message: '`--pages <list>` must be 1-based ranges such as 1-3,5',
            });
            return 2;
        }
        const report = await pdf.deletePages(file, pages, out, flag(options, 'overwrite'));
        ctx.emitResult(
            report,
            `deleted ${report.path} pages=${report.pages} removed=${report.removed} bytes=${report.bytes}`,
        );
        return 0;
    });

cli
    .command('duplicate-pages', 'cli.cmd.duplicate-pages')
    .option('--pages <list>', 'cli.opt.pages')
    .option('--times <count>', 'cli.opt.times')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        const pages = pageRanges(options, 'pages');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        if (pages === undefined || pages === null) {
            ctx.fail('open-pdf.cli.pages-args', {
                message: '`--pages <list>` must be 1-based ranges such as 1-3,5',
            });
            return 2;
        }
        const rawTimes = text(options, 'times');
        const times = rawTimes === undefined ? 2 : Number(rawTimes);
        if (!Number.isSafeInteger(times) || times < 2) {
            ctx.fail('open-pdf.cli.times-args', { message: '`--times <count>` must be an integer of at least 2' });
            return 2;
        }
        const report = await pdf.duplicatePages(file, pages, times, out, flag(options, 'overwrite'));
        ctx.emitResult(report, `duplicated ${report.path} pages=${report.pages} times=${report.times}`);
        return 0;
    });

cli
    .command('insert-pages', 'cli.cmd.insert-pages')
    .option('--insert <file>', 'cli.opt.insert')
    .option('--after <page>', 'cli.opt.after')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        const insert = text(options, 'insert');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        if (insert === undefined || insert.length === 0) {
            ctx.fail('open-pdf.cli.insert-args', { message: '`--insert <file>` is required' });
            return 2;
        }
        const afterText = text(options, 'after');
        const after = afterText === undefined ? Number.NaN : Number(afterText);
        if (!Number.isSafeInteger(after) || after < 0) {
            ctx.fail('open-pdf.cli.after-args', { message: '`--after <page>` must be an integer of at least 0' });
            return 2;
        }
        const report = await pdf.insertPages(file, insert, after, out, flag(options, 'overwrite'));
        ctx.emitResult(
            report,
            `inserted ${report.path} pages=${report.pages} after=${report.after} insertedPages=${report.insertedPages}`,
        );
        return 0;
    });

cli
    .command('replace-pages', 'cli.cmd.replace-pages')
    .option('--pages <list>', 'cli.opt.pages')
    .option('--insert <file>', 'cli.opt.insert')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        const pages = pageRanges(options);
        const insert = text(options, 'insert');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        if (pages === undefined || pages.length === 0) {
            ctx.fail('open-pdf.cli.pages-args', { message: '`--pages <list>` must be a comma-separated 1-based page list' });
            return 2;
        }
        if (insert === undefined || insert.length === 0) {
            ctx.fail('open-pdf.cli.insert-args', { message: '`--insert <file>` is required' });
            return 2;
        }
        const report = await pdf.replacePages(file, pages, insert, out, flag(options, 'overwrite'));
        ctx.emitResult(
            report,
            `replaced ${report.path} pages=${report.pages} replaced=${formatPageList(report.replaced)} replacementPages=${report.replacementPages}`,
        );
        return 0;
    });

cli
    .command('reverse-pages', 'cli.cmd.reverse-pages')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        const report = await pdf.reversePages(file, out, flag(options, 'overwrite'));
        ctx.emitResult(report, `reversed ${report.path} pages=${report.pages}`);
        return 0;
    });

cli
    .command('extract-pages', 'cli.cmd.extract-pages')
    .option('--pages <list>', 'cli.opt.pages')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        const pages = pageRanges(options, 'pages');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        if (pages === undefined || pages === null) {
            ctx.fail('open-pdf.cli.pages-args', {
                message: '`--pages <list>` must be 1-based ranges such as 1-3,5',
            });
            return 2;
        }
        const report = await pdf.keepPages(file, pages, out, flag(options, 'overwrite'));
        ctx.emitResult(
            report,
            `kept ${report.path} pages=${report.pages} removed=${report.removed} bytes=${report.bytes}`,
        );
        return 0;
    });

cli
    .command('rotate', 'cli.cmd.rotate')
    .option('--pages <list>', 'cli.opt.pages')
    .option('--degrees <90|180|270>', 'cli.opt.degrees')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        const pages = pageRanges(options, 'pages');
        const degreesText = text(options, 'degrees');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        if (pages === undefined || pages === null) {
            ctx.fail('open-pdf.cli.pages-args', {
                message: '`--pages <list>` must be 1-based ranges such as 1-3,5',
            });
            return 2;
        }
        const degrees = degreesText === undefined ? Number.NaN : Number(degreesText);
        if (degrees !== 90 && degrees !== 180 && degrees !== 270) {
            ctx.fail('open-pdf.cli.degrees-args', { message: '`--degrees` must be 90, 180 or 270' });
            return 2;
        }
        const report = await pdf.rotatePages(file, pages, degrees, out, flag(options, 'overwrite'));
        ctx.emitResult(
            report,
            `rotated ${report.path} pages=${report.pages} rotated=${report.rotated} degrees=${degrees} bytes=${report.bytes}`,
        );
        return 0;
    });

cli
    .command('set-page-boxes', 'cli.cmd.set-page-boxes')
    .option('--pages <list>', 'cli.opt.pages')
    .option('--box <l,b,r,t>', 'cli.opt.box')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        const pages = pageRanges(options, 'pages');
        const box = cropBox(options, 'box');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        if (pages === undefined || pages === null) {
            ctx.fail('open-pdf.cli.pages-args', {
                message: '`--pages <list>` must be 1-based ranges such as 1-3,5',
            });
            return 2;
        }
        if (box === undefined || box === null) {
            ctx.fail('open-pdf.cli.box-args', {
                message: '`--box <l,b,r,t>` must be four finite numbers',
            });
            return 2;
        }
        const report = await pdf.setPageBoxes(file, pages, box, out, flag(options, 'overwrite'));
        ctx.emitResult(
            report,
            `boxed ${report.path} pages=${report.pages} cropped=${report.cropped} box=${box.join(',')} bytes=${report.bytes}`,
        );
        return 0;
    });

cli
    .command('set-metadata', 'cli.cmd.set-metadata')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--title <text>', 'cli.opt.title')
    .option('--author <text>', 'cli.opt.author')
    .option('--subject <text>', 'cli.opt.subject')
    .option('--keywords <text>', 'cli.opt.keywords')
    .option('--creator <text>', 'cli.opt.creator')
    .option('--producer <text>', 'cli.opt.producer')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        const report = await pdf.setMetadata(file, out, {
            title: text(options, 'title'),
            author: text(options, 'author'),
            subject: text(options, 'subject'),
            keywords: text(options, 'keywords'),
            creator: text(options, 'creator'),
            producer: text(options, 'producer'),
        });
        const metadata = report.metadata;
        const human = [
            `updated ${report.path} pages=${report.pages}`,
            `title: ${metadata.title ?? '(none)'}`,
            `author: ${metadata.author ?? '(none)'}`,
            `subject: ${metadata.subject ?? '(none)'}`,
            `keywords: ${metadata.keywords ?? '(none)'}`,
            `creator: ${metadata.creator ?? '(none)'}`,
            `producer: ${metadata.producer ?? '(none)'}`,
        ].join('\n');
        ctx.emitResult(report, human);
        return 0;
    });

cli
    .command('bookmarks', 'cli.cmd.bookmarks')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--bookmark <title:page>...', 'cli.opt.bookmark')
    .option('--depth <level>', 'cli.opt.depth')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        const entries = list(options, 'bookmark');
        if (entries.length === 0) {
            ctx.fail('open-pdf.cli.bookmark-args', {
                message: 'at least one `--bookmark "Title:Page"` is required',
            });
            return 2;
        }
        const depthText = text(options, 'depth');
        let depth = 0;
        if (depthText !== undefined) {
            depth = Number(depthText);
            if (!Number.isSafeInteger(depth) || depth < 0) {
                ctx.fail('open-pdf.cli.bookmark-args', { message: '`--depth <level>` must be a non-negative integer' });
                return 2;
            }
        }
        const bookmarks: Bookmark[] = [];
        for (const entry of entries) {
            const separator = entry.lastIndexOf(':');
            const title = separator < 0 ? '' : entry.slice(0, separator).trim();
            const pageText = separator < 0 ? '' : entry.slice(separator + 1).trim();
            const page = Number(pageText);
            if (title.length === 0 || !/^\d+$/.test(pageText) || !Number.isSafeInteger(page) || page < 1) {
                ctx.fail('open-pdf.cli.bookmark-args', {
                    message: `\`--bookmark "${entry}"\` must be "Title:Page" with a positive page number`,
                });
                return 2;
            }
            bookmarks.push({ title, page, depth });
        }
        const report = await pdf.setBookmarks(file, out, bookmarks);
        const human = [
            `bookmarks ${report.path} pages=${report.pages} entries=${report.bookmarks}`,
            ...bookmarks.map((bookmark) => `${'  '.repeat(bookmark.depth ?? 0)}${bookmark.title} -> page ${bookmark.page}`),
        ].join('\n');
        ctx.emitResult(report, human);
        return 0;
    });

cli
    .command('fill-form', 'cli.cmd.fill-form')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--set <Name=Value>...', 'cli.opt.set')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        const entries = list(options, 'set');
        if (entries.length === 0) {
            ctx.fail('open-pdf.cli.set-args', { message: 'at least one `--set Name=Value` is required' });
            return 2;
        }
        const values: Record<string, FormValue> = {};
        for (const entry of entries) {
            const separator = entry.indexOf('=');
            const name = separator < 0 ? '' : entry.slice(0, separator).trim();
            if (name.length === 0) {
                ctx.fail('open-pdf.cli.set-args', {
                    message: `\`--set "${entry}"\` must be "Name=Value" with a non-empty field name`,
                });
                return 2;
            }
            const raw = entry.slice(separator + 1);
            values[name] = raw === 'true' ? true : raw === 'false' ? false : raw;
        }
        const report = await pdf.fillForm(file, out, values);
        const human = [
            `filled ${report.path} pages=${report.pages} filled=${report.filled.length} missing=${report.missing.length}`,
            ...report.filled.map((name) => `  ${name}`),
            ...report.missing.map((name) => `  missing ${name}`),
        ].join('\n');
        ctx.emitResult(report, human);
        return 0;
    });

cli
    .command('create', 'cli.cmd.create')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--title <title>', 'cli.opt.title')
    .option('--page <line>...', 'cli.opt.page')
    .option('--page-size <size>', 'cli.opt.page-size')
    .option('--font <name>', 'cli.opt.font')
    .option('--font-size <pt>', 'cli.opt.font-size')
    .option('--margin <pt>', 'cli.opt.margin')
    .option('--align <align>', 'cli.opt.align')
    .option('--author <text>', 'cli.opt.author')
    .option('--subject <text>', 'cli.opt.subject')
    .option('--keywords <text>', 'cli.opt.keywords')
    .option('--creator <text>', 'cli.opt.creator')
    .option('--page-numbers', 'cli.opt.page-numbers')
    .option('--overwrite', 'cli.opt.overwrite')
    .actionCtx(async (options, _args, ctx) => {
        const out = text(options, 'out');
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        const pages = list(options, 'page').map((page) => page.replaceAll('|', '\n'));
        if (pages.length === 0) {
            ctx.fail('open-pdf.cli.page-required', { message: 'at least one `--page <line>` is required' });
            return 2;
        }
        const align = text(options, 'align');
        if (align !== undefined && align !== 'left' && align !== 'center' && align !== 'right') {
            ctx.fail('open-pdf.cli.create-style', { message: '`--align` must be `left`, `center` or `right`' });
            return 2;
        }
        const fontSize = decimal(options, 'font-size');
        if (fontSize === null) {
            ctx.fail('open-pdf.cli.create-style', { message: '`--font-size <pt>` must be a number' });
            return 2;
        }
        const margin = decimal(options, 'margin');
        if (margin === null) {
            ctx.fail('open-pdf.cli.create-style', { message: '`--margin <pt>` must be a number' });
            return 2;
        }
        const report = await pdf.create({
            out,
            title: text(options, 'title'),
            pages,
            overwrite: flag(options, 'overwrite'),
            pageSize: text(options, 'page-size'),
            font: text(options, 'font'),
            fontSize: fontSize ?? undefined,
            marginPt: margin ?? undefined,
            align: align as 'left' | 'center' | 'right' | undefined,
            author: text(options, 'author'),
            subject: text(options, 'subject'),
            keywords: text(options, 'keywords'),
            creator: text(options, 'creator'),
            pageNumbers: flag(options, 'page-numbers'),
        });
        ctx.emitResult(report, `created ${report.path} pages=${report.pages} bytes=${report.bytes}`);
        return 0;
    });

cli
    .command('stamp', 'cli.cmd.stamp')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--text <text>', 'cli.opt.stamp-text')
    .option('--position <position>', 'cli.opt.position')
    .option('--font <name>', 'cli.opt.font')
    .option('--font-size <pt>', 'cli.opt.font-size')
    .option('--rotation <degrees>', 'cli.opt.stamp-rotation')
    .option('--opacity <value>', 'cli.opt.opacity')
    .option('--color <hex>', 'cli.opt.color')
    .option('--pages <list>', 'cli.opt.stamp-pages')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const out = text(options, 'out');
        const stampText = text(options, 'text');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (out === undefined) {
            ctx.fail('open-pdf.cli.out-required', { message: '`--out <file>` is required' });
            return 2;
        }
        if (stampText === undefined || stampText.length === 0) {
            ctx.fail('open-pdf.cli.stamp-args', { message: '`--text <text>` is required' });
            return 2;
        }
        const rotation = decimal(options, 'rotation');
        const opacity = decimal(options, 'opacity');
        const fontSize = decimal(options, 'font-size');
        if (rotation === null || opacity === null || fontSize === null) {
            ctx.fail('open-pdf.cli.stamp-args', {
                message: '`--rotation`, `--opacity` and `--font-size` must be numbers',
            });
            return 2;
        }
        const position = text(options, 'position') as
            | 'center'
            | 'top'
            | 'bottom'
            | 'header'
            | 'footer'
            | undefined;
        const report = await pdf.stamp(file, out, {
            text: stampText,
            position,
            font: text(options, 'font'),
            fontSize: fontSize ?? undefined,
            rotation: rotation ?? undefined,
            opacity: opacity ?? undefined,
            color: text(options, 'color'),
            pages: text(options, 'pages'),
        });
        ctx.emitResult(
            report,
            `stamped ${report.path} pages=${report.pages} stamped=${report.pagesStamped} bytes=${report.bytes}`,
        );
        return 0;
    });

cli
    .command('search', 'cli.cmd.search')
    .option('--limit <n>', 'cli.opt.limit')
    .option('--pages <ranges>', 'cli.opt.pages')
    .option('--ignore-case', 'cli.opt.ignore-case')
    .option('--whole-word', 'cli.opt.whole-word')
    .option('--skip <n>', 'cli.opt.skip')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const query = options._?.[1];
        if (file === undefined || query === undefined || query.trim().length === 0) {
            ctx.fail('open-pdf.cli.search-args', { message: 'a PDF file and a non-empty query are required' });
            return 2;
        }
        let limit: number | undefined;
        const limitText = text(options, 'limit');
        if (limitText !== undefined) {
            const parsed = Number(limitText);
            if (!Number.isSafeInteger(parsed) || parsed < 1) {
                ctx.fail('open-pdf.cli.search-limit', { message: '`--limit <n>` must be a positive integer' });
                return 2;
            }
            limit = parsed;
        }
        let skip = 0;
        const skipText = text(options, 'skip');
        if (skipText !== undefined) {
            const parsed = Number(skipText);
            if (!Number.isSafeInteger(parsed) || parsed < 0) {
                ctx.fail('open-pdf.cli.search-limit', { message: '`--skip <n>` must be a non-negative integer' });
                return 2;
            }
            skip = parsed;
        }
        const report = await pdf.search(file, query, {
            limit,
            pages: text(options, 'pages'),
            matchCase: !flag(options, 'ignore-case'),
            wholeWord: flag(options, 'whole-word'),
            skip,
        });
        const scope = report.selected === undefined ? '' : ` pages=${report.selected.join(',')}`;
        const lines = report.hits.map((hit) => `${hit.line}:${hit.column} ${hit.match}`);
        ctx.emitResult(
            report,
            [`${report.hits.length} hit(s) for ${JSON.stringify(report.query)}${scope}`, ...lines].join('\n'),
        );
        return 0;
    });

cli
    .command('extract-text', 'cli.cmd.extract-text')
    .option('--stdin', 'cli.opt.stdin')
    .option('--pages <ranges>', 'cli.opt.pages')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const useStdin = flag(options, 'stdin');
        const pages = text(options, 'pages');
        if ((file === undefined) === !useStdin) {
            ctx.fail('open-pdf.cli.input-required', { message: 'pass one PDF path or `--stdin`, not both' });
            return 2;
        }
        if (useStdin && pages !== undefined) {
            ctx.fail('open-pdf.cli.pages-need-file', { message: '`--pages` needs a PDF path; standard input cannot be paginated' });
            return 2;
        }
        let report: ExtractReport;
        if (useStdin) {
            const bytes = await stdinBytes();
            if (bytes === undefined || bytes.length === 0) {
                ctx.fail('open-pdf.cli.stdin-empty', { message: 'no PDF bytes arrived on standard input' });
                return 2;
            }
            report = await pdf.extractTextBytes(bytes);
        }
        else {
            report = await pdf.extractText(file as string, pages === undefined ? {} : { pages });
        }
        const losses = report.losses.map((loss) => `loss ${loss.code} page=${loss.page} ${loss.message}`);
        const human = [report.markdown.length > 0 ? report.markdown : '(no text extracted)', ...losses].join('\n');
        ctx.emitResult(report, human);
        return 0;
    });

cli
    .command('extract-images', 'cli.cmd.extract-images')
    .option('-o, --out-dir <dir>', 'cli.opt.image-dir')
    .option('--stdin', 'cli.opt.stdin')
    .option('--name <stem>', 'cli.opt.image-stem')
    .option('--pages <ranges>', 'cli.opt.pages')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const outDir = text(options, 'out-dir');
        const useStdin = flag(options, 'stdin');
        const pages = text(options, 'pages');
        if ((file === undefined) === !useStdin) {
            ctx.fail('open-pdf.cli.input-required', { message: 'pass one PDF path or `--stdin`, not both' });
            return 2;
        }
        if (useStdin && pages !== undefined) {
            ctx.fail('open-pdf.cli.pages-need-file', { message: '`--pages` needs a PDF path; standard input cannot be paginated' });
            return 2;
        }
        if (outDir === undefined) {
            ctx.fail('open-pdf.cli.image-dir-required', { message: '`--out-dir <dir>` is required' });
            return 2;
        }
        let images: ExtractedImage[];
        if (useStdin) {
            const bytes = await stdinBytes();
            if (bytes === undefined || bytes.length === 0) {
                ctx.fail('open-pdf.cli.stdin-empty', { message: 'no PDF bytes arrived on standard input' });
                return 2;
            }
            images = await pdf.extractImagesBytes(bytes, outDir, text(options, 'name') ?? 'stdin');
        }
        else {
            images = await pdf.extractImages(file as string, outDir, pages === undefined ? {} : { pages });
        }
        const human = images.length === 0
            ? '(no images found)'
            : images
                  .map((image) =>
                      image.path === ''
                          ? `loss page=${image.page} name=${image.name} ${image.loss ?? ''}`
                          : `${image.path} ${image.width}x${image.height} ${image.filter}`,
                  )
                  .join('\n');
        ctx.emitResult(images, human);
        return 0;
    });

const entry = process.argv[1];
if (entry !== undefined && import.meta.url === pathToFileURL(entry).href) {
    const code = await cli.parse(process.argv);
    process.exitCode = code;
}
