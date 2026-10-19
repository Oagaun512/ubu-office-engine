// Reviewed TypeScript contract of the ubu-sheets browser build. CI fails when wasm-bindgen's
// generated ubu_sheets.d.ts differs from this file (comments and whitespace ignored; the
// InitOutput interface of raw wasm exports is checked against the .wasm instead).
// Changing it is an API change for Ubunex: review the diff, then update it here.
/* tslint:disable */
/* eslint-disable */

/**
 * An open workbook. Call `free()` when done.
 */
export class SheetBook {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * What the formula bar shows for a cell.
     */
    cellInput(sheet: number, row: number, col: number): string;
    /**
     * A chart laid out at `width` × `height` CSS px, as JSON drawing primitives.
     */
    chart(sheet: number, index: number, width: number, height: number): string;
    /**
     * The active sheet as PDF.
     */
    exportPdf(): Uint8Array;
    /**
     * A sheet's layout, as JSON: sizes (CSS px at 100%), merges, frozen panes, used range,
     * view settings, chart and image anchors, the default style.
     */
    layout(sheet: number): string;
    /**
     * Open a workbook; `name` gives the format by extension when the bytes don't say. Throws a
     * `SheetsError` (`code`: invalid, needs_password, wrong_password, unsupported).
     */
    static open(bytes: Uint8Array, name: string, password?: string | null): SheetBook;
    /**
     * The first character on a sheet the PDF export cannot write, or undefined when it can
     * write the whole sheet.
     */
    pdfUnwritable(sheet: number): string | undefined;
    /**
     * Run an engine command (`cell.set`, `selection.set`, `home.bold`, `edit.undo`, …) with its
     * JSON parameters. Returns JSON: `{"result": <the command's result>, "changed": bool, "rev":
     * n, "sheet": n}`. Throws a `SheetsError` (`code`: invalid_command).
     */
    run(command: string, params_json: string): string;
    /**
     * One sheet as CSV (UTF-8).
     */
    saveCsv(sheet: number): Uint8Array;
    /**
     * The workbook as XLSX.
     */
    saveXlsx(): Uint8Array;
    /**
     * The selection, as JSON: `{"sheet", "active": [row, col], "ranges": [[r0, c0, r1, c1]…]}`.
     */
    selection(): string;
    /**
     * The sheets, as JSON: `[{"name", "visibility", "tabColor"}]`.
     */
    sheets(): string;
    /**
     * What opening noticed (a decrypted workbook, features read but not kept, …).
     */
    warnings(): string[];
    /**
     * The cells of a window (inclusive), as JSON `{"styles", "cells"}`; `digitPx` is the width
     * of a digit in the sheet's default font, for fitting General numbers (0 = off).
     */
    window(sheet: number, row0: number, col0: number, row1: number, col1: number, digit_px: number): string;
    readonly activeSheet: number;
    readonly canRedo: boolean;
    readonly canUndo: boolean;
    readonly isDirty: boolean;
}

/**
 * The engine build: this repository's commit and the gridcraft fork's commit.
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
