//! Headless DOCX engine for Ubu Office (Phase 3, D1).
//!
//! [`Document`] is the API: open, page sizes, render, page text, PDF export, save, plus
//! [`add_font`] for the Office-metric fonts the viewer fetches on demand (decision D14). It is
//! plain Rust so it runs in native tests; the browser binding over it (`wasm`) adds nothing but
//! type conversion.
//!
//! Everything comes from the upstream wordcraft crates (open/save `engine::io`, layout, render,
//! `wordcraft-pdf`), unmodified. The view is the print layout without markup: what a reader of the
//! finished document sees, the same as the PDF export.

#[cfg(target_arch = "wasm32")]
mod wasm;

use std::fmt;
use std::sync::Arc;

use wordcraft_engine::Session;
use wordcraft_engine::layout::DocLayout;
use wordcraft_engine::layout::display::{DisplayOptions, Draw, page_display};
use wordcraft_engine::render::{RenderOptions, render_page};

/// Pixel budget for one raster, as in the PDF engine: the browser's wasm heap is 32-bit and
/// shared with the document.
pub const MAX_PIXELS: f32 = 16.0e6;

/// Why an operation failed. The string form (`code()`) is part of the TypeScript contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// Not a document the engine reads, or too damaged to read.
    Invalid,
    /// The document is encrypted and no password was given.
    NeedsPassword,
    /// The password given is wrong.
    WrongPassword,
    /// A page index past the end, or a scale that is not a positive number.
    OutOfRange,
    /// The document could not be written as PDF.
    ExportFailed,
    /// The document could not be written back as DOCX.
    SaveFailed,
}

impl ErrorCode {
    pub fn code(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::NeedsPassword => "needs_password",
            Self::WrongPassword => "wrong_password",
            Self::OutOfRange => "out_of_range",
            Self::ExportFailed => "export_failed",
            Self::SaveFailed => "save_failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocxError {
    pub code: ErrorCode,
    pub message: String,
}

impl DocxError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl fmt::Display for DocxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.code(), self.message)
    }
}

impl std::error::Error for DocxError {}

/// Page size in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSize {
    pub width_pt: f32,
    pub height_pt: f32,
}

/// One rendered page: straight (not premultiplied) RGBA8, row-major, ready for canvas `ImageData`.
#[derive(Clone, Debug)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// An open document, laid out for reading.
pub struct Document {
    session: Session,
    layout: Arc<DocLayout>,
    render: RenderOptions,
}

impl Document {
    /// Open a document from its bytes. `name` gives the format by extension (`.docx`, and the
    /// other formats wordcraft reads); a misnamed file is recognised by its content. Fails with
    /// `NeedsPassword`/`WrongPassword` for password-protected Word files.
    pub fn open(bytes: &[u8], name: &str, password: Option<&str>) -> Result<Self, DocxError> {
        let password = password.filter(|p| !p.is_empty());
        let (doc, _encrypted) = wordcraft_engine::io::open_bytes_with(name, bytes, password).map_err(|e| {
            let code = if wordcraft_engine::io::needs_password(&e) {
                ErrorCode::NeedsPassword
            } else if wordcraft_engine::io::wrong_password(&e) {
                ErrorCode::WrongPassword
            } else {
                ErrorCode::Invalid
            };
            DocxError::new(code, e)
        })?;
        let session = Session::new(doc);
        let layout = session.export_layout();
        // Print view without markup: headers and footers at full strength, tracked changes as
        // the final text (Word's "No Markup").
        let render =
            RenderOptions { display: DisplayOptions { dim_header: false, markup: false, ..DisplayOptions::default() }, ..RenderOptions::default() };
        Ok(Self { session, layout, render })
    }

    pub fn page_count(&self) -> usize {
        self.layout.pages.len()
    }

    pub fn page_size(&self, index: usize) -> Result<PageSize, DocxError> {
        let page = self.layout.pages.get(index).ok_or_else(|| out_of_range(index, self.page_count()))?;
        Ok(PageSize { width_pt: page.w, height_pt: page.h })
    }

    /// The scale a render at `scale` actually uses once the pixel cap applies.
    pub fn effective_scale(&self, index: usize, scale: f32) -> Result<f32, DocxError> {
        let size = self.page_size(index)?;
        if !(scale.is_finite() && scale > 0.0) {
            return Err(DocxError::new(ErrorCode::OutOfRange, format!("scale must be a positive number, got {scale}")));
        }
        let (w, h) = (size.width_pt.max(1.0), size.height_pt.max(1.0));
        Ok(scale.min((MAX_PIXELS / (w * h)).sqrt()))
    }

    /// Render page `index` at `scale` device pixels per point (1.0 = 72 dpi).
    pub fn render_page(&self, index: usize, scale: f32) -> Result<Raster, DocxError> {
        let scale = self.effective_scale(index, scale)?;
        let r = render_page(&self.session.doc, &self.layout.pages[index], scale, &self.render);
        Ok(Raster { width: r.width, height: r.height, rgba: r.to_straight() })
    }

    /// The text of page `index` in drawing order, a line break where the baseline moves down.
    pub fn page_text(&self, index: usize) -> Result<String, DocxError> {
        let page = self.layout.pages.get(index).ok_or_else(|| out_of_range(index, self.page_count()))?;
        let mut out = String::new();
        let mut last_baseline: Option<f32> = None;
        for draw in page_display(&self.session.doc, page, &self.render.display) {
            let Draw::Glyphs { glyphs, text, size, .. } = draw else { continue };
            let Some(&(_, _, baseline)) = glyphs.first() else { continue };
            match last_baseline {
                Some(prev) if (baseline - prev).abs() > size * 0.5 => out.push('\n'),
                Some(_) if !out.ends_with(' ') && !text.starts_with(' ') => out.push(' '),
                _ => {}
            }
            out.push_str(&text);
            last_baseline = Some(baseline);
        }
        Ok(out)
    }

    /// The document as PDF (the same print layout, without markup).
    pub fn export_pdf(&self) -> Result<Vec<u8>, DocxError> {
        wordcraft_pdf::export(&self.session.doc, &wordcraft_pdf::PdfOptions::default())
            .map_err(|e| DocxError::new(ErrorCode::ExportFailed, e.to_string()))
    }

    /// The document written back as DOCX (wordcraft's writer: content it does not model, such
    /// as charts and SmartArt, is not written; decision D15 checks for that before a download).
    pub fn save_docx(&self) -> Result<Vec<u8>, DocxError> {
        wordcraft_engine::io::save_bytes("document.docx", &self.session.doc).map_err(|e| DocxError::new(ErrorCode::SaveFailed, e))
    }

    /// Lay the document out again (after [`add_font`]: text set in a newly available face moves).
    pub fn relayout(&mut self) {
        self.session.relayout();
        self.layout = self.session.export_layout();
    }
}

/// Make a font available to every document in this instance (TTF/OTF; WOFF/WOFF2 are decoded).
/// Returns how many faces it added. Call [`Document::relayout`] afterwards.
pub fn add_font(bytes: Vec<u8>) -> usize {
    match wordcraft_fonts::decode_font_file(&bytes) {
        Ok(data) => wordcraft_fonts::FontDb::global().add_font(data),
        Err(_) => 0,
    }
}

fn out_of_range(index: usize, count: usize) -> DocxError {
    DocxError::new(ErrorCode::OutOfRange, format!("page {index} does not exist; the document has {count} pages"))
}
