/**
 * Stable error surface of `@wae/open-pdf`.
 *
 * Codes are contract, not prose: callers compare them, localize around them and
 * never parse the message. Each code also freezes its parameter shape, so a
 * payload is either a typed object for that code or is withheld entirely.
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

/** Frozen parameter shape of each stable code, mirroring the Rust contract. */
export interface ErrorParamsByCode {
    readonly 'session/argument-invalid': {
        readonly reason: string;
        readonly pages?: string;
        readonly arg?: string;
        readonly with?: string;
    };
    readonly 'format/invalid': { readonly reason: string };
    readonly 'format/unsupported': { readonly reason: string };
    readonly 'io/read': { readonly path: string; readonly kind: string };
    readonly 'io/write': { readonly path: string };
    readonly 'io/exists': { readonly path: string };
    readonly 'capability/not-implemented': { readonly feature: string };
    readonly 'binding/failure': Readonly<Record<string, never>>;
}

/** Per-code machine branch: the code and its frozen payload travel together. */
export type OpenPdfFailure = {
    [Code in OpenPdfErrorCode]: { readonly code: Code; readonly params: ErrorParamsByCode[Code] };
}[OpenPdfErrorCode];

/** Optional locators carried by a failure. */
export interface OpenPdfErrorDetails {
    /** File or format involved in the failure. */
    readonly path?: string;
    /** Operation name, e.g. `merge`, `split` or `extractText`. */
    readonly feature?: string;
}

/** Normalized parameters exposed on {@link OpenPdfError}. */
export type OpenPdfErrorParams = Readonly<Record<string, unknown>>;

/** Options accepted by {@link OpenPdfError}. */
export interface OpenPdfErrorOptions {
    readonly recoverable?: boolean;
    readonly details?: OpenPdfErrorDetails;
    readonly params?: OpenPdfErrorParams;
    readonly cause?: unknown;
}

const knownCodes = new Set<string>(errorCodes);

interface ParamShape {
    readonly required: readonly string[];
    readonly optional?: readonly string[];
}

const paramShapes: Readonly<Record<OpenPdfErrorCode, ParamShape>> = {
    'session/argument-invalid': { required: ['reason'], optional: ['pages', 'arg', 'with'] },
    'format/invalid': { required: ['reason'] },
    'format/unsupported': { required: ['reason'] },
    'io/read': { required: ['path', 'kind'] },
    'io/write': { required: ['path'] },
    'io/exists': { required: ['path'] },
    'capability/not-implemented': { required: ['feature'] },
    'binding/failure': { required: [] },
};

/**
 * Strictly parses a code's payload against its frozen shape.
 *
 * Returns `undefined` for an unknown code, a non-object, a missing key, an
 * extra key or a key of the wrong type, so a malformed payload is refused
 * instead of leaking through as unchecked data.
 */
export function parseErrorParams(code: string, payload: unknown): ErrorParamsByCode[OpenPdfErrorCode] | undefined {
    if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) {
        return undefined;
    }
    const shape = paramShapes[code as OpenPdfErrorCode];
    if (shape === undefined) {
        return undefined;
    }
    const record = payload as Record<string, unknown>;
    const allowed = new Set<string>([...shape.required, ...(shape.optional ?? [])]);
    for (const key of Object.keys(record)) {
        if (!allowed.has(key)) {
            return undefined;
        }
    }
    for (const key of shape.required) {
        if (typeof record[key] !== 'string') {
            return undefined;
        }
    }
    for (const key of shape.optional ?? []) {
        if (record[key] !== undefined && typeof record[key] !== 'string') {
            return undefined;
        }
    }
    return Object.freeze({ ...record }) as ErrorParamsByCode[OpenPdfErrorCode];
}

/**
 * Normalizes a payload for rendering: a valid shape is kept, anything else
 * degrades to an empty map so copy falls back to the binding text.
 */
export function normalizeErrorParams(code: string, payload: unknown): OpenPdfErrorParams {
    return parseErrorParams(code, payload) ?? {};
}

/** Every failure thrown or rejected by this package. */
export class OpenPdfError extends Error {
    /** Stable, comparable code. */
    readonly code: OpenPdfErrorCode;
    /** Whether retrying with different input or state can succeed. */
    readonly recoverable: boolean;
    /** Optional locators. */
    readonly details: OpenPdfErrorDetails;
    /** Typed parameters of the code, normalized against its frozen shape. */
    readonly params: OpenPdfErrorParams;
    /** Machine branch: set only when the code is known and its payload matches. */
    readonly failure: OpenPdfFailure | undefined;

    constructor(code: string, message: string, options: OpenPdfErrorOptions = {}) {
        super(message, options.cause === undefined ? undefined : { cause: options.cause });
        this.name = 'OpenPdfError';
        this.code = code as OpenPdfErrorCode;
        this.recoverable = options.recoverable ?? false;
        this.details = Object.freeze({ ...options.details });
        const provided = options.params === undefined ? defaultParams(code, message, options.details) : options.params;
        const params = normalizeErrorParams(code, provided);
        this.params = Object.freeze(params);
        this.failure = freezeFailure(code, params);
    }
}

/** Derives the parameters a product-built failure carries when none were given. */
function defaultParams(code: string, message: string, details: OpenPdfErrorDetails | undefined): OpenPdfErrorParams {
    switch (code) {
        case 'session/argument-invalid':
        case 'format/invalid':
        case 'format/unsupported':
            return { reason: message };
        case 'capability/not-implemented':
            return details?.feature === undefined ? {} : { feature: details.feature };
        default:
            return {};
    }
}

function freezeFailure(code: string, params: OpenPdfErrorParams): OpenPdfFailure | undefined {
    if (!knownCodes.has(code)) {
        return undefined;
    }
    const parsed = parseErrorParams(code, params);
    return parsed === undefined ? undefined : ({ code, params: parsed } as OpenPdfFailure);
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
 * `reason`, plus `pages`, `arg` or `with` for the rejected selection), so these
 * templates render localized copy from them. Failures without a matching
 * payload — text produced upstream — keep the binding's own text; the
 * machine-readable `code` is what the contract guarantees, and it never depends
 * on locale.
 */
const messageCatalog: Readonly<Record<string, Readonly<Record<string, string>>>> = {
    en: {
        'session/argument-invalid': 'the request was rejected: {reason}',
        'format/invalid': 'the input is invalid: {reason}',
        'format/unsupported': 'the request is not supported: {reason}',
        'io/read': 'could not read `{path}` ({kind})',
        'io/write': 'could not publish `{path}`',
        'io/exists': '`{path}` already exists; pass overwrite to replace it',
        'capability/not-implemented': 'the `{feature}` capability is not implemented yet',
        'binding/failure': 'the native binding reported a failure without a stable code',
    },
    'zh-CN': {
        'session/argument-invalid': '请求被拒绝：{reason}',
        'format/invalid': '输入无效：{reason}',
        'format/unsupported': '该请求不受支持：{reason}',
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
