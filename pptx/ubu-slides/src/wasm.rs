//! The browser binding: `wasm-bindgen` classes over [`crate::Deck`]. Type conversion only;
//! every behaviour lives in the plain-Rust API so native tests cover it.
//!
//! The generated TypeScript must match `contract/ubu_slides.d.ts` (checked in CI), so renaming
//! anything here is a contract change for Ubunex.

use wasm_bindgen::prelude::*;

use crate::{Deck, SlidesError};

/// A thrown `Error` with `name = "SlidesError"` and a machine-readable `code` (see `ErrorCode`).
fn js_error(e: SlidesError) -> JsValue {
    let err = js_sys::Error::new(&e.message);
    err.set_name("SlidesError");
    // Setting a property on a fresh Error object cannot fail.
    let _ = js_sys::Reflect::set(&err, &JsValue::from_str("code"), &JsValue::from_str(e.code.code()));
    err.into()
}

/// Print a panic's message and place to the console before the instance aborts (the browser
/// build is `panic = "abort"`, so the trap itself says only "unreachable"). Installed once, before
/// the engine's own hook, which chains to it.
fn install_panic_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let at = info.location().map(|l| format!(" at {}:{}", l.file(), l.line())).unwrap_or_default();
            let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned());
            let line = format!("ubu-slides panicked{at}: {}", msg.as_deref().unwrap_or("(no message)"));
            let console = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("console")).ok();
            if let Some(error) = console.as_ref().and_then(|c| js_sys::Reflect::get(c, &JsValue::from_str("error")).ok())
                && let Ok(f) = error.dyn_into::<js_sys::Function>()
            {
                let _ = f.call1(console.as_ref().unwrap_or(&JsValue::UNDEFINED), &JsValue::from_str(&line));
            }
        }));
    });
}

/// A rendered slide: straight RGBA pixels, row by row (for `ImageData`). Call `free()` when done.
#[wasm_bindgen]
pub struct SlidePicture {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

#[wasm_bindgen]
impl SlidePicture {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels (a copy; take them once).
    #[wasm_bindgen(getter)]
    pub fn pixels(&self) -> Vec<u8> {
        self.pixels.clone()
    }
}

/// An open presentation. Call `free()` when done.
#[wasm_bindgen]
pub struct SlideDeck {
    inner: Deck,
}

#[wasm_bindgen]
impl SlideDeck {
    /// Open a presentation (`.pptx`, `.ppsx`, `.potx`); `name` is for messages. Throws a
    /// `SlidesError` (`code`: invalid, unsupported).
    pub fn open(bytes: &[u8], name: &str) -> Result<SlideDeck, JsValue> {
        install_panic_hook();
        Deck::open(bytes, name).map(|inner| SlideDeck { inner }).map_err(js_error)
    }

    /// What opening noticed (slides left out, media that won't play here).
    pub fn warnings(&self) -> Vec<String> {
        self.inner.warnings().to_vec()
    }

    /// The presentation, as JSON: `{"width", "height"}` in points, `"slides"`, `"fonts"`,
    /// `"missingFonts"`.
    pub fn info(&self) -> String {
        self.inner.info()
    }

    /// The slides, as JSON: `[{"id", "hidden", "notes"}]`.
    pub fn slides(&self) -> String {
        self.inner.slides()
    }

    /// Add a font file (TTF/OTF/TTC) for every presentation; returns how many faces it added.
    #[wasm_bindgen(js_name = addFont)]
    pub fn add_font(bytes: Vec<u8>) -> u32 {
        Deck::add_font(bytes) as u32
    }

    /// Render a slide at `scale` pixels per point; `edit` draws it as an editor does
    /// (placeholder prompts and outlines).
    pub fn render(&self, slide: u32, scale: f64, edit: bool) -> Result<SlidePicture, JsValue> {
        let img = self.inner.render(slide as usize, scale, edit).map_err(js_error)?;
        Ok(SlidePicture { width: img.width, height: img.height, pixels: img.pixels })
    }

    /// The text drawn on a slide, as JSON spans in points: `[{"text", "x", "y", "x2", "y2",
    /// "size"}]` ((x, y) → (x2, y2) is the baseline).
    #[wasm_bindgen(js_name = textLayer)]
    pub fn text_layer(&self, slide: u32) -> Result<String, JsValue> {
        self.inner.text_layer(slide as usize).map_err(js_error)
    }

    /// The slide's own shapes, back to front, as JSON: `[{"id", "box", "group", "parent"}]`.
    pub fn shapes(&self, slide: u32) -> Result<String, JsValue> {
        self.inner.shapes(slide as usize).map_err(js_error)
    }

