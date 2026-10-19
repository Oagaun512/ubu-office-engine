//! Headless presentation engine for Ubu Office (Phase 5, P1).
//!
//! [`Deck`] is the API: open a PowerPoint presentation (`.pptx`, `.ppsx`, `.potx`), describe it
//! (slide size, slides, the fonts it uses), render a slide to RGBA pixels at any scale, give the
//! positioned text over a rendered slide (the text layer for selecting, find and screen readers),
//! the slide's shapes with their boxes, its text and speaker notes, run editing commands, and
//! write it as PPTX or export PDF. It is plain Rust so native tests cover it; the browser binding
//! over it (`wasm`) is only type conversion.
//!
//! Everything comes from the upstream deckcraft crates: `deckcraft-engine` (`Session::execute`,
//! undo, the command registry, opening and saving), `deckcraft-render` (slide pixels and the
//! placement of text and shapes), `deckcraft-fonts` (the font database the page adds fonts to).
//! The engine is built without its `media` feature (decision D23, the one patch our fork
//! carries): no audio or video decoder is linked. Video and audio stay in the file and round-trip
//! through PPTX; they show as their poster picture, and [`Deck::warnings`] says they play in
//! PowerPoint (D26).
//!
//! Sizes are points (1/72 inch), as the engine keeps them; [`Deck::render`] takes pixels per
//! point.
//!
//! A panic aborts the wasm instance (the browser build is `panic = "abort"`), so the engine's
//! own catch-and-roll-back inside `execute` does not run there; the viewer's client replays the
//! commands that succeeded on a fresh instance instead, as Ubu Docs and Ubu Sheets do.

#[cfg(target_arch = "wasm32")]
mod wasm;

use std::fmt;
use std::sync::Arc;

use deckcraft_engine::cmd::file;
use deckcraft_engine::{DocState, Session};
use deckcraft_fonts::FontDb;
use deckcraft_model::{Presentation, Shape, ShapeKind};
use kurbo::{Affine, Point};
use serde_json::{Value as Json, json};

/// Why an operation failed. The string form (`code()`) is part of the TypeScript contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// Not a presentation the engine reads, or too damaged to read.
    Invalid,
    /// A presentation the engine cannot open: password-protected, or the old binary `.ppt`.
    Unsupported,
    /// A slide outside the presentation, or a scale out of range.
    OutOfRange,
    /// An editing command was unknown, given bad parameters, not available, or failed.
    InvalidCommand,
    /// The presentation could not be written.
    SaveFailed,
    /// The presentation could not be written as PDF.
    ExportFailed,
}

impl ErrorCode {
    pub fn code(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::Unsupported => "unsupported",
            Self::OutOfRange => "out_of_range",
            Self::InvalidCommand => "invalid_command",
            Self::SaveFailed => "save_failed",
            Self::ExportFailed => "export_failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlidesError {
    pub code: ErrorCode,
    pub message: String,
}

impl SlidesError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl fmt::Display for SlidesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.code(), self.message)
    }
}

impl std::error::Error for SlidesError {}

type Result<T> = std::result::Result<T, SlidesError>;

/// The compound-file signature: the old binary `.ppt` (or an encrypted package).
const CFB_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Commands the viewer may not run: files are opened and saved through [`Deck`], and media can't
/// be inserted without decoders (D26).
const REFUSED_PREFIXES: &[&str] = &["file.", "media."];
const REFUSED: &[&str] = &["insert.audio", "insert.video", "transition.sound"];

/// A rendered slide: straight (unpremultiplied) RGBA, row by row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlideImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// What an editing command did.
#[derive(Clone, Debug, PartialEq)]
pub struct EditResult {
    /// The command's own JSON result.
    pub result: String,
    /// The presentation changed: slides rendered before are stale.
    pub changed: bool,
    /// The engine's revision after the command (every change, selection included, bumps it).
    pub rev: u64,
    /// The current slide after the command.
    pub slide: usize,
    /// How many slides the presentation has after the command.
    pub slides: usize,
}

/// An open presentation in its own engine session.
pub struct Deck {
    session: Session,
    warnings: Vec<String>,
}

