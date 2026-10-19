// Reviewed TypeScript contract of the ubu-docx browser build. CI fails when wasm-bindgen's
// generated ubu_docx.d.ts differs from this file (comments and whitespace ignored; the
// InitOutput interface of raw wasm exports is checked against the .wasm instead).
// Changing it is an API change for Ubunex: review the diff, then update it here.
/* tslint:disable */
/* eslint-disable */

/**
 * An open document. Call `free()` when done.
 */
export class DocxDocument {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * The caret as 4 numbers: page, x, top, height (page points); empty when it is not on a
     * laid-out line.
     */
    caret(): Float32Array;
    /**
     * The text the last `edit.copy` or `edit.cut` put on the clipboard.
     */
    clipboardText(): string;
    /**
     * The document as PDF.
     */
    exportPdf(): Uint8Array;
    /**
     * Find `query` on a page. Returns 5 numbers per box: match index, x0, y0, x1, y1 (page
     * points, y down).
     */
    findText(page: number, query: string, case_sensitive: boolean, whole_words: boolean): Float32Array;
    /**
     * Open a document; `name` gives the format by extension. Throws a `DocxError` (`code`:
     * invalid, needs_password, wrong_password).
     */
    static open(bytes: Uint8Array, name: string, password?: string | null): DocxDocument;
    /**
     * Page size in points.
     */
    pageSize(page: number): PageSize;
    /**
     * The page's text as lines with boxes, for a selectable text layer over the rendered page.
     */
    pageTextLayer(page: number): TextLayer;
    /**
     * The page's text in drawing order, lines separated by `\n`.
     */
    pageText(page: number): string;
    /**
     * Lay the document out again (after `addFont`).
     */
    relayout(): void;
    /**
     * Render a page at `scale` device pixels per point (1 = 72 dpi). Straight-alpha RGBA.
     */
    renderPage(page: number, scale: number): RenderedPage;
    /**
     * Run an editing command (`text.insert`, `caret.set`, `edit.undo`, …) with its JSON
     * parameters. Returns JSON: `{"result": <the command's result>, "changed": bool, "rev": n,
     * "pageCount": n}`. Throws a `DocxError` (`code`: invalid_command).
     */
    run(command: string, params_json: string): string;
    /**
     * The document written back as DOCX.
     */
    saveDocx(): Uint8Array;
    /**
     * The selection as 5 numbers per box: page, x0, y0, x1, y1 (empty for a plain caret).
     */
    selectionRects(): Float32Array;
    readonly canRedo: boolean;
    readonly canUndo: boolean;
    readonly isDirty: boolean;
    readonly pageCount: number;
}

/**
 * A page size in points.
 */
export class PageSize {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly heightPt: number;
    readonly widthPt: number;
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
 * Make a font available to every document in this instance (TTF, OTF, WOFF, WOFF2). Returns
 * how many faces it added; call `relayout()` on open documents afterwards.
 */
export function addFont(bytes: Uint8Array): number;

/**
 * The engine build: this repository's commit and the wordcraft fork's commit.
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
