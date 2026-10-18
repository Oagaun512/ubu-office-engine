//! Headless PDF engine for Ubu Office.
//!
//! [`Document`] is the whole API: open, page sizes, render, text, save. It is plain Rust so it
//! runs in native tests and the round-trip corpus run; the browser binding over it (`wasm`)
//! adds nothing but type conversion.
//!
//! Rendering and text come from the upstream renderer (`pdfcraft-render`); saving comes from the
//! upstream object layer (`pdfcraft-cos`). Neither is modified here.

#[cfg(target_arch = "wasm32")]
mod wasm;

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;

use pdfcraft_render::PageText;
use pdfcraft_render::text::TextFilter;
use pdfcraft_render::{OpenError, PageRenderer, RenderConfig, RenderRequest, RequestKind, effective_scale};

/// Pixel budget for one raster. The upstream cap (64 MP, 256 MB of RGBA) is sized for desktop
/// memory; a browser's wasm heap is 32-bit and shared with the document, so we stop at 16 MP.
pub const MAX_PIXELS: f32 = 16.0e6;

/// Why an operation failed. The string form (`code()`) is part of the TypeScript contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// Not a PDF, or too damaged to read.
    Invalid,
    /// The document is encrypted and no password was given.
    NeedsPassword,
    /// The password given is wrong.
    WrongPassword,
    /// The file uses something the engine cannot handle (e.g. an unknown encryption method).
    Unsupported,
    /// A page index past the end, or a scale that is not a positive number.
    OutOfRange,
    /// The page could not be rendered or its text could not be read.
    RenderFailed,
    /// The document could not be written back.
    SaveFailed,
}

impl ErrorCode {
    pub fn code(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::NeedsPassword => "needs_password",
            Self::WrongPassword => "wrong_password",
            Self::Unsupported => "unsupported",
            Self::OutOfRange => "out_of_range",
            Self::RenderFailed => "render_failed",
            Self::SaveFailed => "save_failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PdfError {
    pub code: ErrorCode,
    pub message: String,
}

impl PdfError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.code(), self.message)
    }
}

impl std::error::Error for PdfError {}

impl From<OpenError> for PdfError {
    fn from(e: OpenError) -> Self {
        let code = match e {
            OpenError::Invalid(_) => ErrorCode::Invalid,
            OpenError::NeedsPassword => ErrorCode::NeedsPassword,
            OpenError::WrongPassword => ErrorCode::WrongPassword,
            OpenError::Unsupported(_) => ErrorCode::Unsupported,
        };
        Self::new(code, e.to_string())
    }
}

/// Displayed page size in PDF points, after `/Rotate` and `/UserUnit`.
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
    /// Non-fatal conditions that made the render partial (skipped content and the like).
    pub warnings: Vec<String>,
}

/// One line of a page's text layer. Coordinates are page points in view space: origin top-left,
/// y down, after `/Rotate`, the same space as [`PageSize`], so a viewer multiplies by its zoom.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub text: String,
    /// `[x0, y0, x1, y1]`: the union of the line's glyph boxes.
    pub rect: [f32; 4],
    /// Writing direction in radians, clockwise from the x axis (0 for ordinary horizontal text).
    pub angle: f32,
}

/// One search match: a box per line it covers (a match can wrap onto the next line).
#[derive(Clone, Debug, PartialEq)]
pub struct TextMatch {
    pub rects: Vec<[f32; 4]>,
}

/// At most this many matches are returned per page and query.
pub const MAX_MATCHES_PER_PAGE: usize = 1000;

/// Extracted page text kept for reuse (text layer, search, plain text), most recent last.
const TEXT_CACHE_PAGES: usize = 64;

/// An open PDF.
pub struct Document {
    bytes: Arc<Vec<u8>>,
    password: Option<String>,
    pages: Vec<PageSize>,
    renderer: PageRenderer,
    text_cache: VecDeque<(usize, Arc<PageText>)>,
}

