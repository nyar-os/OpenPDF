/**
 * Orthogonal native operations.
 *
 * One function performs one operation and returns data; argument parsing, help
 * text and exit codes belong to `cli.ts`. Nothing here knows about a command
 * line.
 */

import { copyFileSync, existsSync, statSync } from 'node:fs';

import { NotImplementedError, OpenPdfError, errorCodes, type OpenPdfErrorCode } from './errors.js';
import { nativeBinding } from './native.js';
import type {
    ArrangeReport,
    ArrangeRequest,
    Bookmark,
    BookmarkReport,
    CropBox,
    CreateReport,
    CreateRequest,
    DeletePagesReport,
    DuplicatePagesReport,
    ExtractedImage,
    ExtractReport,
    FormFillReport,
    FormValue,
    InsertBlankPagesReport,
    InsertPagesReport,
    KeepPagesReport,
    MergeOptions,
    MergeReport,
    MetadataPatch,
    PdfCapabilities,
    PdfInspectReport,
    PdfMetadataReport,
    PdfOutlineReport,
    PdfSearchReport,
    ReplacePagesReport,
    ReversePagesReport,
    RotatePagesReport,
    SetMetadataReport,
    SetPageBoxesReport,
    SplitReport,
    StampReport,
    StampRequest,
} from './types.js';

const knownCodes = new Set<string>(errorCodes);

function binding(feature: string) {
    const native = nativeBinding();
    if (!native) {
        throw new NotImplementedError(feature);
    }
    return native;
}

function decode<T>(work: () => string): T {
    try {
        return JSON.parse(work()) as T;
    } catch (error) {
        throw translate(error);
    }
}

function translate(error: unknown): OpenPdfError {
    if (error instanceof OpenPdfError) {
        return error;
    }
    const message = error instanceof Error ? error.message : String(error);
    const separator = message.indexOf(': ');
    const code = separator > 0 ? message.slice(0, separator) : '';
    if (knownCodes.has(code)) {
        return new OpenPdfError(code as OpenPdfErrorCode, message.slice(separator + 2), {
            details: { feature: code },
        });
    }
    return new OpenPdfError('format/invalid', message, { cause: error });
}

/** Reports the installed addon version. */
export function version(): string {
    return binding('version').version();
}

/** Reads the capability matrix of the installed addon. */
export function capabilities(): PdfCapabilities {
    return decode<PdfCapabilities>(() => binding('capabilities').capabilities());
}

/** Reads version, pages, metadata, page sizes and outlines from one PDF. */
export function inspect(path: string): PdfInspectReport {
    return decode<PdfInspectReport>(() => binding('inspect').pdf_inspect(path));
}

/** Reads the outline tree of one PDF without rendering or rewriting it. */
export function outline(path: string): PdfOutlineReport {
    return decode<PdfOutlineReport>(() => binding('outline').pdf_outline(path));
}

/** Reads the document information dictionary of one PDF. */
export function metadata(path: string): PdfMetadataReport {
    return decode<PdfMetadataReport>(() => binding('metadata').pdf_metadata(path));
}

/** Concatenates PDF files in the given order. */
export function merge(inputs: readonly string[], out: string, options: MergeOptions = {}): MergeReport {
    return decode<MergeReport>(() =>
        binding('merge').pdf_merge(
            JSON.stringify(inputs),
            out,
            options.overwrite ?? false,
            options.title ?? '',
            options.pages === undefined ? '' : JSON.stringify(options.pages),
        ),
    );
}

/** Splits one PDF into files selected by 1-based ranges. */
export function split(path: string, ranges: string, outDir: string): SplitReport {
    return decode<SplitReport>(() => binding('split').pdf_split(path, ranges, outDir));
}

/** Splits one PDF into consecutive files of at most `every` pages each. */
export function splitEvery(path: string, every: number, outDir: string): SplitReport {
    return decode<SplitReport>(() => binding('splitEvery').pdf_split_every(path, every, outDir));
}

