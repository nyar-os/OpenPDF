/**
 * Stable error surface of `@wae/open-pdf`.
 *
 * Codes are contract, not prose: callers compare them, localize around them and
 * never parse the message.
 */

/** Every stable code this package may report. */
export const errorCodes = [
    'session/argument-invalid',
    'format/invalid',
    'format/unsupported',
    'io/read',
    'io/write',
    'io/exists',
    'capability/not-implemented',
] as const;

/** Union of {@link errorCodes}. */
export type OpenPdfErrorCode = (typeof errorCodes)[number];

/** Optional locators carried by a failure. */
export interface OpenPdfErrorDetails {
    /** File or format involved in the failure. */
    readonly path?: string;
    /** Operation name, e.g. `merge`, `split` or `extractText`. */
    readonly feature?: string;
}

/** Options accepted by {@link OpenPdfError}. */
export interface OpenPdfErrorOptions {
    readonly recoverable?: boolean;
    readonly details?: OpenPdfErrorDetails;
    readonly cause?: unknown;
}

/** Every failure thrown or rejected by this package. */
export class OpenPdfError extends Error {
    /** Stable, comparable code. */
    readonly code: OpenPdfErrorCode;
    /** Whether retrying with different input or state can succeed. */
    readonly recoverable: boolean;
    /** Optional locators. */
    readonly details: OpenPdfErrorDetails;

    constructor(code: OpenPdfErrorCode, message: string, options: OpenPdfErrorOptions = {}) {
        super(message, options.cause === undefined ? undefined : { cause: options.cause });
        this.name = 'OpenPdfError';
        this.code = code;
        this.recoverable = options.recoverable ?? false;
        this.details = Object.freeze({ ...options.details });
    }
}

/** Reported when the native addon is not installed. */
export class NotImplementedError extends OpenPdfError {
    constructor(feature: string, message = `\`${feature}\` needs the native OpenPDF addon`) {
        super('capability/not-implemented', message, { recoverable: false, details: { feature } });
        this.name = 'NotImplementedError';
    }
}

/** Narrows an unknown thrown value to {@link OpenPdfError}. */
export function isOpenPdfError(value: unknown): value is OpenPdfError {
    return value instanceof OpenPdfError;
}
