//! Headless PDF engine for Ubu Office.
//!
//! [`Document`] is the whole API: open, page sizes, render, text, save, password protection, and
//! editing ([`Edit`]: page operations and comments, saved incrementally). It is plain Rust so it
//! runs in native tests and the round-trip corpus run; the browser binding over it (`wasm`)
//! adds nothing but type conversion.
//!
//! Rendering and text come from the upstream renderer (`pdfcraft-render`); saving comes from the
//! upstream object layer (`pdfcraft-cos`). Neither is modified here.

mod edit;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use edit::Edit;

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;

use pdfcraft_render::PageText;
use pdfcraft_render::text::TextFilter;
use pdfcraft_render::{OpenError, PageInfo, PageRenderer, RenderConfig, RenderRequest, RequestKind, effective_scale};

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
    /// A password given to protect a document breaks the rules (empty open password, or a
    /// permissions password equal to the open password when restrictions are set).
    InvalidPassword,
    /// The document restricts what its open password allows; removing or changing its
    /// protection needs the owner (permissions) password (Ubu Office decision D11).
    OwnerPasswordRequired,
    /// An edit was refused: a page operation or comment the document cannot take (deleting
    /// every page, a page that does not exist, a comment after a page operation in the same edit).
    EditFailed,
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
            Self::InvalidPassword => "invalid_password",
            Self::OwnerPasswordRequired => "owner_password_required",
            Self::EditFailed => "edit_failed",
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

/// What anyone who opens a protected PDF with its open password may do. Viewers honour these
/// flags; nothing can enforce them (§7.6.4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allow {
    /// Print, at any quality.
    pub print: bool,
    /// Copy or extract text and images (extraction for accessibility is always allowed).
    pub copy: bool,
    /// Change the document: edit, annotate, fill forms, insert, rotate or delete pages.
    pub modify: bool,
}

impl Allow {
    pub const ALL: Self = Self { print: true, copy: true, modify: true };

    /// The `/P` bits (Table 22): every bit set, then the ones these flags deny cleared. The
    /// reserved bits are normalised by the encryption layer.
    pub fn p_bits(self) -> i32 {
        let mut p: i32 = -1;
        let clear = |p: &mut i32, bit: u32| *p &= !(1 << (bit - 1));
        if !self.print {
            clear(&mut p, 3);
            clear(&mut p, 12);
        }
        if !self.copy {
            clear(&mut p, 5);
        }
        if !self.modify {
            for bit in [4, 6, 9, 11] {
                clear(&mut p, bit);
            }
        }
        p
    }
}

/// How the open document is protected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Protection {
    /// The file has an `/Encrypt` dictionary (it needed, or accepted, a password to open).
    pub encrypted: bool,
    /// It was opened with its owner (permissions) password, or it is not encrypted.
    pub opened_as_owner: bool,
    /// The open password alone is denied something (printing, copying, changing...).
    pub restricted: bool,
    /// What the password it was opened with allows.
    pub allow: Allow,
}

