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
    /** Nesting level, where top-level entries are `1`. */
    readonly depth: number;
}

/** Result of `outline()`. */
export interface PdfOutlineReport {
    readonly path: string;
    readonly pages: number;
    /** Number of outline entries. */
    readonly count: number;
    /** Deepest nesting level, `0` when the document has no outline. */
    readonly maxDepth: number;
    readonly bookmarks: readonly PdfBookmark[];
}

/** Result of `metadata()`. */
export interface PdfMetadataReport {
    readonly path: string;
    readonly pages: number;
    readonly metadata: PdfMetadata;
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
            readonly image: {
                readonly colorSpaces: {
                    readonly deviceGray: SupportLevel;
                    readonly deviceRgb: SupportLevel;
                    readonly deviceCmyk: SupportLevel;
                    readonly indexed: SupportLevel;
                    readonly other: SupportLevel;
                };
            };
        };
    };
    readonly operations: {
        readonly inspect: SupportLevel;
        readonly outline: SupportLevel;
        readonly create: SupportLevel;
        readonly merge: SupportLevel;
        readonly split: SupportLevel;
        readonly splitParts: SupportLevel;
        readonly arrange: SupportLevel;
        readonly movePages: SupportLevel;
        readonly deletePages: SupportLevel;
        readonly duplicatePages: SupportLevel;
        readonly insertPages: SupportLevel;
        readonly insertBlankPages: SupportLevel;
        readonly replacePages: SupportLevel;
        readonly reversePages: SupportLevel;
        readonly rotatePages: SupportLevel;
        readonly setPageBoxes: SupportLevel;
        readonly metadata: SupportLevel;
        readonly setMetadata: SupportLevel;
        readonly clearMetadata: SupportLevel;
        readonly setBookmarks: SupportLevel;
        readonly fillForm: SupportLevel;
        readonly stamp: SupportLevel;
        readonly extractText: SupportLevel;
        readonly extractTextPages: SupportLevel;
        readonly search: SupportLevel;
        readonly extractImages: SupportLevel;
        readonly extractImagesPages: SupportLevel;
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
    /**
     * One optional page selection per input, in the same order as `inputs`.
     * Each entry accepts one-based pages and inclusive ranges such as `1,3-4`;
     * inputs past the end of the list keep every page. A selection is rejected
     * when it is empty, malformed, reversed or outside its document.
     */
    readonly pages?: readonly string[];
}

/** One merged input and the pages it contributed. */
export interface MergeSource {
    readonly path: string;
    /** Page count of the input before selection. */
    readonly pages: number;
    /** Pages kept from the input. */
    readonly selected: number;
    /** The page ranges given for this input, or `null` when it kept every page. */
    readonly ranges: string | null;
}