impl Deck {
    /// Open a presentation. `name` is used in messages only: the bytes decide the format.
    pub fn open(bytes: &[u8], name: &str) -> Result<Deck> {
        if !deckcraft_pptx::sniff(bytes) {
            if deckcraft_pptx::is_encrypted(bytes) {
                return Err(SlidesError::new(
                    ErrorCode::Unsupported,
                    format!("'{name}' is password-protected, and password-protected presentations can't be opened yet"),
                ));
            }
            if bytes.starts_with(&CFB_MAGIC) {
                return Err(SlidesError::new(
                    ErrorCode::Unsupported,
                    format!("'{name}' is an old PowerPoint 97–2003 presentation (.ppt); save it as .pptx in PowerPoint to open it here"),
                ));
            }
            return Err(SlidesError::new(ErrorCode::Invalid, format!("'{name}' is not a PowerPoint presentation")));
        }
        let (doc, damage) = file::open_checked(name, bytes).map_err(|e| SlidesError::new(ErrorCode::Invalid, e.to_string()))?;
        let mut warnings = Vec::new();
        if !damage.slides.is_empty() {
            let n = damage.slides.len();
            warnings.push(if n == 1 {
                "1 slide couldn't be read and is left out".to_string()
            } else {
                format!("{n} slides couldn't be read and are left out")
            });
        }
        if has_media(&doc) {
            warnings.push("This presentation has video or audio: it shows as a still picture here and plays in PowerPoint".to_string());
        }
        let mut session = Session::new();
        file::add_opened(&mut session, name, None, doc);
        Ok(Deck { session, warnings })
    }

