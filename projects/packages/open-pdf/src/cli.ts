#!/usr/bin/env node
/**
 * Command-line assembly only.
 *
 * Every command maps options onto one operation of `service.ts`; PDF semantics
 * stay native. Help text and errors come from `@wae/commander`, so the CLI does
 * not grow its own parser.
 */

import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { createCli, type ParsedOptions } from '@wae/commander';

import * as service from './service.js';
import type { PdfCapabilities, PdfInspectReport, SplitReport } from './types.js';

const here = path.dirname(fileURLToPath(import.meta.url));

const cli = createCli('open-pdf').locales(path.join(here, '..', 'locales')).intro('cli.intro');

export { cli };

const text = (options: ParsedOptions, key: string): string | undefined =>
    typeof options[key] === 'string' ? (options[key] as string) : undefined;

const flag = (options: ParsedOptions, key: string): boolean => options[key] === true || options[key] === 'true';

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

const input = (options: ParsedOptions): string | undefined => options._?.[0];

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

cli.command('capabilities', 'cli.cmd.capabilities').actionCtx(async (_options, _args, ctx) => {
    const matrix = service.capabilities();
    ctx.emitResult(matrix, describeCapabilities(matrix));
    return 0;
});

cli.command('inspect', 'cli.cmd.inspect').actionCtx(async (options, _args, ctx) => {
    const file = input(options);
    if (file === undefined) {
        ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
        return 2;
    }
    const report = service.inspect(file);
    ctx.emitResult(report, describeInspect(report));
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
        const report = service.merge(inputs, out, {
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
            report = service.splitEvery(file, count, outDir);
        } else {
            report = service.split(file, ranges as string, outDir);
        }
        ctx.emitResult(
            report,
            report.documents.map((part) => `${part.path} pages=${part.pages}`).join('\n'),
        );
        return 0;
    });

cli
    .command('create', 'cli.cmd.create')
    .option('-o, --out <file>', 'cli.opt.out')
    .option('--title <title>', 'cli.opt.title')
    .option('--page <line>...', 'cli.opt.page')
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
        const report = service.create({
            out,
            title: text(options, 'title'),
            pages,
            overwrite: flag(options, 'overwrite'),
        });
        ctx.emitResult(report, `created ${report.path} pages=${report.pages} bytes=${report.bytes}`);
        return 0;
    });

cli.command('extract-text', 'cli.cmd.extract-text').actionCtx(async (options, _args, ctx) => {
    const file = input(options);
    if (file === undefined) {
        ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
        return 2;
    }
    const report = service.extractText(file);
    const losses = report.losses.map((loss) => `loss ${loss.code} page=${loss.page} ${loss.message}`);
    const human = [report.markdown.length > 0 ? report.markdown : '(no text extracted)', ...losses].join('\n');
    ctx.emitResult(report, human);
    return 0;
});

cli
    .command('extract-images', 'cli.cmd.extract-images')
    .option('-o, --out-dir <dir>', 'cli.opt.image-dir')
    .actionCtx(async (options, _args, ctx) => {
        const file = input(options);
        const outDir = text(options, 'out-dir');
        if (file === undefined) {
            ctx.fail('open-pdf.cli.file-required', { message: 'a PDF file is required' });
            return 2;
        }
        if (outDir === undefined) {
            ctx.fail('open-pdf.cli.image-dir-required', { message: '`--out-dir <dir>` is required' });
            return 2;
        }
        const images = service.extractImages(file, outDir);
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
