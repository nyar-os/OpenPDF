/**
 * Public SDK of `@wae/open-pdf`.
 *
 * The session API is the product: `new OpenPDF(options?)` keeps the caller's
 * defaults, and each method performs one operation and returns a report. There
 * is no document handle and no read/write lifecycle to manage.
 *
 * The static methods are thin compatibility wrappers over the same operations
 * for callers that do not want a session.
 */

import * as service from './service.js';
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
    ExtractLinksReport,
    ExtractReport,
    FormFillReport,
    FormValue,
    InsertBlankPagesReport,
    InsertPagesReport,
    KeepPagesReport,
    MergeOptions,
    MergeReport,
    MetadataPatch,
    MovePagesReport,
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
    SplitBookmarksReport,
    SplitReport,
    StampPageNumbersReport,
    StampReport,
    StampRequest,
    SwapPagesReport,
} from './types.js';

export { OpenPdfError, NotImplementedError, errorCodes, isOpenPdfError } from './errors.js';
export type { OpenPdfErrorCode, OpenPdfErrorDetails, OpenPdfErrorOptions } from './errors.js';
export { documentFormats, formatFromPath, formatInfo, isPdfPath } from './formats.js';
export type { PdfFormat, PdfFormatInfo } from './formats.js';
export type * from './types.js';

/** Construction options of an {@link OpenPDF} session. */
export interface OpenPDFOptions {
    /** BCP-47 tag used for diagnostics and default copy. */
    readonly locale?: string;
    /** Default for operations that may replace an existing output. */
    readonly overwrite?: boolean;
}

/** PDF session. One instance carries the caller's defaults; it owns no handle. */
export class OpenPDF {
    /** BCP-47 tag used for diagnostics and default copy. */
    readonly locale: string;
    private readonly defaultOverwrite: boolean;

    constructor(options: OpenPDFOptions = {}) {
        this.locale = options.locale ?? 'en';
        this.defaultOverwrite = options.overwrite ?? false;
    }

    /** Reads the capability matrix of the installed addon. */
    async capabilities(): Promise<PdfCapabilities> {
        return service.capabilities();
    }

    /** Lists every link annotation one PDF carries. */
    async extractLinks(path: string): Promise<ExtractLinksReport> {
        return service.extractLinks(path);
    }

    /** Reads version, pages, metadata, page sizes and outlines from one PDF. */
    async inspect(path: string): Promise<PdfInspectReport> {
        return service.inspect(path);
    }

    /** Reads the outline tree of one PDF, optionally capped at `options.depth`. */
    async outline(path: string, options: { readonly depth?: number } = {}): Promise<PdfOutlineReport> {
        return limitOutline(service.outline(path), options.depth);
    }

    /** Reads the document information dictionary of one PDF. */
    async metadata(path: string): Promise<PdfMetadataReport> {
        return service.metadata(path);
    }

    /** Concatenates PDF files in the given order. */
    async merge(inputs: readonly string[], out: string, options: MergeOptions = {}): Promise<MergeReport> {
        return service.merge(inputs, out, {
            overwrite: options.overwrite ?? this.defaultOverwrite,
            title: options.title,
            pages: options.pages,
        });
    }

    /** Splits one PDF into files selected by 1-based ranges. */
    async split(path: string, ranges: string, outDir: string): Promise<SplitReport> {
        return service.split(path, ranges, outDir);
    }

    /** Splits one PDF into consecutive files of at most `every` pages each. */
    async splitEvery(path: string, every: number, outDir: string): Promise<SplitReport> {
        return service.splitEvery(path, every, outDir);
    }

    /** Splits one PDF into `parts` consecutive files, balancing page counts. */
    async splitParts(path: string, parts: number, outDir: string): Promise<SplitReport> {
        return service.splitParts(path, parts, outDir);
    }

    /** Splits the document into files that start at each top-level bookmark. */
    async splitBookmarks(path: string, outDir: string): Promise<SplitBookmarksReport> {
        return service.splitBookmarks(path, outDir);
    }

    /** Reorders, removes and rotates pages into a new PDF. */
    async arrange(path: string, out: string, ops: ArrangeRequest = {}): Promise<ArrangeReport> {
        return service.arrange(path, out, ops);
    }

