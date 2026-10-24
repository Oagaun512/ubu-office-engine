//! The browser binding: `wasm-bindgen` classes over [`crate::Document`]. Type conversion only;
//! every behaviour lives in the plain-Rust API so native tests cover it.
//!
//! The generated TypeScript must match `contract/ubu_pdf.d.ts` (checked in CI), so renaming
//! anything here is a contract change for Ubunex.

use wasm_bindgen::Clamped;
use wasm_bindgen::prelude::*;

use crate::{Allow, Document, Edit, ErrorCode, PdfError};

/// A thrown `Error` with `name = "PdfError"` and a machine-readable `code` (see `ErrorCode`).
fn js_error(e: PdfError) -> JsValue {
    let err = js_sys::Error::new(&e.message);
    err.set_name("PdfError");
    // Setting a property on a fresh Error object cannot fail.
    let _ = js_sys::Reflect::set(&err, &JsValue::from_str("code"), &JsValue::from_str(e.code.code()));
    err.into()
}

/// An open PDF document. Call `free()` when done; the bytes live in wasm memory until then.
#[wasm_bindgen]
pub struct PdfDocument {
    inner: Document,
}

#[wasm_bindgen]
impl PdfDocument {
    /// Open a PDF. Throws a `PdfError` (`code`: invalid, needs_password, wrong_password, unsupported).
    pub fn open(bytes: &[u8], password: Option<String>) -> Result<PdfDocument, JsValue> {
        Document::open(bytes.to_vec(), password.as_deref()).map(|inner| PdfDocument { inner }).map_err(js_error)
    }

    #[wasm_bindgen(getter, js_name = pageCount)]
    pub fn page_count(&self) -> u32 {
        self.inner.page_count() as u32
    }

    /// Displayed page size in points (after rotation).
    #[wasm_bindgen(js_name = pageSize)]
    pub fn page_size(&self, page: u32) -> Result<PageSize, JsValue> {
        let s = self.inner.page_size(page as usize).map_err(js_error)?;
        Ok(PageSize { width_pt: s.width_pt, height_pt: s.height_pt })
    }

    /// Render a page at `scale` device pixels per point (1 = 72 dpi). Straight-alpha RGBA.
    #[wasm_bindgen(js_name = renderPage)]
    pub fn render_page(&mut self, page: u32, scale: f32) -> Result<RenderedPage, JsValue> {
        let r = self.inner.render_page(page as usize, scale).map_err(js_error)?;
        Ok(RenderedPage { width: r.width, height: r.height, rgba: r.rgba, warnings: r.warnings })
    }

    /// The page's text in reading order, lines separated by `\n`.
    #[wasm_bindgen(js_name = pageText)]
    pub fn page_text(&mut self, page: u32) -> Result<String, JsValue> {
        self.inner.page_text(page as usize).map_err(js_error)
    }

    /// The page's text as lines with boxes, for a selectable text layer over the rendered page.
    #[wasm_bindgen(js_name = pageTextLayer)]
    pub fn page_text_layer(&mut self, page: u32) -> Result<TextLayer, JsValue> {
        let lines = self.inner.page_text_layer(page as usize).map_err(js_error)?;
        let mut boxes = Vec::with_capacity(lines.len() * 5);
        for l in &lines {
            boxes.extend_from_slice(&l.rect);
            boxes.push(l.angle);
        }
        Ok(TextLayer { text: lines.into_iter().map(|l| l.text).collect(), boxes })
    }

