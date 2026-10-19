// Reviewed TypeScript contract of the ubu-slides browser build. CI fails when wasm-bindgen's
// generated ubu_slides.d.ts differs from this file (comments and whitespace ignored; the
// InitOutput interface of raw wasm exports is checked against the .wasm instead).
// Changing it is an API change for Ubunex: review the diff, then update it here.
/* tslint:disable */
/* eslint-disable */

/**
 * An open presentation. Call `free()` when done.
 */
export class SlideDeck {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Add a font file (TTF/OTF/TTC) for every presentation; returns how many faces it added.
     */
    static addFont(bytes: Uint8Array): number;
    /**
     * The presentation as PDF.
     */
    exportPdf(): Uint8Array;
    /**
     * The presentation, as JSON: `{"width", "height"}` in points, `"slides"`, `"fonts"`,
     * `"missingFonts"`.
     */
    info(): string;
    /**
     * A slide's speaker notes.
     */
    notes(slide: number): string;
    /**
     * The input method's composition, accepted.
     */
    commitText(text: string): string;
    /**
     * An input method's composition so far; `active_start`/`active_end` (characters in `text`)
     * mark the part being converted, when there is one.
     */
    composeText(text: string, active_start?: number | null, active_end?: number | null): string;
    /**
     * A key press (`Enter`, `ArrowLeft`, `Delete`, …, or a character with Ctrl). `result.handled`
     * says whether the engine used it. Returns the same JSON as `run`.
     */
    key(key: string, mods: number): string;
    /**
     * Open a presentation (`.pptx`, `.ppsx`, `.potx`); `name` is for messages. Throws a
     * `SlidesError` (`code`: invalid, unsupported).
     */
    static open(bytes: Uint8Array, name: string): SlideDeck;
    /**
     * What an editor draws over the current slide, as JSON in points: selected shapes (corners,
     * handles, rotate handle), the text being edited (caret, caret colour, selected text),
     * the rubber band and what the pointer is over.
     */
    overlay(): string;
    /**
     * A pointer event on the current slide, in points: `kind` is `down`, `drag`, `up`, `move`,
     * `double`, `triple` or `context`; `mods` 1 Shift, 2 Alt, 4 Ctrl/Cmd; `tol` the hit
     * tolerance in points. Returns the same JSON as `run`.
     */
    pointer(kind: string, x: number, y: number, mods: number, tol: number): string;
    /**
     * Render a slide at `scale` pixels per point; `edit` draws it as an editor does
     * (placeholder prompts and outlines).
     */
    render(slide: number, scale: number, edit: boolean): SlidePicture;
    /**
     * Run an engine command with its JSON parameters. Returns JSON: `{"result": <the command's
     * result>, "changed": bool, "rev": n, "slide": n, "slides": n}`. Throws a `SlidesError`
     * (`code`: invalid_command).
     */
    run(command: string, params_json: string): string;
    /**
     * The presentation as PPTX.
     */
    savePptx(): Uint8Array;
    /**
     * The selection, as JSON (the engine's own shape).
     */
    selection(): string;
    /**
     * The slide's own shapes, back to front, as JSON: `[{"id", "box", "group", "parent"}]`.
     */
    shapes(slide: number): string;
    /**
     * The text on a slide, in drawing order.
     */
    slideText(slide: number): string;
    /**
     * The slides, as JSON: `[{"id", "hidden", "notes"}]`.
     */
    slides(): string;
    /**
     * The text drawn on a slide, as JSON spans in points: `[{"text", "x", "y", "x2", "y2",
     * "size"}]` ((x, y) → (x2, y2) is the baseline).
     */
    textLayer(slide: number): string;
    /**
     * Type text at the caret (or start editing the selected shape with it).
     */
    typeText(text: string): string;
    /**
     * What opening noticed (slides left out, media that won't play here).
     */
    warnings(): string[];
    readonly canRedo: boolean;
    readonly canUndo: boolean;
    readonly isDirty: boolean;
}

/**
 * A rendered slide: straight RGBA pixels, row by row (for `ImageData`). Call `free()` when done.
 */
export class SlidePicture {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly height: number;
    /**
     * The pixels (a copy; take them once).
     */
    readonly pixels: Uint8Array;
    readonly width: number;
}

/**
 * The engine build: this repository's commit and the deckcraft fork's commit.
 */
export function engineVersion(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;


export type SyncInitInput = BufferSource | WebAssembly.Module;

export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