    /** Removes the selected pages into a new PDF. */
    async deletePages(path: string, pages: string, out: string, overwrite = this.defaultOverwrite): Promise<DeletePagesReport> {
        return service.deletePages(path, pages, out, overwrite);
    }

    /** Repeats the selected pages directly after themselves in a new PDF. */
    async duplicatePages(
        path: string,
        pages: string,
        times: number,
        out: string,
        overwrite = this.defaultOverwrite,
    ): Promise<DuplicatePagesReport> {
        return service.duplicatePages(path, pages, times, out, overwrite);
    }

    /** Inserts every page of `insert` after the one-based target page `after`. */
    async insertPages(
        path: string,
        insert: string,
        after: number,
        out: string,
        overwrite = this.defaultOverwrite,
        selected?: string,
    ): Promise<InsertPagesReport> {
        return service.insertPages(path, insert, after, out, overwrite, selected);
    }

    /**
     * Inserts `count` blank pages after the one-based target page `after`.
     *
     * `size` is a named paper size or `WIDTHxHEIGHT` in points; when omitted the
     * blanks take the geometry of the source's first page.
     */
    async insertBlankPages(
        path: string,
        after: number,
        count: number,
        size: string | undefined,
        out: string,
        overwrite = this.defaultOverwrite,
    ): Promise<InsertBlankPagesReport> {
        return service.insertBlankPages(path, after, count, size, out, overwrite);
    }

    /** Replaces the selected pages with pages of `insert`, optionally choosing source pages. */
    async replacePages(
        path: string,
        pages: string,
        insert: string,
        out: string,
        overwrite = this.defaultOverwrite,
        selected: string | null = null,
    ): Promise<ReplacePagesReport> {
        return service.replacePages(path, pages, insert, out, overwrite, selected);
    }

    /** Reverses the page order into a new PDF. */
    /** Moves the selected pages after the anchor page, or the front with `after = 0`. */
    async movePages(
        path: string,
        pages: string,
        after: number,
        out: string,
        overwrite = this.defaultOverwrite,
    ): Promise<MovePagesReport> {
        return service.movePages(path, pages, after, out, overwrite);
    }
    /** Reverses every page, or only the selected one-based pages. */
    async reversePages(
        path: string,
        out: string,
        overwrite = this.defaultOverwrite,
        selected: string | null = null,
    ): Promise<ReversePagesReport> {
        return service.reversePages(path, out, overwrite, selected);
    }

    /** Exchanges two equal-length page selections and keeps every other page in place. */
    async swapPages(
        path: string,
        pages: string,
        withPages: string,
        out: string,
        overwrite = this.defaultOverwrite,
    ): Promise<SwapPagesReport> {
        return service.swapPages(path, pages, withPages, out, overwrite);
    }

    /** Keeps only the selected pages, in source order, into a new PDF. */
    async keepPages(path: string, pages: string, out: string, overwrite = this.defaultOverwrite): Promise<KeepPagesReport> {
        return service.keepPages(path, pages, out, overwrite);
    }

    /** Adds a clockwise rotation to the selected pages into a new PDF. */
    async rotatePages(
        path: string,
        pages: string,
        degrees: number,
        out: string,
        overwrite = this.defaultOverwrite,
    ): Promise<RotatePagesReport> {
        return service.rotatePages(path, pages, degrees, out, overwrite);
    }

    /** Writes a `/CropBox` on the selected pages into a new PDF. */
    async setPageBoxes(
        path: string,
        pages: string,
        crop: CropBox,
        out: string,
        overwrite = this.defaultOverwrite,
    ): Promise<SetPageBoxesReport> {
        return service.setPageBoxes(path, pages, crop, out, overwrite);
    }

    /** Copies every page while rewriting the information dictionary. */
    async setMetadata(path: string, out: string, patch: MetadataPatch): Promise<SetMetadataReport> {
        return service.setMetadata(path, out, patch);
    }

    /** Copies every page while clearing every standard `/Info` field. */
    async clearMetadata(path: string, out: string): Promise<SetMetadataReport> {
        return service.clearMetadata(path, out);
    }

    /** Copies every page while replacing the outline tree. */
    async setBookmarks(path: string, out: string, bookmarks: readonly Bookmark[]): Promise<BookmarkReport> {
        return service.setBookmarks(path, out, bookmarks);
    }