    /// Find `query` on a page. Returns 5 numbers per box: match index, x0, y0, x1, y1 (page
    /// points, y down). A match that wraps onto the next line has one box per line.
    #[wasm_bindgen(js_name = findText)]
    pub fn find_text(&mut self, page: u32, query: &str, case_sensitive: bool, whole_words: bool) -> Result<Vec<f32>, JsValue> {
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

    /// The whole document written back as a new PDF (full rewrite).
    pub fn save(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.save().map_err(js_error)
    }

    /// How the document is protected.
    pub fn protection(&self) -> Result<Protection, JsValue> {
        let p = self.inner.protection().map_err(js_error)?;
        Ok(Protection {
            encrypted: p.encrypted,
            opened_as_owner: p.opened_as_owner,
            restricted: p.restricted,
            can_change: p.can_change(),
            allow_print: p.allow.print,
            allow_copy: p.allow.copy,
            allow_modify: p.allow.modify,
        })
    }

    /// A copy protected with AES-256. `permissionsPassword` empty or absent: a random one when
    /// anything is denied, else the open password. Throws a `PdfError` (`code`:
    /// invalid_password, owner_password_required, save_failed).
    #[wasm_bindgen(js_name = saveProtected)]
    pub fn save_protected(
        &self,
        open_password: &str,
        permissions_password: Option<String>,
        allow_print: bool,
        allow_copy: bool,
        allow_modify: bool,
    ) -> Result<Vec<u8>, JsValue> {
        // Keys, salts and any random owner password come from the browser's CSPRNG.
        let mut random = [0u8; 64];
        getrandom::fill(&mut random).map_err(|e| js_error(PdfError::new(ErrorCode::SaveFailed, format!("no secure random numbers: {e}"))))?;
        let allow = Allow { print: allow_print, copy: allow_copy, modify: allow_modify };
        self.inner.save_protected(open_password, permissions_password.as_deref(), allow, &random).map_err(js_error)
    }

    /// A copy with the protection removed. Throws `owner_password_required` when the document
    /// was opened with an open password that is denied anything.
    #[wasm_bindgen(js_name = saveUnprotected)]
    pub fn save_unprotected(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.save_unprotected().map_err(js_error)
    }

    /// Start editing (page operations and comments). `date` is a PDF date (`D:YYYYMMDDHHmmSSZ`)
    /// for new comments and `/ModDate`; `id_prefix` starts each new comment's unique name. Throws
    /// `owner_password_required` when the open password may not change the file, `save_failed`
    /// when its structure cannot be read.
    pub fn edit(&self, date: &str, id_prefix: &str) -> Result<PdfEdit, JsValue> {
        self.inner.edit(Some(date), id_prefix).map(|inner| PdfEdit { inner }).map_err(js_error)
    }
}

/// Edits to an open document: page operations and comments. Coordinates are page points in view
/// space (origin top-left, y down, after rotation), as the text layer reports them. Comments use
/// the pages as opened: after a page operation in the same edit they throw `edit_failed`.
/// `save()` returns the original bytes plus one incremental update. Call `free()` when done.
#[wasm_bindgen]
pub struct PdfEdit {
    inner: Edit,
}

#[wasm_bindgen]
impl PdfEdit {
    /// Pages in the document as edited so far.
    #[wasm_bindgen(getter, js_name = pageCount)]
    pub fn page_count(&self) -> u32 {
        self.inner.page_count() as u32
    }

    /// Rotate pages by a multiple of 90 degrees, clockwise.
    #[wasm_bindgen(js_name = rotatePages)]
    pub fn rotate_pages(&mut self, pages: &[u32], degrees: i32) -> Result<(), JsValue> {
        self.inner.rotate_pages(pages, i64::from(degrees)).map_err(js_error)
    }

    /// Delete pages (never every page).
    #[wasm_bindgen(js_name = deletePages)]
    pub fn delete_pages(&mut self, pages: &[u32]) -> Result<(), JsValue> {
        self.inner.delete_pages(pages).map_err(js_error)
    }

    /// Move pages, in their relative order, so they start at position `to`.
    #[wasm_bindgen(js_name = movePages)]
    pub fn move_pages(&mut self, pages: &[u32], to: u32) -> Result<(), JsValue> {
        self.inner.move_pages(pages, to).map_err(js_error)
    }

