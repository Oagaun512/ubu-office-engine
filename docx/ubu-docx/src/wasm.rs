//! The browser binding: `wasm-bindgen` classes over [`crate::Document`]. Type conversion only;
//! every behaviour lives in the plain-Rust API so native tests cover it.
//!
//! The generated TypeScript must match `contract/ubu_docx.d.ts` (checked in CI), so renaming
//! anything here is a contract change for Ubunex.

use wasm_bindgen::Clamped;
use wasm_bindgen::prelude::*;

use crate::{Document, DocxError};

/// A thrown `Error` with `name = "DocxError"` and a machine-readable `code` (see `ErrorCode`).
fn js_error(e: DocxError) -> JsValue {
    let err = js_sys::Error::new(&e.message);
    err.set_name("DocxError");
    // Setting a property on a fresh Error object cannot fail.
    let _ = js_sys::Reflect::set(&err, &JsValue::from_str("code"), &JsValue::from_str(e.code.code()));
    err.into()
}

/// An open document. Call `free()` when done.
#[wasm_bindgen]
pub struct DocxDocument {
    inner: Document,
}

#[wasm_bindgen]
impl DocxDocument {
    /// Open a document; `name` gives the format by extension. Throws a `DocxError` (`code`:
    /// invalid, needs_password, wrong_password).
    pub fn open(bytes: &[u8], name: &str, password: Option<String>) -> Result<DocxDocument, JsValue> {
        Document::open(bytes, name, password.as_deref()).map(|inner| DocxDocument { inner }).map_err(js_error)
    }

    #[wasm_bindgen(getter, js_name = pageCount)]
    pub fn page_count(&self) -> u32 {
        self.inner.page_count() as u32
    }

    /// Page size in points.
    #[wasm_bindgen(js_name = pageSize)]
    pub fn page_size(&self, page: u32) -> Result<PageSize, JsValue> {
        let s = self.inner.page_size(page as usize).map_err(js_error)?;
        Ok(PageSize { width_pt: s.width_pt, height_pt: s.height_pt })
    }

    /// Render a page at `scale` device pixels per point (1 = 72 dpi). Straight-alpha RGBA.
    #[wasm_bindgen(js_name = renderPage)]
    pub fn render_page(&self, page: u32, scale: f32) -> Result<RenderedPage, JsValue> {
        let r = self.inner.render_page(page as usize, scale).map_err(js_error)?;
        Ok(RenderedPage { width: r.width, height: r.height, rgba: r.rgba })
    }

    /// The page's text in drawing order, lines separated by `\n`.
    #[wasm_bindgen(js_name = pageText)]
    pub fn page_text(&self, page: u32) -> Result<String, JsValue> {
        self.inner.page_text(page as usize).map_err(js_error)
    }

    /// The page's text as lines with boxes, for a selectable text layer over the rendered page.
    #[wasm_bindgen(js_name = pageTextLayer)]
    pub fn page_text_layer(&self, page: u32) -> Result<TextLayer, JsValue> {
        let lines = self.inner.page_text_layer(page as usize).map_err(js_error)?;
        let mut boxes = Vec::with_capacity(lines.len() * 5);
        for l in &lines {
            boxes.extend_from_slice(&l.rect);
            boxes.push(l.angle);
        }
        Ok(TextLayer { text: lines.into_iter().map(|l| l.text).collect(), boxes })
    }

    /// Find `query` on a page. Returns 5 numbers per box: match index, x0, y0, x1, y1 (page
    /// points, y down).
    #[wasm_bindgen(js_name = findText)]
    pub fn find_text(&self, page: u32, query: &str, case_sensitive: bool, whole_words: bool) -> Result<Vec<f32>, JsValue> {
        let matches = self.inner.find_text(page as usize, query, case_sensitive, whole_words).map_err(js_error)?;
        let mut out = Vec::new();
        for (i, m) in matches.iter().enumerate() {
            for r in &m.rects {
                out.push(i as f32);
                out.extend_from_slice(r);
            }
        }
        Ok(out)
    }

    /// The document as PDF.
    #[wasm_bindgen(js_name = exportPdf)]
    pub fn export_pdf(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.export_pdf().map_err(js_error)
    }

