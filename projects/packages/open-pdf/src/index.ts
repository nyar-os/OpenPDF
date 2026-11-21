/**
 * Public SDK of `@wae/open-pdf`.
 *
 * The API is imperative: there is no open document handle and no read/write
 * lifecycle to manage. Call an operation, get a report back.
 */

import * as service from './service.js';
import type {
    ArrangeReport,
    ArrangeRequest,
    CreateReport,
    CreateRequest,
    ExtractedImage,
    ExtractReport,
    MergeOptions,
    MergeReport,
    MetadataPatch,
    PdfCapabilities,
    PdfInspectReport,
    SetMetadataReport,
    SplitReport,
} from './types.js';

export { OpenPdfError, NotImplementedError, errorCodes, isOpenPdfError } from './errors.js';
export type { OpenPdfErrorCode, OpenPdfErrorDetails, OpenPdfErrorOptions } from './errors.js';
export { documentFormats, formatFromPath, formatInfo, isPdfPath } from './formats.js';
export type { PdfFormat, PdfFormatInfo } from './formats.js';
export type * from './types.js';

/** Function collection mirroring the native surface with a class-shaped facade. */
export class OpenPDF {
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

    /** Copies every page while rewriting the information dictionary. */
    static setMetadata(path: string, out: string, patch: MetadataPatch): SetMetadataReport {
        return service.setMetadata(path, out, patch);
    }

    /** Creates a PDF with one Helvetica text page per request entry. */
    static create(request: CreateRequest): CreateReport {
        return service.create(request);
    }

    /** Extracts text from content streams decoded through their filters. */
    static extractText(path: string): ExtractReport {
        return service.extractText(path);
    }

    /** Extracts images from every page into `outDir`. */
    static extractImages(path: string, outDir: string): ExtractedImage[] {
        return service.extractImages(path, outDir);
    }
}