/**
 * Splits one PDF into `parts` consecutive files, balancing page counts.
 *
 * Earlier files take the remainder: seven pages in three parts become
 * `3, 2, 2`. `parts` must be between `1` and the page count.
 */
export function splitParts(path: string, parts: number, outDir: string): SplitReport {
    return decode<SplitReport>(() => binding('splitParts').pdf_split_parts(path, parts, outDir));
}

/** Reorders, removes and rotates pages into a new PDF. */
export function arrange(path: string, out: string, ops: ArrangeRequest = {}): ArrangeReport {
    return decode<ArrangeReport>(() =>
        binding('arrange').pdf_arrange(
            path,
            out,
            JSON.stringify({
                order: ops.order === undefined ? undefined : [...ops.order],
                rotate: ops.rotate,
                remove: ops.remove === undefined ? undefined : [...ops.remove],
            }),
        ),
    );
}

/** Removes the selected pages into a new PDF. */
export function deletePages(path: string, pages: string, out: string, overwrite = false): DeletePagesReport {
    return decode<DeletePagesReport>(() =>
        binding('deletePages').pdf_delete_pages(path, pages, out, overwrite),
    );
}

/**
 * Repeats the selected pages directly after themselves.
 *
 * The document keeps its order; each selected page is written `times` times,
 * so `times: 2` doubles the selection in place.
 */
export function duplicatePages(
    path: string,
    pages: string,
    times: number,
    out: string,
    overwrite = false,
): DuplicatePagesReport {
    return decode<DuplicatePagesReport>(() =>
        binding('duplicatePages').pdf_duplicate_pages(path, pages, times, out, overwrite),
    );
}

/**
 * Inserts every page of `insert` after the selected page of `path`.
 *
 * `after` is the one-based target page the inserted pages follow; `0` puts
 * them before the first page and `pages` appends them at the end.
 */
export function insertPages(
    path: string,
    insert: string,
    after: number,
    out: string,
    overwrite = false,
    selected?: string,
): InsertPagesReport {
    return decode<InsertPagesReport>(() =>
        binding('insertPages').pdf_insert_pages(path, insert, after, out, overwrite, selected ?? ''),
    );
}

/**
 * Inserts `count` blank pages after the one-based target page `after`.
 *
 * `after: 0` puts the blanks before the first page and `after: pages` appends
 * them. `size` is a named paper size or `WIDTHxHEIGHT` in points; when omitted
 * the blanks take the geometry of the source's first page.
 */
export function insertBlankPages(
    path: string,
    after: number,
    count: number,
    size: string | undefined,
    out: string,
    overwrite = false,
): InsertBlankPagesReport {
    return decode<InsertBlankPagesReport>(() =>
        binding('insertBlankPages').pdf_insert_blank_pages(path, after, count, size, out, overwrite),
    );
}

/**
 * Replaces the selected pages of `path` with pages of `insert`.
 *
 * The replacement pages take the position of the first removed page and the
 * remaining pages keep their order. `selected` is an optional one-based source
 * range list such as `3,1`; every page of `insert` is used when it is omitted.
 */
export function replacePages(
    path: string,
    pages: string,
    insert: string,
    out: string,
    overwrite = false,
    selected: string | null = null,
): ReplacePagesReport {
    return decode<ReplacePagesReport>(() =>
        binding('replacePages').pdf_replace_pages(path, pages, insert, out, overwrite, selected),
    );
}

/** Reverses the page order of `path` into a new PDF. */
export function reversePages(path: string, out: string, overwrite = false): ReversePagesReport {
    return decode<ReversePagesReport>(() => binding('reversePages').pdf_reverse_pages(path, out, overwrite));
}

/**
 * Expands a 1-based page list such as `1-3,5` into ascending page numbers.
 *
 * Duplicates collapse and the result always follows document order, so a list
 * `5,1` keeps page 1 before page 5. Out-of-range bounds answer with
 * `session/argument-invalid` instead of producing a truncated document.
 */
