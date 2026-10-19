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
use deckcraft_engine::tools::box_handles;
use deckcraft_engine::{DocState, Mods, PointerEvent, PointerKind, Session};
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

impl fmt::Debug for Deck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Deck").field("slides", &self.doc().slides.len()).field("warnings", &self.warnings).finish()
    }
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
    /// the renderer's limit). `edit` draws it as an editor does (Phase 5 P5): empty placeholders
    /// show their prompts ("Click to add title") and dashed outlines, and the text being edited
    /// shows markers on its empty list paragraphs.
    pub fn render(&self, slide: usize, scale: f64, edit: bool) -> Result<SlideImage> {
        let i = self.slide_index(slide)?;
        if !(scale.is_finite() && scale > 0.0 && scale <= 16.0) {
            return Err(SlidesError::new(ErrorCode::OutOfRange, format!("scale {scale} is outside 0–16 pixels per point")));
        }
        let doc = self.doc();
        let editing = self.state().selection.text.as_ref().filter(|t| edit && !t.notes).map(|t| t.shape);
        let opts = deckcraft_render::RenderOpts { scale: file::image_scale(doc, scale), edit, editing_text: editing, ..Default::default() };
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
        Ok(self.finish(&before, result))
    }

    fn finish(&self, before: &Arc<Presentation>, result: Json) -> EditResult {
        let st = self.state();
        EditResult {
            result: result.to_string(),
            changed: !Arc::ptr_eq(before, &st.doc),
            rev: st.revision,
            slide: st.selection.slide,
            slides: st.doc.slides.len(),
        }
    }

    /// A pointer event on the current slide, in slide points (Phase 5 P5): the engine's own
    /// editor tools decide what it does: a press selects or places the caret, a drag moves,
    /// resizes or rotates the selection or selects text, a double click edits a shape's text or
    /// selects a word. `kind`: `down`, `drag`, `up`, `move`, `double`, `triple`, `context`.
    /// `mods`: 1 Shift, 2 Alt, 4 Ctrl (Cmd on a Mac). `tol`: the hit tolerance in points (about 4
    /// screen pixels at the zoom shown). A finished gesture is one undo step.
    pub fn pointer(&mut self, kind: &str, x: f64, y: f64, mods: u32, tol: f64) -> Result<EditResult> {
        let kind = match kind {
            "down" => PointerKind::Down,
            "drag" => PointerKind::Drag,
            "up" => PointerKind::Up,
            "move" => PointerKind::Move,
            "double" => PointerKind::DoubleClick,
            "triple" => PointerKind::TripleClick,
            "context" => PointerKind::ContextClick,
            other => return Err(SlidesError::new(ErrorCode::InvalidCommand, format!("unknown pointer event `{other}`"))),
        };
        if !(x.is_finite() && y.is_finite() && tol.is_finite()) {
            return Err(SlidesError::new(ErrorCode::InvalidCommand, "pointer coordinates must be numbers"));
        }
        let before = self.doc().clone();
        let ev = PointerEvent { kind, x, y, mods: mods_of(mods), tol: tol.clamp(0.0, 100.0), pressure: None };
        let result = self.session.pointer(ev).map_err(|e| SlidesError::new(ErrorCode::InvalidCommand, e.to_string()))?;
        Ok(self.finish(&before, result))
    }

    /// A key press (`Enter`, `Backspace`, `ArrowLeft`, `Escape`, `Delete`, `Tab`, `Home`, or a
    /// character with Ctrl for a shortcut, such as `z`): text keys while editing text, nudges and
    /// Delete for a selection, the command shortcuts otherwise. The result says whether the
    /// engine handled it (`{"handled": bool, …}`), so the page can leave the rest to the browser.
    pub fn key(&mut self, key: &str, mods: u32) -> Result<EditResult> {
        let before = self.doc().clone();
        let result = self.session.key(key, mods_of(mods)).map_err(|e| SlidesError::new(ErrorCode::InvalidCommand, e.to_string()))?;
        Ok(self.finish(&before, result))
    }

    /// Type text at the caret (or, with a shape selected and no text being edited, start editing
    /// it with this text, as PowerPoint does).
    pub fn type_text(&mut self, text: &str) -> Result<EditResult> {
        let before = self.doc().clone();
        let result = self.session.type_text(text).map_err(|e| SlidesError::new(ErrorCode::InvalidCommand, e.to_string()))?;
        Ok(self.finish(&before, result))
    }

    /// An input method's composition so far (shown underlined in place), with the part being
    /// converted (`active`, characters within `text`), or `commit_text` when it is accepted.
    pub fn compose_text(&mut self, text: &str, active: Option<(usize, usize)>) -> Result<EditResult> {
        let before = self.doc().clone();
        let result = self.session.compose_text(text, active).map_err(|e| SlidesError::new(ErrorCode::InvalidCommand, e.to_string()))?;
        Ok(self.finish(&before, result))
    }

    pub fn commit_text(&mut self, text: &str) -> Result<EditResult> {
        let before = self.doc().clone();
        let result = self.session.commit_text(text).map_err(|e| SlidesError::new(ErrorCode::InvalidCommand, e.to_string()))?;
        Ok(self.finish(&before, result))
    }

    /// What an editor draws over the current slide, in slide points (Phase 5 P5), as JSON:
    /// - `slide`: the current slide;
    /// - `shapes`: each selected shape's `id`, its `corners` (4 points, as drawn, rotation
    ///   included), its resize `handles` (8 points: NW, N, NE, E, SE, S, SW, W) and `rotate`
    ///   handle, `line` (a line's two ends are its only handles: `ends`), `locked`;
    /// - `editing`: the shape whose text is being edited, or null; `caret`: the insertion point
    ///   as a line `[[x, y], [x, y]]` when nothing is selected in the text, with `caretColor`
    ///   (`#rrggbb`, the colour of the text typed there); `textSelection`: the selected text as
    ///   quads (4 points each); `composition`: an input method's underlines `[[x0, y0, x1, y1,
    ///   thick]]`;
    /// - `marquee`: the rubber band being dragged `[x0, y0, x1, y1]`, or null;
    /// - `hover`: what the pointer is over (the engine's hit, for the cursor), or null.
    pub fn overlay(&self) -> String {
        use deckcraft_engine::cmd::text as tx;
        let st = self.state();
        let pt = |p: Point| json!([round2(p.x), round2(p.y)]);
        let editing = st.selection.text.as_ref().filter(|t| !t.notes);
        let shapes: Vec<Json> = st
            .selection
            .shapes
            .iter()
            .filter_map(|id| st.shape(*id))
            .map(|sh| {
                let x = deckcraft_engine::cmd::xfrm_of(&st.doc, &st.selection, sh);
                let a = x.affine();
                let corners: Vec<Json> =
                    [(0.0, 0.0), (x.w, 0.0), (x.w, x.h), (0.0, x.h)].iter().map(|&(px, py)| pt(a * Point::new(px, py))).collect();
                let (handles, rotate) = box_handles(&x);
                let line = sh.is_line();
                json!({
                    "id": sh.id.0,
                    "corners": corners,
                    "handles": if line { Json::Array(vec![]) } else { Json::Array(handles.iter().map(|h| pt(*h)).collect()) },
                    "rotate": if line { Json::Null } else { pt(rotate) },
                    "line": line,
                    "ends": if line { json!([pt(a * Point::new(0.0, 0.0)), pt(a * Point::new(x.w, x.h))]) } else { Json::Null },
                    "locked": sh.locked,
                })
            })
            .collect();
        let caret = editing.filter(|t| !t.is_range()).and_then(|t| tx::caret_line(st, t)).map(|(top, bot)| json!([pt(top), pt(bot)]));
        let caret_color = editing.and_then(|t| tx::caret_color(st, t)).map(|c| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b));
        let text_selection: Vec<Json> = editing
            .filter(|t| t.is_range())
            .and_then(|t| {
                let (l, m) = tx::text_frame(st, t)?;
                let (s0, s1) = t.ordered();
                let pos = |p: (usize, usize)| deckcraft_text::Pos { para: p.0, ch: p.1 };
                Some(
                    l.selection_rects(pos(s0), pos(s1))
                        .into_iter()
                        .map(|r| {
                            Json::Array(
                                [(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)].iter().map(|&(qx, qy)| pt(m * Point::new(qx, qy))).collect(),
                            )
                        })
                        .collect(),
                )
            })
            .unwrap_or_default();
        let composition: Vec<Json> = tx::composition_underlines(st)
            .into_iter()
            .map(|(a, b, thick)| json!([round2(a.x), round2(a.y), round2(b.x), round2(b.y), thick]))
            .collect();
        let tool = &self.session.tool;
        json!({
            "slide": st.selection.slide,
            "shapes": shapes,
            "editing": editing.map(|t| t.shape.0),
            "caret": caret,
            "caretColor": caret_color,
            "textSelection": text_selection,
            "composition": composition,
            "marquee": tool.marquee.map(|r| json!([round2(r.x0), round2(r.y0), round2(r.x1), round2(r.y1)])),
            "hover": tool.hover.as_ref().and_then(|h| serde_json::to_value(h).ok()),
        })
        .to_string()
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

/// Modifier keys from the page's bit set: 1 Shift, 2 Alt, 4 Ctrl or Cmd.
fn mods_of(bits: u32) -> Mods {
    Mods { shift: bits & 1 != 0, alt: bits & 2 != 0, cmd: bits & 4 != 0 }
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