    /// The text on a slide, in drawing order.
    #[wasm_bindgen(js_name = slideText)]
    pub fn slide_text(&self, slide: u32) -> Result<String, JsValue> {
        self.inner.slide_text(slide as usize).map_err(js_error)
    }

    /// A slide's speaker notes.
    pub fn notes(&self, slide: u32) -> Result<String, JsValue> {
        self.inner.notes(slide as usize).map_err(js_error)
    }

    /// Run an engine command with its JSON parameters. Returns JSON: `{"result": <the command's
    /// result>, "changed": bool, "rev": n, "slide": n, "slides": n}`. Throws a `SlidesError`
    /// (`code`: invalid_command).
    pub fn run(&mut self, command: &str, params_json: &str) -> Result<String, JsValue> {
        self.inner.run(command, params_json).map(edit_json).map_err(js_error)
    }

    /// A pointer event on the current slide, in points: `kind` is `down`, `drag`, `up`, `move`,
    /// `double`, `triple` or `context`; `mods` 1 Shift, 2 Alt, 4 Ctrl/Cmd; `tol` the hit
    /// tolerance in points. Returns the same JSON as `run`.
    pub fn pointer(&mut self, kind: &str, x: f64, y: f64, mods: u32, tol: f64) -> Result<String, JsValue> {
        self.inner.pointer(kind, x, y, mods, tol).map(edit_json).map_err(js_error)
    }

    /// A key press (`Enter`, `ArrowLeft`, `Delete`, …, or a character with Ctrl). `result.handled`
    /// says whether the engine used it. Returns the same JSON as `run`.
    pub fn key(&mut self, key: &str, mods: u32) -> Result<String, JsValue> {
        self.inner.key(key, mods).map(edit_json).map_err(js_error)
    }

    /// Type text at the caret (or start editing the selected shape with it).
    #[wasm_bindgen(js_name = typeText)]
    pub fn type_text(&mut self, text: &str) -> Result<String, JsValue> {
        self.inner.type_text(text).map(edit_json).map_err(js_error)
    }

    /// An input method's composition so far; `active_start`/`active_end` (characters in `text`)
    /// mark the part being converted, when there is one.
    #[wasm_bindgen(js_name = composeText)]
    pub fn compose_text(&mut self, text: &str, active_start: Option<u32>, active_end: Option<u32>) -> Result<String, JsValue> {
        let active = active_start.zip(active_end).map(|(a, b)| (a as usize, b as usize));
        self.inner.compose_text(text, active).map(edit_json).map_err(js_error)
    }

    /// The input method's composition, accepted.
    #[wasm_bindgen(js_name = commitText)]
    pub fn commit_text(&mut self, text: &str) -> Result<String, JsValue> {
        self.inner.commit_text(text).map(edit_json).map_err(js_error)
    }

    /// What an editor draws over the current slide, as JSON in points: selected shapes (corners,
    /// handles, rotate handle), the text being edited (caret, caret colour, selected text),
    /// the rubber band and what the pointer is over.
    pub fn overlay(&self) -> String {
        self.inner.overlay()
    }

    /// The selection, as JSON (the engine's own shape).
    pub fn selection(&self) -> String {
        self.inner.selection()
    }

    #[wasm_bindgen(getter, js_name = isDirty)]
    pub fn is_dirty(&self) -> bool {
        self.inner.is_dirty()
    }

    #[wasm_bindgen(getter, js_name = canUndo)]
    pub fn can_undo(&self) -> bool {
        self.inner.can_undo()
    }

    #[wasm_bindgen(getter, js_name = canRedo)]
    pub fn can_redo(&self) -> bool {
        self.inner.can_redo()
    }

    /// The presentation as PPTX.
    #[wasm_bindgen(js_name = savePptx)]
    pub fn save_pptx(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.save_pptx().map_err(js_error)
    }

    /// The presentation as PDF.
    #[wasm_bindgen(js_name = exportPdf)]
    pub fn export_pdf(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.export_pdf().map_err(js_error)
    }
}

/// What an edit did, as JSON: `{"result", "changed", "rev", "slide", "slides"}`.
fn edit_json(r: crate::EditResult) -> String {
    format!("{{\"result\":{},\"changed\":{},\"rev\":{},\"slide\":{},\"slides\":{}}}", r.result, r.changed, r.rev, r.slide, r.slides)
}

/// The engine build: this repository's commit and the deckcraft fork's commit.
#[wasm_bindgen(js_name = engineVersion)]
pub fn engine_version() -> String {
    format!(
        "ubu-office-engine {} (slides engine {})",
        option_env!("UBU_ENGINE_COMMIT").unwrap_or("dev"),
        option_env!("UBU_SLIDES_UPSTREAM_COMMIT").unwrap_or("dev")
    )
}