    /// What opening noticed (slides left out, media that won't play here).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    fn state(&self) -> &DocState {
        // A Deck always has its one document: `open` adds it and nothing here closes it.
        self.session.active().expect("a deck has a document")
    }

    fn doc(&self) -> &Arc<Presentation> {
        &self.state().doc
    }

    fn slide_index(&self, slide: usize) -> Result<usize> {
        let n = self.doc().slides.len();
        if slide < n { Ok(slide) } else { Err(SlidesError::new(ErrorCode::OutOfRange, format!("no slide {slide} (the presentation has {n})"))) }
    }

    /// The presentation, as JSON: `{"width", "height"}` (the slide size in points), `"slides"`
    /// (how many), `"fonts"` (the families its text uses), `"missingFonts"` (those of them
    /// neither added nor compiled in; substitutes draw them).
    pub fn info(&self) -> String {
        let doc = self.doc();
        json!({
            "width": doc.slide_size.width,
            "height": doc.slide_size.height,
            "slides": doc.slides.len(),
            "fonts": deckcraft_engine::fonts::used_fonts(doc),
            "missingFonts": deckcraft_engine::fonts::missing_fonts(doc),
        })
        .to_string()
    }

    /// The slides, as JSON: `[{"id", "hidden", "notes": bool}]`.
    pub fn slides(&self) -> String {
        let v: Vec<Json> = self.doc().slides.iter().map(|s| json!({"id": s.id.0, "hidden": s.hidden, "notes": !s.notes.is_empty()})).collect();
        Json::Array(v).to_string()
    }

    /// Add a font file (TTF/OTF/TTC) to the engine's font database; returns how many faces it
    /// added. Fonts the presentation names but doesn't have are drawn with their open
    /// substitutes once those are added (Carlito for Calibri, …; D25).
    pub fn add_font(bytes: Vec<u8>) -> usize {
        FontDb::global().add_font(bytes)
    }

    /// Render slide `slide` at `scale` pixels per point (clamped so the longer side stays within
    /// the renderer's limit).
    pub fn render(&self, slide: usize, scale: f64) -> Result<SlideImage> {
        let i = self.slide_index(slide)?;
        if !(scale.is_finite() && scale > 0.0 && scale <= 16.0) {
            return Err(SlidesError::new(ErrorCode::OutOfRange, format!("scale {scale} is outside 0–16 pixels per point")));
        }
        let doc = self.doc();
        let opts = deckcraft_render::RenderOpts { scale: file::image_scale(doc, scale), ..Default::default() };
        let img = deckcraft_render::render_slide(doc, i, &opts);
        Ok(SlideImage { width: img.width, height: img.height, pixels: img.to_straight() })
    }

    /// The text drawn on a slide, as JSON spans in slide points: `[{"text", "x", "y", "x2",
    /// "y2", "size"}]`, where (x, y) → (x2, y2) is the run's baseline (rotated text has a sloped
    /// one) and `size` the font size. Master and layout text (footers, slide numbers) come first,
    /// as drawn.
    pub fn text_layer(&self, slide: usize) -> Result<String> {
        let i = self.slide_index(slide)?;
        let placed = deckcraft_render::place_slide(self.doc(), i);
        let mut spans = Vec::new();
        for pt in &placed.texts {
            for run in &pt.layout.runs {
                if run.glyphs.is_empty() || run.size <= 0.0 || run.cells.is_empty() {
                    continue;
                }
                let text = run_text(&pt.body, run.para, run.chars);
                if text.trim().is_empty() {
                    continue;
                }
                let first = run.glyphs[0];
                let last = run.glyphs[run.glyphs.len() - 1];
                let advance = run.face.advance(last.0) * run.size / run.face.upem.max(1.0);
                let a = pt.transform * Point::new(first.1, first.2);
                let b = pt.transform * Point::new(last.1 + advance, last.2);
                let size = run.size * scale_of(pt.transform);
                spans.push(json!({"text": text, "x": round2(a.x), "y": round2(a.y), "x2": round2(b.x), "y2": round2(b.y), "size": round2(size)}));
            }
        }
        Ok(Json::Array(spans).to_string())
    }

    /// The slide's own shapes (not its layout's or master's), back to front, as JSON: `[{"id",
    /// "box": [x0, y0, x1, y1], "group": bool, "parent": index | null}]` in slide points.
    pub fn shapes(&self, slide: usize) -> Result<String> {
        let i = self.slide_index(slide)?;
        let placed = deckcraft_render::place_slide(self.doc(), i);
        let v: Vec<Json> = placed
            .shapes
            .iter()
            .map(|s| {
                json!({
                    "id": s.id.0,
                    "box": [round2(s.rect.x0), round2(s.rect.y0), round2(s.rect.x1), round2(s.rect.y1)],
                    "group": s.is_group,
                    "parent": s.parent,
                })
            })
            .collect();
        Ok(Json::Array(v).to_string())
    }

    /// The text on a slide, one text body per line group, in drawing order (for find and the
    /// summary).
    pub fn slide_text(&self, slide: usize) -> Result<String> {
        let i = self.slide_index(slide)?;
        let placed = deckcraft_render::place_slide(self.doc(), i);
        let parts: Vec<String> = placed.texts.iter().map(|t| t.body.text()).filter(|t| !t.trim().is_empty()).collect();
        Ok(parts.join("\n"))
    }

    /// A slide's speaker notes, as plain text.
    pub fn notes(&self, slide: usize) -> Result<String> {
        let i = self.slide_index(slide)?;
        Ok(self.doc().slides[i].notes.text())
    }

    /// Run an engine command (`slide.new`, `text.insert`, `edit.undo`, …) with its JSON
    /// parameters (empty for none).
    pub fn run(&mut self, command: &str, params_json: &str) -> Result<EditResult> {
        if REFUSED.contains(&command) || REFUSED_PREFIXES.iter().any(|p| command.starts_with(p)) {
            return Err(SlidesError::new(ErrorCode::InvalidCommand, format!("`{command}` is not available here")));
        }
        let params: Json = if params_json.trim().is_empty() {
            Json::Null
        } else {
            serde_json::from_str(params_json).map_err(|e| SlidesError::new(ErrorCode::InvalidCommand, format!("bad parameters: {e}")))?
        };
        let before = self.doc().clone();
        let result = self.session.execute(command, &params).map_err(|e| SlidesError::new(ErrorCode::InvalidCommand, e.to_string()))?;
        let st = self.state();
        Ok(EditResult {
            result: result.to_string(),
            changed: !Arc::ptr_eq(&before, &st.doc),
            rev: st.revision,
            slide: st.selection.slide,
            slides: st.doc.slides.len(),
        })
    }

    /// The selection, as the engine keeps it (JSON): the current slide, selected slides and
    /// shapes, the text caret or range, table cells.
    pub fn selection(&self) -> String {
        serde_json::to_string(&self.state().selection).unwrap_or_else(|_| "null".into())
    }

    pub fn is_dirty(&self) -> bool {
        self.state().is_dirty()
    }

    pub fn can_undo(&self) -> bool {
        !self.state().history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.state().history.redo.is_empty()
    }

    /// The presentation as PPTX.
    pub fn save_pptx(&self) -> Result<Vec<u8>> {
        file::save_bytes(self.doc(), "pptx").map_err(|e| SlidesError::new(ErrorCode::SaveFailed, e.to_string()))
    }

    /// The presentation as PDF: one page per slide (hidden ones left out), with a text layer.
    pub fn export_pdf(&self) -> Result<Vec<u8>> {
        file::pdf_bytes(self.doc(), &json!({})).map_err(|e| SlidesError::new(ErrorCode::ExportFailed, e.to_string()))
    }
}

/// The characters `chars` of paragraph `para`.
fn run_text(body: &deckcraft_model::text::TextBody, para: usize, chars: (usize, usize)) -> String {
    body.paragraphs.get(para).map(|p| p.text()).unwrap_or_default().chars().skip(chars.0).take(chars.1.saturating_sub(chars.0)).collect()
}

/// The uniform scale of a placement transform (its determinant's square root).
fn scale_of(t: Affine) -> f64 {
    t.determinant().abs().sqrt()
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Video or audio anywhere on the slides (inside groups too).
fn has_media(p: &Presentation) -> bool {
    fn any(shapes: &[Shape]) -> bool {
        shapes.iter().any(|s| match &s.kind {
            ShapeKind::Media(_) => true,
            ShapeKind::Group { children, .. } => any(children),
            _ => false,
        })
    }
    p.slides.iter().any(|s| any(&s.shapes))
}