    /// Insert pages of another PDF at `at` (`pages` empty: all of them); its bookmarks go under
    /// one titled `name`. Returns how many pages were inserted.
    #[wasm_bindgen(js_name = insertPages)]
    pub fn insert_pages(&mut self, bytes: &[u8], password: Option<String>, name: &str, pages: &[u32], at: u32) -> Result<u32, JsValue> {
        self.inner.insert_pages(bytes.to_vec(), password.as_deref(), name, pages, at).map(|n| n as u32).map_err(js_error)
    }

    /// Highlight, underline, strikeout or squiggly over rectangles (4 numbers each).
    #[allow(clippy::too_many_arguments)]
    pub fn markup(&mut self, page: u32, kind: &str, rects: &[f32], color: &[f32], opacity: f64, contents: &str, author: &str) -> Result<(), JsValue> {
        self.inner.markup(page, kind, rects, color, opacity, contents, author).map_err(js_error)
    }

    /// A sticky note whose icon's top-left corner is at (`x`, `y`).
    pub fn note(&mut self, page: u32, x: f32, y: f32, contents: &str, author: &str, color: &[f32]) -> Result<(), JsValue> {
        self.inner.note(page, x, y, contents, author, color).map_err(js_error)
    }

    /// A text box showing `contents` in `rect` (x0, y0, x1, y1).
    #[wasm_bindgen(js_name = textBox)]
    pub fn text_box(&mut self, page: u32, rect: &[f32], font_size: f64, contents: &str, author: &str, color: &[f32]) -> Result<(), JsValue> {
        self.inner.text_box(page, rect, font_size, contents, author, color).map_err(js_error)
    }

    /// Freehand ink: x, y pairs in `points`, the number of points of each stroke in `counts`.
    #[allow(clippy::too_many_arguments)]
    pub fn ink(&mut self, page: u32, points: &[f32], counts: &[u32], width: f64, color: &[f32], opacity: f64, author: &str) -> Result<(), JsValue> {
        self.inner.ink(page, points, counts, width, color, opacity, author).map_err(js_error)
    }

    /// Set (or clear, with "") a document-information entry such as `Producer`.
    #[wasm_bindgen(js_name = setInfo)]
    pub fn set_info(&mut self, key: &str, value: &str) -> Result<(), JsValue> {
        self.inner.set_info(key, value).map_err(js_error)
    }

    /// The original bytes followed by one incremental update with every edit.
    pub fn save(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.save().map_err(js_error)
    }

    /// A new PDF of these pages of the document as edited so far.
    pub fn extract(&self, pages: &[u32]) -> Result<Vec<u8>, JsValue> {
        self.inner.extract(pages).map_err(js_error)
    }
}

/// How a document is protected. `canChange`: its protection may be removed or changed (opened
/// with the owner password, or the open password is denied nothing).
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct Protection {
    #[wasm_bindgen(readonly)]
    pub encrypted: bool,
    #[wasm_bindgen(readonly, js_name = openedAsOwner)]
    pub opened_as_owner: bool,
    #[wasm_bindgen(readonly)]
    pub restricted: bool,
    #[wasm_bindgen(readonly, js_name = canChange)]
    pub can_change: bool,
    #[wasm_bindgen(readonly, js_name = allowPrint)]
    pub allow_print: bool,
    #[wasm_bindgen(readonly, js_name = allowCopy)]
    pub allow_copy: bool,
    #[wasm_bindgen(readonly, js_name = allowModify)]
    pub allow_modify: bool,
}

/// A page size in PDF points.
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
    warnings: Vec<String>,
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

    /// Non-fatal conditions that made the render partial.
    pub fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
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

/// The engine build: this repository's commit and the PDF engine fork's commit.
#[wasm_bindgen(js_name = engineVersion)]
pub fn engine_version() -> String {
    format!(
        "ubu-office-engine {} (pdf engine {})",
        option_env!("UBU_ENGINE_COMMIT").unwrap_or("dev"),
        option_env!("UBU_PDF_UPSTREAM_COMMIT").unwrap_or("dev")
    )
}