export function expandPageList(spec: string, total: number): number[] {
    const parts = spec.split(',').map((part) => part.trim()).filter((part) => part.length > 0);
    if (parts.length === 0) {
        throw new OpenPdfError('session/argument-invalid', '`pages` must list 1-based pages such as `1-3,5`', {
            details: { feature: 'keepPages' },
        });
    }
    const selected = new Set<number>();
    for (const part of parts) {
        const bounds = part.split('-');
        if (bounds.length > 2 || !bounds.every((bound) => /^\d+$/.test(bound))) {
            throw new OpenPdfError('session/argument-invalid', `\`${part}\` is not a page number or range`, {
                details: { feature: 'keepPages' },
            });
        }
        const start = Number(bounds[0]);
        const end = bounds.length === 1 ? start : Number(bounds[1]);
        if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 1 || end < start) {
            throw new OpenPdfError('session/argument-invalid', `\`${part}\` is not a 1-based page range`, {
                details: { feature: 'keepPages' },
            });
        }
        if (end > total) {
            throw new OpenPdfError(
                'session/argument-invalid',
                `\`${part}\` reaches past the document's ${total} pages`,
                { details: { feature: 'keepPages' } },
            );
        }
        for (let page = start; page <= end; page += 1) {
            selected.add(page);
        }
    }
    return [...selected].sort((left, right) => left - right);
}

/** Writes ascending page numbers as compact 1-based ranges such as `1-3,5`. */
export function formatPageList(pages: readonly number[]): string {
    const parts: string[] = [];
    let index = 0;
    while (index < pages.length) {
        let end = index;
        while (end + 1 < pages.length && pages[end + 1] === pages[end] + 1) {
            end += 1;
        }
        parts.push(index === end ? `${pages[index]}` : `${pages[index]}-${pages[end]}`);
        index = end + 1;
    }
    return parts.join(',');
}

/** Keeps only the selected pages, in source order, into a new PDF. */
export function keepPages(path: string, pages: string, out: string, overwrite = false): KeepPagesReport {
    const total = inspect(path).pages;
    const kept = expandPageList(pages, total);
    const selected = new Set(kept);
    const removed = Array.from({ length: total }, (_, index) => index + 1).filter((page) => !selected.has(page));
    if (removed.length === 0) {
        if (!overwrite && existsSync(out)) {
            throw new OpenPdfError('io/exists', `${out} already exists; pass overwrite to replace it`, {
                details: { feature: 'keepPages', path: out },
            });
        }
        copyFileSync(path, out);
        return { path: out, pages: total, kept: total, removed: 0, bytes: statSync(out).size };
    }
    const report = deletePages(path, formatPageList(removed), out, overwrite);
    return { path: report.path, pages: report.pages, kept: report.pages, removed: report.removed, bytes: report.bytes };
}

/** Adds a clockwise rotation to the selected pages into a new PDF. */
export function rotatePages(path: string, pages: string, degrees: number, out: string, overwrite = false): RotatePagesReport {
    return decode<RotatePagesReport>(() =>
        binding('rotatePages').pdf_rotate_pages(path, pages, degrees, out, overwrite),
    );
}

/** Writes a `/CropBox` on the selected pages into a new PDF. */
export function setPageBoxes(path: string, pages: string, crop: CropBox, out: string, overwrite = false): SetPageBoxesReport {
    return decode<SetPageBoxesReport>(() =>
        binding('setPageBoxes').pdf_set_page_boxes(path, pages, JSON.stringify([...crop]), out, overwrite),
    );
}

/** Copies every page while rewriting the information dictionary. */
export function setMetadata(path: string, out: string, patch: MetadataPatch): SetMetadataReport {
    return decode<SetMetadataReport>(() => binding('setMetadata').pdf_set_metadata(path, out, JSON.stringify(patch)));
}

/** Copies every page while clearing every standard `/Info` field. */
export function clearMetadata(path: string, out: string): SetMetadataReport {
    return decode<SetMetadataReport>(() => binding('clearMetadata').pdf_clear_metadata(path, out));
}

