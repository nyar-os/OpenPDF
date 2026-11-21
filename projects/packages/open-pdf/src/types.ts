/**
 * Result and capability types of `@wae/open-pdf`.
 *
 * These describe what the native engine returns. Nothing here implies support:
 * read {@link PdfCapabilities} from `capabilities()` before assuming an
 * operation works.
 */

/** Disclosure level. `partial` always ships a loss path. */
export type SupportLevel = 'supported' | 'partial' | 'unsupported';

/** Document information dictionary. Missing entries are `null`. */
export interface PdfMetadata {
    readonly title: string | null;
    readonly author: string | null;
    readonly subject: string | null;
    readonly keywords: string | null;
    readonly creator: string | null;
    readonly producer: string | null;
    readonly creationDate: string | null;
    readonly modDate: string | null;
}

/** One page's geometry. */
export interface PdfPageSize {
    readonly widthPt: number;
    readonly heightPt: number;
    readonly rotation: number;
}

/** One outline entry. */
export interface PdfBookmark {
    readonly title: string;
    readonly depth: number;
}

/** Result of `inspect()`. */
export interface PdfInspectReport {
    readonly label: string;
    readonly version: string;
    readonly pages: number;
    readonly encrypted: boolean;
    readonly metadata: PdfMetadata;
    readonly pageSizes: readonly PdfPageSize[];
    readonly bookmarks: readonly PdfBookmark[];
}

/** What the installed engine can do. */
export interface PdfCapabilities {
    readonly engine: { readonly package: string; readonly version: string };
    readonly formats: {
        readonly pdf: {
            readonly import: SupportLevel;
            readonly extract: SupportLevel;
            readonly export: SupportLevel;
            readonly structures: {
                readonly classicXref: SupportLevel;
                readonly xrefStream: SupportLevel;
                readonly objectStream: SupportLevel;
                readonly encrypted: SupportLevel;
            };
            readonly filters: {
                readonly flateDecode: SupportLevel;
                readonly pngPredictor: SupportLevel;
                readonly tiffPredictor: SupportLevel;
                readonly other: SupportLevel;
            };
        };
    };
    readonly operations: {
        readonly inspect: SupportLevel;
        readonly create: SupportLevel;
        readonly merge: SupportLevel;
        readonly split: SupportLevel;
        readonly extractText: SupportLevel;
    };
    readonly checks: {
        readonly metadata: SupportLevel;
        readonly outline: SupportLevel;
        readonly pageTree: SupportLevel;
    };
    readonly renderers: {
        readonly pdf: SupportLevel;
        readonly raster: SupportLevel;
        readonly svg: SupportLevel;
    };
}

/** Options accepted by `merge()`. */
export interface MergeOptions {
    /** Replace the output file when it already exists. Defaults to false. */
    readonly overwrite?: boolean;
    /** Replaces `/Title` in the output information dictionary. */
    readonly title?: string;
}

/** Result of `merge()`. */
export interface MergeReport {
    readonly path: string;
    readonly pages: number;
    readonly bytes: number;
}

/** One file produced by `split()`. */
export interface SplitPart {
    readonly path: string;
    readonly pages: number;
}

/** Result of `split()`. */
export interface SplitReport {
    readonly documents: readonly SplitPart[];
}

/** Request accepted by `create()`. */
export interface CreateRequest {
    readonly out: string;
    readonly title?: string;
    /** One entry per page. `\n` starts a new line inside the page. */
    readonly pages: readonly string[];
    readonly overwrite?: boolean;
}

/** Result of `create()`. */
export interface CreateReport {
    readonly path: string;
    readonly pages: number;
    readonly bytes: number;
}

/** One text fragment that could not be extracted. */
export interface ExtractLoss {
    readonly code: string;
    readonly page: number;
    readonly message: string;
}

/** Result of `extractText()`. */
export interface ExtractReport {
    readonly pages: number;
    readonly markdown: string;
    readonly losses: readonly ExtractLoss[];
}