    /// The document written back as DOCX.
    #[wasm_bindgen(js_name = saveDocx)]
    pub fn save_docx(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.save_docx().map_err(js_error)
    }

    /// Lay the document out again (after `addFont`).
    pub fn relayout(&mut self) {
        self.inner.relayout();
    }

    /// Run an editing command (`text.insert`, `caret.set`, `edit.undo`, …) with its JSON
    /// parameters. Returns JSON: `{"result": <the command's result>, "changed": bool, "rev": n,
    /// "pageCount": n}`. Throws a `DocxError` (`code`: invalid_command).
    pub fn run(&mut self, command: &str, params_json: &str) -> Result<String, JsValue> {
        let r = self.inner.run(command, params_json).map_err(js_error)?;
        Ok(format!("{{\"result\":{},\"changed\":{},\"rev\":{},\"pageCount\":{}}}", r.result, r.changed, r.rev, r.page_count))
    }

    /// The caret as 4 numbers: page, x, top, height (page points); empty when it is not on a
    /// laid-out line.
    pub fn caret(&self) -> Vec<f32> {
        self.inner.caret().map(|c| vec![c.page as f32, c.x, c.top, c.height]).unwrap_or_default()
    }

    /// The selection as 5 numbers per box: page, x0, y0, x1, y1 (empty for a plain caret).
    #[wasm_bindgen(js_name = selectionRects)]
    pub fn selection_rects(&self) -> Vec<f32> {
        let mut out = Vec::new();
        for (page, r) in self.inner.selection_rects() {
            out.push(page as f32);
            out.extend_from_slice(&r);
        }
        out
    }

    /// The text the last `edit.copy` or `edit.cut` put on the clipboard.
    #[wasm_bindgen(js_name = clipboardText)]
    pub fn clipboard_text(&self) -> String {
        self.inner.clipboard_text().to_owned()
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
}

/// A page size in points.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct PageSize {
    #[wasm_bindgen(readonly, js_name = widthPt)]
    pub width_pt: f32,
    #[wasm_bindgen(readonly, js_name = heightPt)]
    pub height_pt: f32,
}

/// One rendered page.
#[wasm_bindgen]
pub struct RenderedPage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[wasm_bindgen]
impl RenderedPage {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Straight-alpha RGBA8, row-major: `new ImageData(page.rgba(), page.width, page.height)`.
    pub fn rgba(&self) -> Clamped<Vec<u8>> {
        Clamped(self.rgba.clone())
    }
}

/// A page's text layer: `lineCount` lines, each with its text and 5 numbers in `boxes()`:
/// x0, y0, x1, y1 (page points, origin top-left, y down) and the writing angle in radians.
#[wasm_bindgen]
pub struct TextLayer {
    text: Vec<String>,
    boxes: Vec<f32>,
}

#[wasm_bindgen]
impl TextLayer {
    #[wasm_bindgen(getter, js_name = lineCount)]
    pub fn line_count(&self) -> u32 {
        self.text.len() as u32
    }

    /// All lines' text, in order.
    pub fn lines(&self) -> Vec<String> {
        self.text.clone()
    }

    /// 5 numbers per line: x0, y0, x1, y1, angle.
    pub fn boxes(&self) -> Vec<f32> {
        self.boxes.clone()
    }
}

/// Make a font available to every document in this instance (TTF, OTF, WOFF, WOFF2). Returns
/// how many faces it added; call `relayout()` on open documents afterwards.
#[wasm_bindgen(js_name = addFont)]
pub fn add_font(bytes: &[u8]) -> u32 {
    crate::add_font(bytes.to_vec()) as u32
}

/// The engine build: this repository's commit and the wordcraft fork's commit.
#[wasm_bindgen(js_name = engineVersion)]
pub fn engine_version() -> String {
    format!(
        "ubu-office-engine {} (docx engine {})",
        option_env!("UBU_ENGINE_COMMIT").unwrap_or("dev"),
        option_env!("UBU_DOCX_UPSTREAM_COMMIT").unwrap_or("dev")
    )
}
