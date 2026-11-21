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

/** One terminal AcroForm field. */
export interface FormField {
    /** Fully qualified name built from the `/T` chain, such as `person.name`. */
    readonly name: string;
    /** Inherited `/FT` value (`Tx`, `Btn`, `Ch`, `Sig`), or `null`. */
    readonly type: string | null;
    /** `/V` as a text or name value, or `null`. */
    readonly value: string | null;
    /** Bit 1 of the inherited `/Ff` flags. */
    readonly readOnly: boolean;
    /** Bit 2 of the inherited `/Ff` flags. */
    readonly required: boolean;
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
    readonly formFields: readonly FormField[];
    readonly warnings: readonly string[];
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
                readonly acroForm: SupportLevel;
                readonly encrypted: SupportLevel;
            };
            readonly filters: {
                readonly flateDecode: SupportLevel;
                readonly asciiHexDecode: SupportLevel;
                readonly ascii85Decode: SupportLevel;
                readonly runLengthDecode: SupportLevel;
                readonly lzwDecode: SupportLevel;
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
        readonly arrange: SupportLevel;
        readonly extractText: SupportLevel;
        readonly extractImages: SupportLevel;
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

/** Page reordering, removal and rotation accepted by `arrange()`. */
export interface ArrangeRequest {
    /**
     * One input page number per output page. When `remove` is also set,
     * removal runs first and `order` must list every surviving page once.
     */
    readonly order?: readonly number[];
    /** Clockwise degrees added to each page's `/Rotate`: 90, 180 or 270. */
    readonly rotate?: number;
    /** Input pages to drop before applying `order`. */
    readonly remove?: readonly number[];
}

/** Result of `arrange()`. */
export interface ArrangeReport {
    readonly path: string;
    readonly pages: number;
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

/** One image reached through a page's `/Resources /XObject` graph. */
export interface ExtractedImage {
    /** One-based page number. */
    readonly page: number;
    /** One-based image index within the page. */
    readonly index: number;
    /** Resource name used by the page or Form XObject. */
    readonly name: string;
    /** Pixel width, `0` when the dictionary is unreadable. */
    readonly width: number;
    /** Pixel height, `0` when the dictionary is unreadable. */
    readonly height: number;
    /** Filter chain such as `DCTDecode` or `FlateDecode`, or `none`. */
    readonly filter: string;
    /** Written file path; empty when nothing was written. */
    readonly path: string;
    /** Why the image was skipped or only partially decoded. */
    readonly loss?: string;
}