/** Result of `merge()`. */
export interface MergeReport {
    readonly path: string;
    readonly pages: number;
    readonly bytes: number;
    readonly sources?: readonly MergeSource[];
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

/** Result of `duplicatePages()`. */
export interface DuplicatePagesReport {
    readonly path: string;
    /** Pages written to the output. */
    readonly pages: number;
    /** Pages in the source document. */
    readonly sourcePages: number;
    /** One-based pages that were repeated, in document order. */
    readonly duplicated: readonly number[];
    /** Copies written for each selected page. */
    readonly times: number;
}

/** Result of `insertPages()`. */
export interface InsertPagesReport {
    readonly path: string;
    /** Pages written to the output. */
    readonly pages: number;
    /** Pages in the target document. */
    readonly sourcePages: number;
    /** Pages taken from the inserted document. */
    readonly insertedPages: number;
    /** One-based pages available in the inserted document. */
    readonly sourceInsertPages?: number;
    /** One-based pages taken from the inserted document, in insertion order. */
    readonly selectedPages?: readonly number[];
    /** The inserted document. */
    readonly insert: string;
    /** One-based target page the inserted pages follow; `0` means first. */
    readonly after: number;
}

/** Result of `insertBlankPages()`. */
export interface InsertBlankPagesReport {
    readonly path: string;
    /** Pages written to the output. */
    readonly pages: number;
    /** Pages in the target document before the insertion. */
    readonly sourcePages: number;
    /** Blank pages that were inserted. */
    readonly insertedPages: number;
    /** One-based target page the blanks follow; `0` means first. */
    readonly after: number;
}

/** Result of `replacePages()`. */
export interface ReplacePagesReport {
    readonly path: string;
    /** Pages written to the output. */
    readonly pages: number;
    /** Pages in the target document. */
    readonly sourcePages: number;
    /** One-based pages that were removed, in document order. */
    readonly replaced: readonly number[];
    /** Pages taken from the replacement document, in the order used. */
    readonly replacementPages: number;
    /** Pages in the replacement document. */
    readonly sourceInsertPages: number;
    /** One-based source pages used as the replacement, in that order. */
    readonly selectedPages: readonly number[];
    /** The replacement document. */
    readonly insert: string;
    /** One-based pages reversed; every page when no selection was given. */
    readonly reversed: readonly number[];
}

/** Result of `movePages()`. */
export interface MovePagesReport {
    readonly path: string;
    /** Pages written to the output. */
    readonly pages: number;
    /** Pages in the source document. */
    readonly sourcePages: number;
    /** One-based pages moved, in the order they now appear. */
    readonly moved: readonly number[];
    /** Anchor page the selection follows; `0` means the front. */
    readonly after: number;
}

/** Result of `reversePages()`. */
export interface ReversePagesReport {
    readonly path: string;
    /** Pages written to the output. */
    readonly pages: number;
    /** Pages in the source document. */
    readonly sourcePages: number;
}

/** Result of `deletePages()`. */
export interface DeletePagesReport {
    readonly path: string;
    /** Pages written to the output. */
    readonly pages: number;
    /** Pages dropped from the source document. */
    readonly removed: number;
    readonly bytes: number;
}

/** Result of `keepPages()`. */
export interface KeepPagesReport {
    readonly path: string;
    /** Pages written to the output, in source order. */
    readonly pages: number;
    /** Pages kept from the source document. */
    readonly kept: number;
    /** Pages dropped from the source document. */
    readonly removed: number;
    readonly bytes: number;
}

/** Result of `rotatePages()`. */
export interface RotatePagesReport {
    readonly path: string;
    /** Pages written to the output. */
    readonly pages: number;
    /** Pages whose effective `/Rotate` changed. */
    readonly rotated: number;
    readonly bytes: number;
}

/** Result of `setPageBoxes()`. */
export interface SetPageBoxesReport {
    readonly path: string;
    /** Pages written to the output. */
    readonly pages: number;
    /** Pages that received the new `/CropBox`. */
    readonly cropped: number;
    readonly bytes: number;
}

/** `/CropBox` corners in points, `[left, bottom, right, top]`. */
export type CropBox = readonly [left: number, bottom: number, right: number, top: number];

/**
 * `/Info` fields accepted by `setMetadata()`.
 *
 * A missing field keeps its source value, an empty string removes it.
 */
export interface MetadataPatch {
    readonly title?: string;
    readonly author?: string;
    readonly subject?: string;
    readonly keywords?: string;
    readonly creator?: string;
    readonly producer?: string;
}

/** Result of `setMetadata()`. */
export interface SetMetadataReport {
    readonly path: string;
    readonly pages: number;
    readonly metadata: PdfMetadata;
}

/** One bookmark written by `setBookmarks()`. */
export interface Bookmark {
    /** Outline title; must not be blank. */
    readonly title: string;
    /** One-based destination page, inside `1..=pages`. */
    readonly page: number;
    /** Zero-based nesting level, where `0` is a top-level entry. Defaults to 0. */
    readonly depth?: number;
}

/** Outline tree accepted by `setBookmarks()`. */
export interface BookmarkRequest {
    readonly bookmarks: readonly Bookmark[];
}

/** Result of `setBookmarks()`. */
export interface BookmarkReport {
    readonly path: string;
    readonly pages: number;
    readonly bookmarks: number;
}

/** One value accepted by `fillForm()`. */
export type FormValue = string | boolean;

/** Field values accepted by `fillForm()`, keyed by fully qualified field name. */
export interface FormFillRequest {
    readonly values: Readonly<Record<string, FormValue>>;
}

/** Result of `fillForm()`. */
export interface FormFillReport {
    readonly path: string;
    /** Requested names that were written, in sorted request order. */
    readonly filled: readonly string[];
    /** Requested names that matched no terminal field, in sorted order. */
    readonly missing: readonly string[];
    /** Pages copied into the output. */
    readonly pages: number;
}

/** Request accepted by `create()`. */
export interface CreateRequest {
    readonly out: string;
    readonly title?: string;
    /** One entry per page. `\n` starts a new line inside the page. */
    readonly pages: readonly string[];
    readonly overwrite?: boolean;
    /** `a3`, `a4`, `a5`, `letter`, `legal`, `tabloid` or `WIDTHxHEIGHT`. Defaults to Letter. */
    readonly pageSize?: string;
    /** `helvetica`, `helvetica-bold`, `times`, `times-bold`, `courier` or `courier-bold`. */
    readonly font?: string;
    /** Body font size in points, `6..144`. */
    readonly fontSize?: number;
    /** Page margin in points. */
    readonly marginPt?: number;
    /** `left`, `center` or `right`. */
    readonly align?: 'left' | 'center' | 'right';
    readonly author?: string;
    readonly subject?: string;
    readonly keywords?: string;
    readonly creator?: string;
    /** Draw a `page / total` footer on every page. */
    readonly pageNumbers?: boolean;
}

/** Text stamp accepted by `stamp()`. */
export interface StampRequest {
    /** Text to paint; `{page}` and `{pages}` are substituted. */
    readonly text: string;
    /** `center` (default), `top`/`header` or `bottom`/`footer`. */
    readonly position?: 'center' | 'top' | 'bottom' | 'header' | 'footer';
    /** Base-14 font family, same names as `CreateRequest.font`. */
    readonly font?: string;
    /** Font size in points; defaults to 36 centered and 12 otherwise. */
    readonly fontSize?: number;
    /** Clockwise rotation for a centered stamp, `-180..180`. */
    readonly rotation?: number;
    /** Opacity `0..1`; defaults to 0.3 centered and 0.6 otherwise. */
    readonly opacity?: number;
    /** `#RGB` or `#RRGGBB` text colour. */
    readonly color?: string;
    /** 1-based page ranges such as `1-3,5`; every page when absent. */
    readonly pages?: string;
}

/** Result of `stamp()`. */
export interface StampReport {
    readonly path: string;
    readonly pages: number;
    readonly pagesStamped: number;
    readonly bytes: number;
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
    /** Pages in the extracted projection: the selected count when `pages` was given. */
    readonly pages: number;
    /** Pages in the source document; present when a page selection was used. */
    readonly sourcePages?: number;
    /** One-based source pages that were extracted, in extraction order. */
    readonly selected?: readonly number[];
    readonly markdown: string;
    readonly losses: readonly ExtractLoss[];
}

/** One line-scoped match reported by `search()`. */
export interface PdfSearchHit {
    /** One-based line in the extracted Markdown. */
    readonly line: number;
    /** One-based byte column within the line. */
    readonly column: number;
    /** Byte offset of the match within its line. */
    readonly start: number;
    /** Byte offset just past the match within its line. */
    readonly end: number;
    /** Matching fragment. */
    readonly match: string;
    /** Whole line that contains the match. */
    readonly text: string;
}

/** Result of `search()`. */
export interface PdfSearchReport {
    readonly path: string;
    readonly query: string;
    /** Whether ASCII letters were compared case-sensitively. */
    readonly matchCase: boolean;
    /** Whether only matches on word boundaries were accepted. */
    readonly wholeWord: boolean;
    /** Matches discarded before collecting `hits`. */
    readonly skip: number;
    /** Effective limit after clamping to 1-500. */
    readonly limit: number;
    readonly pages: number;
    /** Pages in the source document; present when a page selection was used. */
    readonly sourcePages?: number;
    /** One-based source pages that were searched, in extraction order. */
    readonly selected?: readonly number[];
    readonly lines: number;
    /** Whether more matches exist beyond `limit`. */
    readonly truncated: boolean;
    readonly hits: readonly PdfSearchHit[];
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