impl Document {
    /// Open a PDF from its bytes. Fails with `NeedsPassword`/`WrongPassword` for encrypted files.
    pub fn open(bytes: Vec<u8>, password: Option<&str>) -> Result<Self, PdfError> {
        let bytes = Arc::new(bytes);
        let password = password.filter(|p| !p.is_empty()).map(str::to_owned);
        // The renderer never fails on open (an unreadable file has zero pages), so the upstream
        // inspection pass is what classifies errors; it also gives the page sizes.
        let info = pdfcraft_render::inspect(bytes.clone(), password.as_deref())?;
        let pages = info.pages.iter().map(|p| PageSize { width_pt: p.width, height_pt: p.height }).collect::<Vec<_>>();
        let config = RenderConfig { password: password.as_deref().map(Arc::from), ..RenderConfig::default() };
        let renderer = PageRenderer::new(bytes.clone(), config);
        if renderer.page_count() != pages.len() {
            return Err(PdfError::new(
                ErrorCode::Invalid,
                format!("the renderer read {} pages where inspection read {}", renderer.page_count(), pages.len()),
            ));
        }
        Ok(Self { bytes, password, pages, renderer, text_cache: VecDeque::new() })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn page_size(&self, index: usize) -> Result<PageSize, PdfError> {
        self.pages.get(index).copied().ok_or_else(|| out_of_range(index, self.pages.len()))
    }

    /// The scale a render at `scale` actually uses once the pixel caps apply.
    pub fn effective_scale(&self, index: usize, scale: f32) -> Result<f32, PdfError> {
        let size = self.page_size(index)?;
        if !(scale.is_finite() && scale > 0.0) {
            return Err(PdfError::new(ErrorCode::OutOfRange, format!("scale must be a positive number, got {scale}")));
        }
        let (w, h) = (size.width_pt.max(1.0), size.height_pt.max(1.0));
        let by_area = (MAX_PIXELS / (w * h)).sqrt();
        Ok(effective_scale(w, h, scale).min(by_area))
    }

    /// Render page `index` at `scale` device pixels per point (1.0 = 72 dpi).
    pub fn render_page(&mut self, index: usize, scale: f32) -> Result<Raster, PdfError> {
        let scale = self.effective_scale(index, scale)?;
        let page = self.renderer.render(RenderRequest { page: index, kind: RequestKind::Pixels, tile: None, scale, tag: 0, preview: false });
        if let Some(e) = page.error {
            return Err(PdfError::new(ErrorCode::RenderFailed, e));
        }
        let mut rgba = page.rgba.to_vec();
        unpremultiply(&mut rgba);
        Ok(Raster { width: page.width, height: page.height, rgba, warnings: page.warnings.iter().map(|w| format!("{w:?}")).collect() })
    }

    /// The text of page `index` in reading order, with line breaks between lines.
    pub fn page_text(&mut self, index: usize) -> Result<String, PdfError> {
        Ok(self.extracted_text(index)?.plain_text())
    }

    /// The page's text as lines with boxes, in reading order: what a viewer lays over the
    /// rendered page so text can be selected and copied.
    pub fn page_text_layer(&mut self, index: usize) -> Result<Vec<TextLine>, PdfError> {
        let text = self.extracted_text(index)?;
        Ok(lines_of(&text)
            .into_iter()
            .map(|glyphs| {
                let mut line = String::new();
                for (n, &g) in glyphs.iter().enumerate() {
                    if n > 0 && text.space_before[g] {
                        line.push(' ');
                    }
                    line.push_str(&text.glyphs[g].text);
                }
                let [dx, dy] = text.glyphs[glyphs[0]].direction;
                TextLine { text: line, rect: union(glyphs.iter().map(|&g| text.glyphs[g].rect)), angle: dy.atan2(dx) }
            })
            .collect())
    }

    /// Find `query` on page `index` with Acrobat's options (case-sensitive, whole words only).
    /// Line breaks count as one space, so a phrase can match across lines. At most
    /// [`MAX_MATCHES_PER_PAGE`] matches; an empty query finds nothing.
    pub fn find_text(&mut self, index: usize, query: &str, case_sensitive: bool, whole_words: bool) -> Result<Vec<TextMatch>, PdfError> {
        let text = self.extracted_text(index)?;
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let found = text.find_opts_limited(query, case_sensitive, whole_words, MAX_MATCHES_PER_PAGE);
        Ok(found
            .into_iter()
            .map(|range| {
                // One box per line the match touches, in line order.
                let mut by_line: Vec<(u32, Vec<[f32; 4]>)> = Vec::new();
                for g in range.filter(|&g| g < text.glyphs.len()) {
                    let line = text.line_of[g];
                    match by_line.iter_mut().find(|(l, _)| *l == line) {
                        Some((_, rects)) => rects.push(text.glyphs[g].rect),
                        None => by_line.push((line, vec![text.glyphs[g].rect])),
                    }
                }
                by_line.sort_by_key(|(l, _)| *l);
                TextMatch { rects: by_line.into_iter().map(|(_, rects)| union(rects.into_iter())).collect() }
            })
            .filter(|m| !m.rects.is_empty())
            .collect())
    }

    /// The page's extracted text, from the cache when it was read recently.
    fn extracted_text(&mut self, index: usize) -> Result<Arc<PageText>, PdfError> {
        self.page_size(index)?;
        if let Some(hit) = self.text_cache.iter().position(|(i, _)| *i == index).and_then(|pos| self.text_cache.remove(pos)) {
            let text = hit.1.clone();
            self.text_cache.push_back(hit);
            return Ok(text);
        }
        let text = Arc::new(self.renderer.filtered_text(index, TextFilter::default()).map_err(|e| PdfError::new(ErrorCode::RenderFailed, e))?);
        if self.text_cache.len() == TEXT_CACHE_PAGES {
            self.text_cache.pop_front();
        }
        self.text_cache.push_back((index, text.clone()));
        Ok(text)
    }

    /// Write the document back as a full, garbage-collected rewrite.
    ///
    /// The object layer is opened here rather than in `open`, so a file the renderer can show
    /// but the object layer cannot parse still opens for viewing; only saving it fails.
    pub fn save(&self) -> Result<Vec<u8>, PdfError> {
        let doc = pdfcraft_cos::Document::open_with_password(self.bytes.clone(), self.password.as_deref())
            .map_err(|e| PdfError::new(ErrorCode::SaveFailed, e.to_string()))?;
        pdfcraft_cos::write_full(&doc, &pdfcraft_cos::SaveOptions::default()).map_err(|e| PdfError::new(ErrorCode::SaveFailed, e.to_string()))
    }
}

/// Glyph indices grouped by line, lines in reading order, glyphs in content order within a line.
fn lines_of(text: &PageText) -> Vec<Vec<usize>> {
    let mut lines: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (g, &line) in text.line_of.iter().enumerate().take(text.glyphs.len()) {
        lines.entry(line).or_default().push(g);
    }
    lines.into_values().collect()
}

/// The smallest `[x0, y0, x1, y1]` box holding every box given.
fn union(rects: impl Iterator<Item = [f32; 4]>) -> [f32; 4] {
    rects.reduce(|[a0, b0, a1, b1], [c0, d0, c1, d1]| [a0.min(c0), b0.min(d0), a1.max(c1), b1.max(d1)]).unwrap_or([0.0; 4])
}

fn out_of_range(index: usize, count: usize) -> PdfError {
    PdfError::new(ErrorCode::OutOfRange, format!("page {index} does not exist; the document has {count} pages"))
}

/// Premultiplied RGBA8 → straight RGBA8, in place. Canvas `ImageData` expects straight alpha.
pub fn unpremultiply(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a == 0 || a == 255 {
            continue;
        }
        for c in &mut px[..3] {
            *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
        }
    }
}
