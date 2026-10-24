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
     * Start editing (page operations and comments). `date` is a PDF date (`D:YYYYMMDDHHmmSSZ`)
     * for new comments and `/ModDate`; `id_prefix` starts each new comment's unique name. Throws
     * `owner_password_required` when the open password may not change the file, `save_failed`
     * when its structure cannot be read.
     */
    edit(date: string, id_prefix: string): PdfEdit;
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
     * How the document is protected.
     */
    protection(): Protection;
    /**
     * Render a page at `scale` device pixels per point (1 = 72 dpi). Straight-alpha RGBA.
     */
    renderPage(page: number, scale: number): RenderedPage;
    /**
     * A copy protected with AES-256. `permissionsPassword` empty or absent: a random one when
     * anything is denied, else the open password. Throws a `PdfError` (`code`:
     * invalid_password, owner_password_required, save_failed).
     */
    saveProtected(open_password: string, permissions_password: string | null | undefined, allow_print: boolean, allow_copy: boolean, allow_modify: boolean): Uint8Array;
    /**
     * A copy with the protection removed. Throws `owner_password_required` when the document
     * was opened with an open password that is denied anything.
     */
    saveUnprotected(): Uint8Array;
    /**
     * The whole document written back as a new PDF (full rewrite).
     */
    save(): Uint8Array;
    readonly pageCount: number;
}

/**
 * Edits to an open document: page operations and comments. Coordinates are page points in view
 * space (origin top-left, y down, after rotation), as the text layer reports them. Comments use
 * the pages as opened: after a page operation in the same edit they throw `edit_failed`.
 * `save()` returns the original bytes plus one incremental update. Call `free()` when done.
 */
export class PdfEdit {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Delete pages (never every page).
     */
    deletePages(pages: Uint32Array): void;
    /**
     * A new PDF of these pages of the document as edited so far.
     */
    extract(pages: Uint32Array): Uint8Array;
    /**
     * Freehand ink: x, y pairs in `points`, the number of points of each stroke in `counts`.
     */
    ink(page: number, points: Float32Array, counts: Uint32Array, width: number, color: Float32Array, opacity: number, author: string): void;
    /**
     * Insert pages of another PDF at `at` (`pages` empty: all of them); its bookmarks go under
     * one titled `name`. Returns how many pages were inserted.
     */
    insertPages(bytes: Uint8Array, password: string | null | undefined, name: string, pages: Uint32Array, at: number): number;
    /**
     * Highlight, underline, strikeout or squiggly over rectangles (4 numbers each).
     */
    markup(page: number, kind: string, rects: Float32Array, color: Float32Array, opacity: number, contents: string, author: string): void;
    /**
     * Move pages, in their relative order, so they start at position `to`.
     */
    movePages(pages: Uint32Array, to: number): void;
    /**
     * A sticky note whose icon's top-left corner is at (`x`, `y`).
     */
    note(page: number, x: number, y: number, contents: string, author: string, color: Float32Array): void;
    /**
     * Rotate pages by a multiple of 90 degrees, clockwise.
     */
    rotatePages(pages: Uint32Array, degrees: number): void;
    /**
     * The original bytes followed by one incremental update with every edit.
     */
    save(): Uint8Array;
    /**
     * Set (or clear, with "") a document-information entry such as `Producer`.
     */
    setInfo(key: string, value: string): void;
    /**
     * A text box showing `contents` in `rect` (x0, y0, x1, y1).
     */
    textBox(page: number, rect: Float32Array, font_size: number, contents: string, author: string, color: Float32Array): void;
    /**
     * Pages in the document as edited so far.
     */
    readonly pageCount: number;
}

/**
 * How a document is protected. `canChange`: its protection may be removed or changed (opened
 * with the owner password, or the open password is denied nothing).
 */
export class Protection {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly allowCopy: boolean;
    readonly allowModify: boolean;
    readonly allowPrint: boolean;
    readonly canChange: boolean;
    readonly encrypted: boolean;
    readonly openedAsOwner: boolean;
    readonly restricted: boolean;
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