    /** Copies every page while filling AcroForm field values. */
    async fillForm(path: string, out: string, values: Readonly<Record<string, FormValue>>): Promise<FormFillReport> {
        return service.fillForm(path, out, values);
    }

    /** Creates a PDF with one text page per request entry. */
    async create(request: CreateRequest): Promise<CreateReport> {
        return service.create({ ...request, overwrite: request.overwrite ?? this.defaultOverwrite });
    }

    /** Paints a text stamp over the selected pages and writes a new PDF. */
    async stamp(path: string, out: string, request: StampRequest): Promise<StampReport> {
        return service.stamp(path, out, request);
    }

    /**
     * Stamps sequential page numbers on the selected pages into a new PDF.
     *
     * The first selected page is numbered `start` and every following page
     * counts up. `format` accepts `{page}` and `{pages}` and defaults to
     * `{page} of {pages}`; `position` is `center`, `top` or `bottom` and
     * defaults to `bottom`.
     */
    async stampPageNumbers(
        path: string,
        pages: string,
        start: number,
        format: string | undefined,
        position: string | undefined,
        out: string,
        overwrite = this.defaultOverwrite,
    ): Promise<StampPageNumbersReport> {
        return service.stampPageNumbers(path, pages, start, format, position, out, overwrite);
    }

    /**
     * Extracts text from content streams decoded through their filters.
     *
     * `options.pages` is a one-based selection such as `1-3,5`; selected pages
     * are extracted in the requested order and `report.selected` names them.
     */
    async extractText(path: string, options: { readonly pages?: string } = {}): Promise<ExtractReport> {
        return service.extractText(path, options.pages);
    }

    /**
     * Searches the extracted text and reports line-scoped matches.
     *
     * Matching runs on the same Markdown as `extractText()`, so pages without
     * extractable text contribute no hits. `options.matchCase` defaults to
     * `true`; when `false`, ASCII letters compare case-insensitively.
     * `options.wholeWord` accepts only matches whose neighbouring characters
     * are not letters, digits or underscores. `options.pages` restricts the
     * search to a one-based selection such as `1-3,5`. `options.skip` drops
     * that many accepted matches before collecting, so callers can page
     * through hits. `limit` defaults to 100 and is clamped to 1-500.
     */
    async search(
        path: string,
        query: string,
        options: {
            readonly limit?: number;
            readonly pages?: string;
            readonly matchCase?: boolean;
            readonly wholeWord?: boolean;
            readonly skip?: number;
        } = {},
    ): Promise<PdfSearchReport> {
        return service.search(
            path,
            query,
            options.limit,
            options.pages,
            options.matchCase ?? true,
            options.wholeWord ?? false,
            options.skip ?? 0,
        );
    }

    /**
     * Extracts images into `outDir`.
     *
     * `options.pages` is a one-based selection such as `1-3,5`; selected pages
     * are extracted in the requested order and every image reports its source
     * page.
     */
    async extractImages(path: string, outDir: string, options: { readonly pages?: string } = {}): Promise<ExtractedImage[]> {
        return service.extractImages(path, outDir, options.pages);
    }

    /** Reads version, pages, metadata, page sizes and outlines from PDF bytes. */
    async inspectBytes(bytes: Uint8Array): Promise<PdfInspectReport> {
        return service.inspectBytes(bytes);
    }

    /** Extracts text from PDF bytes. */
    async extractTextBytes(bytes: Uint8Array): Promise<ExtractReport> {
        return service.extractTextBytes(bytes);
    }

    /** Extracts images from PDF bytes into `outDir`, naming files after `stem`. */
    async extractImagesBytes(bytes: Uint8Array, outDir: string, stem: string): Promise<ExtractedImage[]> {
        return service.extractImagesBytes(bytes, outDir, stem);
    }

    /** Reports the installed addon version. */
    static version(): string {
        return service.version();
    }

    /** Reads the capability matrix of the installed addon. */
    static capabilities(): PdfCapabilities {
        return service.capabilities();
    }

    /** Lists every link annotation one PDF carries. */
    static extractLinks(path: string): ExtractLinksReport {
        return service.extractLinks(path);
    }

    /** Reads version, pages, metadata, page sizes and outlines from one PDF. */
    static inspect(path: string): PdfInspectReport {
        return service.inspect(path);
    }