/** Copies every page while replacing the outline tree. */
export function setBookmarks(path: string, out: string, bookmarks: readonly Bookmark[]): BookmarkReport {
    return decode<BookmarkReport>(() =>
        binding('setBookmarks').pdf_set_bookmarks(
            path,
            out,
            JSON.stringify({
                bookmarks: bookmarks.map((bookmark) => ({
                    title: bookmark.title,
                    page: bookmark.page,
                    depth: bookmark.depth,
                })),
            }),
        ),
    );
}

/** Copies every page while filling AcroForm field values. */
export function fillForm(path: string, out: string, values: Readonly<Record<string, FormValue>>): FormFillReport {
    return decode<FormFillReport>(() =>
        binding('fillForm').pdf_fill_form(path, out, JSON.stringify({ values })),
    );
}

/** Creates a PDF with one text page per request entry. */
export function create(request: CreateRequest): CreateReport {
    return decode<CreateReport>(() =>
        binding('create').pdf_create(
            JSON.stringify({
                out: request.out,
                title: request.title,
                pages: [...request.pages],
                overwrite: request.overwrite ?? false,
                pageSize: request.pageSize,
                font: request.font,
                fontSize: request.fontSize,
                marginPt: request.marginPt,
                align: request.align,
                author: request.author,
                subject: request.subject,
                keywords: request.keywords,
                creator: request.creator,
                pageNumbers: request.pageNumbers,
            }),
        ),
    );
}

/** Paints a text stamp over the selected pages and writes a new PDF. */
export function stamp(path: string, out: string, request: StampRequest): StampReport {
    return decode<StampReport>(() =>
        binding('stamp').pdf_stamp(
            path,
            out,
            JSON.stringify({
                text: request.text,
                position: request.position,
                font: request.font,
                fontSize: request.fontSize,
                rotation: request.rotation,
                opacity: request.opacity,
                color: request.color,
                pages: request.pages,
            }),
        ),
    );
}

/** Extracts text from content streams decoded through their filters. */
export function extractText(path: string, pages?: string): ExtractReport {
    if (pages === undefined) {
        return decode<ExtractReport>(() => binding('extractText').pdf_extract_text(path));
    }
    return decode<ExtractReport>(() => binding('extractTextPages').pdf_extract_text_pages(path, pages));
}

/** Searches extracted text and reports line-scoped matches. */
export function search(
    path: string,
    query: string,
    limit: number | undefined,
    pages: string | undefined,
    matchCase: boolean,
    wholeWord: boolean,
    skip: number,
): PdfSearchReport {
    if (pages === undefined) {
        return decode<PdfSearchReport>(() => binding('search').pdf_search(path, query, limit, matchCase, wholeWord, skip));
    }
    return decode<PdfSearchReport>(() =>
        binding('searchPages').pdf_search_pages(path, query, pages, limit, matchCase, wholeWord, skip),
    );
}

/** Extracts images from every page into `outDir`, or only from `pages`. */
export function extractImages(path: string, outDir: string, pages?: string): ExtractedImage[] {
    if (pages === undefined) {
        return decode<ExtractedImage[]>(() => binding('extractImages').pdf_extract_images(path, outDir));
    }
    return decode<ExtractedImage[]>(() => binding('extractImagesPages').pdf_extract_images_pages(path, outDir, pages));
}

/** Reads version, pages, metadata, page sizes and outlines from PDF bytes. */
export function inspectBytes(bytes: Uint8Array): PdfInspectReport {
    return decode<PdfInspectReport>(() => binding('inspectBytes').pdf_inspect_bytes(bytes));
}

/** Extracts text from PDF bytes. */
export function extractTextBytes(bytes: Uint8Array): ExtractReport {
    return decode<ExtractReport>(() => binding('extractTextBytes').pdf_extract_text_bytes(bytes));
}

/** Extracts images from PDF bytes into `outDir`, naming files after `stem`. */
export function extractImagesBytes(bytes: Uint8Array, outDir: string, stem: string): ExtractedImage[] {
    return decode<ExtractedImage[]>(() => binding('extractImagesBytes').pdf_extract_images_bytes(bytes, outDir, stem));
}
