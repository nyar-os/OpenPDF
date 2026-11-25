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
    'binding/failure',
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

/** Machine-readable per-code parameters delivered by the binding contract. */
export type OpenPdfErrorParams = Readonly<Record<string, unknown>>;

/** Options accepted by {@link OpenPdfError}. */
export interface OpenPdfErrorOptions {
    readonly recoverable?: boolean;
    readonly details?: OpenPdfErrorDetails;
    readonly params?: OpenPdfErrorParams;
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
    /** Typed parameters of the code, as delivered by the binding contract. */
    readonly params: OpenPdfErrorParams;

    constructor(code: OpenPdfErrorCode, message: string, options: OpenPdfErrorOptions = {}) {
        super(message, options.cause === undefined ? undefined : { cause: options.cause });
        this.name = 'OpenPdfError';
        this.code = code;
        this.recoverable = options.recoverable ?? false;
        this.details = Object.freeze({ ...options.details });
        this.params = Object.freeze({ ...options.params });
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

/**
 * Product copy for each stable code.
 *
 * Failures built by the product surface carry machine-readable `params`
 * (`io/exists` names the blocked `path`; `session/argument-invalid` names the
 * `reason`, plus `pages` or `arg` for the rejected selection), so these
 * templates render localized copy from them. Failures without params — text
 * produced upstream — keep the binding's own text; the machine-readable `code`
 * is what the contract guarantees, and it never depends on locale.
 */
const messageCatalog: Readonly<Record<string, Readonly<Record<string, string>>>> = {
    en: {
        'session/argument-invalid': 'the request was rejected: {reason}',
        'format/invalid': 'the `{format}` input is invalid: {reason}',
        'format/unsupported': '`{format}` is not an offered format',
        'io/read': 'could not read `{path}` ({kind})',
        'io/write': 'could not publish `{path}`',
        'io/exists': '`{path}` already exists; pass overwrite to replace it',
        'capability/not-implemented': 'the `{feature}` capability is not implemented yet',
        'binding/failure': 'the native binding reported a failure without a stable code',
    },
    'zh-CN': {
        'session/argument-invalid': '请求被拒绝：{reason}',
        'format/invalid': '`{format}` 输入无效：{reason}',
        'format/unsupported': '`{format}` 不是可提供的格式',
        'io/read': '无法读取 `{path}`（{kind}）',
        'io/write': '无法发布 `{path}`',
        'io/exists': '`{path}` 已存在；传入 overwrite 以替换',
        'capability/not-implemented': '`{feature}` 能力尚未实现',
        'binding/failure': '原生绑定返回了没有稳定码的失败',
    },
};

let currentLocale = 'en';

/** Selects the locale used to render error copy; `zh*` maps to `zh-CN`. */
export function setErrorLocale(tag: string): void {
    currentLocale = tag.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en';
}

/**
 * Renders the product copy for a code and its params.
 *
 * Unknown codes, missing params and missing translations fall back to the
 * binding's own text, so a replaced or removed native message never changes
 * which branch a caller takes: that branch is decided by `code`.
 */
export function renderErrorCopy(code: string, params: OpenPdfErrorParams, fallback: string): string {
    const template = messageCatalog[currentLocale]?.[code];
    if (template === undefined) {
        return fallback;
    }
    let complete = true;
    const rendered = template.replace(/\{(\w+)\}/g, (match, key: string) => {
        if (params[key] === undefined) {
            complete = false;
            return match;
        }
        return String(params[key]);
    });
    return complete ? rendered : fallback;
}