    /** Reads the outline tree of one PDF, optionally capped at `options.depth`. */
    static outline(path: string, options: { readonly depth?: number } = {}): PdfOutlineReport {
        return limitOutline(service.outline(path), options.depth);
    }

    /** Reads the document information dictionary of one PDF. */
    static metadata(path: string): PdfMetadataReport {
        return service.metadata(path);
    }

    /** Concatenates PDF files in the given order. */
    static merge(inputs: readonly string[], out: string, options: MergeOptions = {}): MergeReport {
        return service.merge(inputs, out, options);
    }

    /** Splits one PDF into files selected by 1-based ranges. */
    static split(path: string, ranges: string, outDir: string): SplitReport {
        return service.split(path, ranges, outDir);
    }

    /** Splits one PDF into consecutive files of at most `every` pages each. */
    static splitEvery(path: string, every: number, outDir: string): SplitReport {
        return service.splitEvery(path, every, outDir);
    }

    /** Splits one PDF into `parts` consecutive files, balancing page counts. */
    static splitParts(path: string, parts: number, outDir: string): SplitReport {
        return service.splitParts(path, parts, outDir);
    }

    /** Splits the document into files that start at each top-level bookmark. */
    static splitBookmarks(path: string, outDir: string): SplitBookmarksReport {
        return service.splitBookmarks(path, outDir);
    }

    /** Reorders, removes and rotates pages into a new PDF. */
    static arrange(path: string, out: string, ops: ArrangeRequest = {}): ArrangeReport {
        return service.arrange(path, out, ops);
    }

    /** Removes the selected pages into a new PDF. */
    static deletePages(path: string, pages: string, out: string, overwrite = false): DeletePagesReport {
        return service.deletePages(path, pages, out, overwrite);
    }

    /** Repeats the selected pages directly after themselves in a new PDF. */
    static duplicatePages(
        path: string,
        pages: string,
        times: number,
        out: string,
        overwrite = false,
    ): DuplicatePagesReport {
        return service.duplicatePages(path, pages, times, out, overwrite);
    }

    /** Inserts every page of `insert` after the one-based target page `after`. */
    static insertPages(
        path: string,
        insert: string,
        after: number,
        out: string,
        overwrite = false,
        selected?: string,
    ): InsertPagesReport {
        return service.insertPages(path, insert, after, out, overwrite, selected);
    }

    /** Inserts blank pages after the one-based target page `after`. */
    static insertBlankPages(
        path: string,
        after: number,
        count: number,
        size: string | undefined,
        out: string,
        overwrite = false,
    ): InsertBlankPagesReport {
        return service.insertBlankPages(path, after, count, size, out, overwrite);
    }

    /** Replaces the selected pages with pages of `insert`, optionally choosing source pages. */
    static replacePages(
        path: string,
        pages: string,
        insert: string,
        out: string,
        overwrite = false,
        selected: string | null = null,
    ): ReplacePagesReport {
        return service.replacePages(path, pages, insert, out, overwrite, selected);
    }

    /** Reverses the page order into a new PDF. */
    /** Moves the selected pages after the anchor page, or the front with `after = 0`. */
    static movePages(path: string, pages: string, after: number, out: string, overwrite = false): MovePagesReport {
        return service.movePages(path, pages, after, out, overwrite);
    }
    /** Reverses every page, or only the selected one-based pages. */
    static reversePages(
        path: string,
        out: string,
        overwrite = false,
        selected: string | null = null,
    ): ReversePagesReport {
        return service.reversePages(path, out, overwrite, selected);
    }

    /** Exchanges two equal-length page selections and keeps every other page in place. */
    static swapPages(
        path: string,
        pages: string,
        withPages: string,
        out: string,
        overwrite = false,
    ): SwapPagesReport {
        return service.swapPages(path, pages, withPages, out, overwrite);
    }

    /** Keeps only the selected pages, in source order, into a new PDF. */
    static keepPages(path: string, pages: string, out: string, overwrite = false): KeepPagesReport {
        return service.keepPages(path, pages, out, overwrite);
    }

    /** Adds a clockwise rotation to the selected pages into a new PDF. */
    static rotatePages(
        path: string,
        pages: string,
        degrees: number,
        out: string,
        overwrite = false,
    ): RotatePagesReport {
        return service.rotatePages(path, pages, degrees, out, overwrite);
    }

