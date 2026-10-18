//! The browser binding: `wasm-bindgen` classes over [`crate::Document`]. Type conversion only;
//! every behaviour lives in the plain-Rust API so native tests cover it.
//!
//! The generated TypeScript must match `contract/ubu_pdf.d.ts` (checked in CI), so renaming
//! anything here is a contract change for Ubunex.

use wasm_bindgen::Clamped;
use wasm_bindgen::prelude::*;

use crate::{Document, PdfError};

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

    /// The whole document written back as a new PDF (full rewrite).
    pub fn save(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.save().map_err(js_error)
    }
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

/// The engine build: this repository's commit and the PDF engine fork's commit.
#[wasm_bindgen(js_name = engineVersion)]
pub fn engine_version() -> String {
    format!(
        "ubu-office-engine {} (pdf engine {})",
        option_env!("UBU_ENGINE_COMMIT").unwrap_or("dev"),
        option_env!("UBU_PDF_UPSTREAM_COMMIT").unwrap_or("dev")
    )
}