impl Protection {
    /// Whether its protection may be removed or changed (decision D11): only with the owner
    /// password, or when the open password is not denied anything.
    pub fn can_change(&self) -> bool {
        self.opened_as_owner || !self.restricted
    }
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
    /// Each page's crop box and rotation, for placing comments given in view space.
    geometry: Vec<PageInfo>,
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
        Ok(Self { bytes, password, pages, geometry: info.pages, renderer, text_cache: VecDeque::new() })
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
        write(&self.object_layer()?)
    }

    /// How the document is protected, read by the object layer (which also does the writing).
    pub fn protection(&self) -> Result<Protection, PdfError> {
        Ok(protection_of(&self.object_layer()?))
    }

    /// A copy protected with AES-256 (PDF 2.0, V5/R6): `open_password` is needed to open it, and
    /// `allow` says what that password may do.
    ///
    /// The owner (permissions) password (decision D12) is `permissions_password` when given;
    /// otherwise, when `allow` restricts anything, a random one nobody is shown, so the
    /// restrictions cannot be lifted; with no restrictions it is the open password itself.
    /// `random` is 64 bytes of entropy (32 for the keys and salts, 32 for a random owner
    /// password), supplied by the caller so this crate needs no OS access and tests are
    /// deterministic.
    ///
    /// A document that is already protected and restricted needs its owner password to be
    /// re-protected (D11), as re-protecting would otherwise drop its restrictions.
    pub fn save_protected(
        &self,
        open_password: &str,
        permissions_password: Option<&str>,
        allow: Allow,
        random: &[u8; 64],
    ) -> Result<Vec<u8>, PdfError> {
        if open_password.is_empty() {
            return Err(PdfError::new(ErrorCode::InvalidPassword, "the open password is empty"));
        }
        let restricted = allow != Allow::ALL;
        let owner = match permissions_password.filter(|p| !p.is_empty()) {
            Some(p) if p == open_password => {
                return Err(PdfError::new(ErrorCode::InvalidPassword, "the permissions password must differ from the open password"));
            }
            Some(p) => p.to_owned(),
            None if restricted => hex(&random[32..]),
            None => open_password.to_owned(),
        };
        let mut doc = self.object_layer()?;
        require_change_allowed(&doc)?;
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&random[..32]);
        doc.set_encryption(&pdfcraft_cos::NewEncryption {
            algorithm: pdfcraft_cos::Algorithm::Aes256,
            user_password: open_password,
            owner_password: &owner,
            permissions: allow.p_bits(),
            encrypt_metadata: true,
            seed,
        })
        .map_err(|e| PdfError::new(ErrorCode::SaveFailed, e.to_string()))?;
        write(&doc)
    }

    /// A copy with the protection removed. Needs the owner password when the open password is
    /// restricted (D11); fails with `OwnerPasswordRequired` otherwise.
    pub fn save_unprotected(&self) -> Result<Vec<u8>, PdfError> {
        let mut doc = self.object_layer()?;
        require_change_allowed(&doc)?;
        doc.remove_encryption();
        write(&doc)
    }

    /// Start editing: page operations and comments, saved as an incremental update
    /// ([`Edit::save`]). `date` is the PDF date (`D:YYYYMMDDHHmmSSZ`) stamped on new comments and
    /// `/ModDate`; `id_prefix` starts each new comment's unique name. Refused with
    /// `OwnerPasswordRequired` when the password the file was opened with may not change it.
    pub fn edit(&self, date: Option<&str>, id_prefix: &str) -> Result<Edit, PdfError> {
        let doc = self.object_layer()?;
        let p = protection_of(&doc);
        if !p.allow.modify && !p.opened_as_owner {
            return Err(PdfError::new(
                ErrorCode::OwnerPasswordRequired,
                "this document's open password may not change it; open it with its owner (permissions) password to edit it",
            ));
        }
        Ok(Edit::new(doc, self.geometry.clone(), date, id_prefix))
    }

    /// The upstream object layer over the same bytes and password. Opened on demand rather than
    /// in `open`, so a file the renderer can show but the object layer cannot parse still opens
    /// for viewing; only writing it fails.
    fn object_layer(&self) -> Result<pdfcraft_cos::Document, PdfError> {
        pdfcraft_cos::Document::open_with_password(self.bytes.clone(), self.password.as_deref())
            .map_err(|e| PdfError::new(ErrorCode::SaveFailed, e.to_string()))
    }
}

/// A full, garbage-collected rewrite with the document's output security.
fn write(doc: &pdfcraft_cos::Document) -> Result<Vec<u8>, PdfError> {
    pdfcraft_cos::write_full(doc, &pdfcraft_cos::SaveOptions::default()).map_err(|e| PdfError::new(ErrorCode::SaveFailed, e.to_string()))
}

fn protection_of(doc: &pdfcraft_cos::Document) -> Protection {
    match doc.permissions() {
        None => Protection { encrypted: false, opened_as_owner: true, restricted: false, allow: Allow::ALL },
        Some(p) => {
            // What the open password alone allows: the same bits without the owner override.
            let open_only = pdfcraft_cos::Permissions { bits: p.bits, owner: false };
            let restricted = !(open_only.print()
                && open_only.print_high_quality()
                && open_only.copy()
                && open_only.modify()
                && open_only.annotate()
                && open_only.fill_forms()
                && open_only.assemble());
            Protection {
                encrypted: true,
                opened_as_owner: p.owner,
                restricted,
                allow: Allow { print: p.print(), copy: p.copy(), modify: p.modify() },
            }
        }
    }
}

fn require_change_allowed(doc: &pdfcraft_cos::Document) -> Result<(), PdfError> {
    if protection_of(doc).can_change() {
        Ok(())
    } else {
        Err(PdfError::new(
            ErrorCode::OwnerPasswordRequired,
            "this document restricts what its open password allows; open it with its owner (permissions) password to change its protection",
        ))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