    /** Writes a `/CropBox` on the selected pages into a new PDF. */
    static setPageBoxes(
        path: string,
        pages: string,
        crop: CropBox,
        out: string,
        overwrite = false,
    ): SetPageBoxesReport {
        return service.setPageBoxes(path, pages, crop, out, overwrite);
    }

    /** Copies every page while rewriting the information dictionary. */
    static setMetadata(path: string, out: string, patch: MetadataPatch): SetMetadataReport {
        return service.setMetadata(path, out, patch);
    }

    /** Copies every page while clearing every standard `/Info` field. */
    static clearMetadata(path: string, out: string): SetMetadataReport {
        return service.clearMetadata(path, out);
    }

    /** Copies every page while replacing the outline tree. */
    static setBookmarks(path: string, out: string, bookmarks: readonly Bookmark[]): BookmarkReport {
        return service.setBookmarks(path, out, bookmarks);
    }

    /** Copies every page while filling AcroForm field values. */
    static fillForm(path: string, out: string, values: Readonly<Record<string, FormValue>>): FormFillReport {
        return service.fillForm(path, out, values);
    }

    /** Creates a PDF with one text page per request entry. */
    static create(request: CreateRequest): CreateReport {
        return service.create(request);
    }

    /** Paints a text stamp over the selected pages and writes a new PDF. */
    static stamp(path: string, out: string, request: StampRequest): StampReport {
        return service.stamp(path, out, request);
    }

    /** Stamps sequential page numbers on the selected pages into a new PDF. */
    static stampPageNumbers(
        path: string,
        pages: string,
        start: number,
        format: string | undefined,
        position: string | undefined,
        out: string,
        overwrite = false,
    ): StampPageNumbersReport {
        return service.stampPageNumbers(path, pages, start, format, position, out, overwrite);
    }

    /**
     * Extracts text from content streams decoded through their filters.
     *
     * `options.pages` is a one-based selection such as `1-3,5`; selected pages
     * are extracted in the requested order and `report.selected` names them.
     */
    static extractText(path: string, options: { readonly pages?: string } = {}): ExtractReport {
        return service.extractText(path, options.pages);
    }

    /** Searches the extracted text and reports line-scoped matches. */
    static search(
        path: string,
        query: string,
        options: {
            readonly limit?: number;
            readonly pages?: string;
            readonly matchCase?: boolean;
            readonly wholeWord?: boolean;
            readonly skip?: number;
        } = {},
    ): PdfSearchReport {
        return service.search(
            path,
            query,
            options.limit,
            options.pages,
            options.matchCase ?? true,
            options.wholeWord ?? false,
            options.skip ?? 0,
        );
    }

    /** Extracts images into `outDir`, optionally from the selected `pages`. */
    static extractImages(path: string, outDir: string, options: { readonly pages?: string } = {}): ExtractedImage[] {
        return service.extractImages(path, outDir, options.pages);
    }

    /** Reads version, pages, metadata, page sizes and outlines from PDF bytes. */
    static inspectBytes(bytes: Uint8Array): PdfInspectReport {
        return service.inspectBytes(bytes);
    }

    /** Extracts text from PDF bytes. */
    static extractTextBytes(bytes: Uint8Array): ExtractReport {
        return service.extractTextBytes(bytes);
    }

    /** Extracts images from PDF bytes into `outDir`, naming files after `stem`. */
    static extractImagesBytes(bytes: Uint8Array, outDir: string, stem: string): ExtractedImage[] {
        return service.extractImagesBytes(bytes, outDir, stem);
    }
}

/** Drops outline entries deeper than `depth` and recomputes the summary. */
function limitOutline(report: PdfOutlineReport, depth: number | undefined): PdfOutlineReport {
    if (depth === undefined) {
        return report;
    }
    if (!Number.isInteger(depth) || depth < 1) {
        throw new RangeError('`depth` must be a positive integer');
    }
    const bookmarks = report.bookmarks.filter((bookmark) => bookmark.depth <= depth);
    return {
        ...report,
        bookmarks,
        count: bookmarks.length,
        maxDepth: bookmarks.reduce((deepest, bookmark) => Math.max(deepest, bookmark.depth), 0),
    };
}
