/**
 * Format classification for `@wae/open-pdf`.
 *
 * The product handles one container: the fixed-layout PDF file. Classification
 * is not a support claim; `capabilities()` reports what the installed engine can
 * actually read, write and extract.
 */

/** Every extension this product classifies. */
export const documentFormats = ['pdf'] as const;

/** Union of {@link documentFormats}. */
export type PdfFormat = (typeof documentFormats)[number];

/** What a format is, independent of whether it is implemented. */
export interface PdfFormatInfo {
    readonly format: PdfFormat;
    readonly family: 'fixed';
    readonly markup: 'fixed';
    readonly encrypted: boolean;
    readonly opensAs: 'fixed-document';
}

const registry: Readonly<Record<PdfFormat, Omit<PdfFormatInfo, 'format'>>> = Object.freeze({
    pdf: { family: 'fixed', markup: 'fixed', encrypted: false, opensAs: 'fixed-document' },
});

/** Returns the full classification of one format. */
export function formatInfo(format: PdfFormat): PdfFormatInfo {
    return { format, ...registry[format] };
}

/** Extracts a classified extension from a path, ignoring case. */
export function formatFromPath(path: string): PdfFormat | undefined {
    const dot = path.lastIndexOf('.');
    if (dot < 0) {
        return undefined;
    }
    const extension = path.slice(dot + 1).toLowerCase();
    return extension === 'pdf' ? 'pdf' : undefined;
}

/** Whether a path carries the PDF extension. */
export function isPdfPath(path: string): boolean {
    return formatFromPath(path) === 'pdf';
}
