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
    ExtractedImage,
    ExtractReport,
    FormFillReport,
    FormValue,
    KeepPagesReport,
    MergeOptions,
    MergeReport,
    MetadataPatch,
    PdfCapabilities,
    PdfInspectReport,
    PdfOutlineReport,
    PdfSearchReport,
    RotatePagesReport,
    SetMetadataReport,
    SetPageBoxesReport,
    SplitReport,
    StampReport,
    StampRequest,
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

    /** Reads version, pages, metadata, page sizes and outlines from one PDF. */
    async inspect(path: string): Promise<PdfInspectReport> {
        return service.inspect(path);
    }

    /** Reads the outline tree of one PDF with entry counts and nesting depth. */
    async outline(path: string): Promise<PdfOutlineReport> {
        return service.outline(path);
    }

    /** Concatenates PDF files in the given order. */
    async merge(inputs: readonly string[], out: string, options: MergeOptions = {}): Promise<MergeReport> {
        return service.merge(inputs, out, { overwrite: options.overwrite ?? this.defaultOverwrite, title: options.title });
    }

    /** Splits one PDF into files selected by 1-based ranges. */
    async split(path: string, ranges: string, outDir: string): Promise<SplitReport> {
        return service.split(path, ranges, outDir);
    }

    /** Splits one PDF into consecutive files of at most `every` pages each. */
    async splitEvery(path: string, every: number, outDir: string): Promise<SplitReport> {
        return service.splitEvery(path, every, outDir);
    }

    /** Reorders, removes and rotates pages into a new PDF. */
    async arrange(path: string, out: string, ops: ArrangeRequest = {}): Promise<ArrangeReport> {
        return service.arrange(path, out, ops);
    }

    /** Removes the selected pages into a new PDF. */
    async deletePages(path: string, pages: string, out: string, overwrite = this.defaultOverwrite): Promise<DeletePagesReport> {
        return service.deletePages(path, pages, out, overwrite);
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

    /** Reads version, pages, metadata, page sizes and outlines from one PDF. */
    static inspect(path: string): PdfInspectReport {
        return service.inspect(path);
    }

    /** Reads the outline tree of one PDF with entry counts and nesting depth. */
    static outline(path: string): PdfOutlineReport {
        return service.outline(path);
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

    /** Reorders, removes and rotates pages into a new PDF. */
    static arrange(path: string, out: string, ops: ArrangeRequest = {}): ArrangeReport {
        return service.arrange(path, out, ops);
    }

    /** Removes the selected pages into a new PDF. */
    static deletePages(path: string, pages: string, out: string, overwrite = false): DeletePagesReport {
        return service.deletePages(path, pages, out, overwrite);
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
