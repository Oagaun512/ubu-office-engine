// Reviewed TypeScript contract of the ubu-pdf browser build. CI fails when wasm-bindgen's
// generated ubu_pdf.d.ts differs from this file (comments and whitespace ignored; the
// InitOutput interface of raw wasm exports is checked against the .wasm instead).
// Changing it is an API change for Ubunex: review the diff, then update it here.
/* tslint:disable */
/* eslint-disable */

/**
 * A page size in PDF points.
 */
export class PageSize {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly heightPt: number;
    readonly widthPt: number;
}

/**
 * An open PDF document. Call `free()` when done; the bytes live in wasm memory until then.
 */
export class PdfDocument {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Find `query` on a page. Returns 5 numbers per box: match index, x0, y0, x1, y1 (page
     * points, y down). A match that wraps onto the next line has one box per line.
     */
    findText(page: number, query: string, case_sensitive: boolean, whole_words: boolean): Float32Array;
    /**
     * Open a PDF. Throws a `PdfError` (`code`: invalid, needs_password, wrong_password, unsupported).
     */
    static open(bytes: Uint8Array, password?: string | null): PdfDocument;
    /**
     * Displayed page size in points (after rotation).
     */
    pageSize(page: number): PageSize;
    /**
     * The page's text as lines with boxes, for a selectable text layer over the rendered page.
     */
    pageTextLayer(page: number): TextLayer;
    /**
     * The page's text in reading order, lines separated by `\n`.
     */
    pageText(page: number): string;
    /**
     * Render a page at `scale` device pixels per point (1 = 72 dpi). Straight-alpha RGBA.
     */
    renderPage(page: number, scale: number): RenderedPage;
    /**
     * The whole document written back as a new PDF (full rewrite).
     */
    save(): Uint8Array;
    readonly pageCount: number;
}

/**
 * One rendered page.
 */
export class RenderedPage {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Straight-alpha RGBA8, row-major: `new ImageData(page.rgba(), page.width, page.height)`.
     */
    rgba(): Uint8ClampedArray;
    /**
     * Non-fatal conditions that made the render partial.
     */
    warnings(): string[];
    readonly height: number;
    readonly width: number;
}

/**
 * A page's text layer: `lineCount` lines, each with its text and 5 numbers in `boxes()`:
 * x0, y0, x1, y1 (page points, origin top-left, y down) and the writing angle in radians.
 */
export class TextLayer {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * 5 numbers per line: x0, y0, x1, y1, angle.
     */
    boxes(): Float32Array;
    /**
     * All lines' text, in order.
     */
    lines(): string[];
    readonly lineCount: number;
}

/**
 * The engine build: this repository's commit and the PDF engine fork's commit.
 */
export function engineVersion(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;


export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
