/**
 * Orthogonal native operations.
 *
 * One function performs one operation and returns data; argument parsing, help
 * text and exit codes belong to `cli.ts`. Nothing here knows about a command
 * line.
 */

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
    ExtractedImage,
    ExtractReport,
    FormFillReport,
    FormValue,
    MergeOptions,
    MergeReport,
    MetadataPatch,
    PdfCapabilities,
    PdfInspectReport,
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

/** Concatenates PDF files in the given order. */
export function merge(inputs: readonly string[], out: string, options: MergeOptions = {}): MergeReport {
    return decode<MergeReport>(() =>
        binding('merge').pdf_merge(JSON.stringify(inputs), out, options.overwrite ?? false, options.title ?? ''),
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
export function extractText(path: string): ExtractReport {
    return decode<ExtractReport>(() => binding('extractText').pdf_extract_text(path));
}

/** Extracts images from every page into `outDir`. */
export function extractImages(path: string, outDir: string): ExtractedImage[] {
    return decode<ExtractedImage[]>(() => binding('extractImages').pdf_extract_images(path, outDir));
}
