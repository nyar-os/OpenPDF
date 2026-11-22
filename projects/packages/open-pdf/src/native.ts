/**
 * Native addon discovery.
 *
 * The addon only maps values: parsing, writing and page copying stay in the
 * engine. An absent addon is a declarative state, so calls report
 * `capability/not-implemented` instead of inventing results.
 */

import { createRequire } from 'node:module';

const requireNative = createRequire(import.meta.url);

/** Orthogonal exports of the built addon. Every call returns a JSON string. */
export interface NativeBinding {
    version(): string;
    capabilities(): string;
    pdf_inspect(path: string): string;
    pdf_outline(path: string): string;
    pdf_merge(inputsJson: string, out: string, overwrite: boolean, title: string): string;
    pdf_split(path: string, ranges: string, outDir: string): string;
    pdf_split_every(path: string, every: number, outDir: string): string;
    pdf_arrange(path: string, out: string, opsJson: string): string;
    pdf_delete_pages(path: string, pages: string, out: string, overwrite: boolean): string;
    pdf_duplicate_pages(path: string, pages: string, times: number, out: string, overwrite: boolean): string;
    pdf_rotate_pages(path: string, pages: string, degrees: number, out: string, overwrite: boolean): string;
    pdf_set_page_boxes(path: string, pages: string, cropJson: string, out: string, overwrite: boolean): string;
    pdf_set_metadata(path: string, out: string, patchJson: string): string;
    pdf_set_bookmarks(path: string, out: string, requestJson: string): string;
    pdf_fill_form(path: string, out: string, requestJson: string): string;
    pdf_create(requestJson: string): string;
    pdf_extract_text(path: string): string;
    pdf_extract_text_pages(path: string, ranges: string): string;
    pdf_search(
        path: string,
        query: string,
        limit: number | undefined,
        matchCase: boolean,
        wholeWord: boolean,
        skip: number,
    ): string;
    pdf_search_pages(
        path: string,
        query: string,
        ranges: string,
        limit: number | undefined,
        matchCase: boolean,
        wholeWord: boolean,
        skip: number,
    ): string;
    pdf_extract_images(path: string, outDir: string): string;
    pdf_extract_images_pages(path: string, outDir: string, ranges: string): string;
    pdf_inspect_bytes(bytes: Uint8Array): string;
    pdf_extract_text_bytes(bytes: Uint8Array): string;
    pdf_extract_images_bytes(bytes: Uint8Array, outDir: string, stem: string): string;
    pdf_stamp(path: string, out: string, requestJson: string): string;
}

let cached: NativeBinding | null | undefined;

/** Loads the addon once, or returns `undefined` when it is not installed. */
export function nativeBinding(): NativeBinding | undefined {
    if (cached === undefined) {
        cached = load();
    }
    return cached ?? undefined;
}

function load(): NativeBinding | null {
    for (const candidate of candidates()) {
        try {
            return requireNative(candidate) as NativeBinding;
        } catch (error) {
            if (candidate === explicitPath()) {
                throw error;
            }
            if (!isModuleNotFound(error, candidate)) {
                throw error;
            }
        }
    }
    return null;
}

function candidates(): string[] {
    const explicit = explicitPath();
    if (explicit) {
        return [explicit];
    }
    return [`@wae/open-pdf-${process.platform}-${process.arch}`];
}

function explicitPath(): string | undefined {
    const value = process.env.OPEN_PDF_NATIVE;
    return value !== undefined && value.length > 0 ? value : undefined;
}

function isModuleNotFound(error: unknown, candidate: string): boolean {
    if (typeof error !== 'object' || error === null) {
        return false;
    }
    const code = (error as { code?: unknown }).code;
    const message = (error as Error).message.replace(/\\/g, '/');
    return code === 'MODULE_NOT_FOUND' && message.includes(candidate);
}
