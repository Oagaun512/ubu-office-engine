//! Headless spreadsheet engine for Ubu Office (Phase 4, S1).
//!
//! [`Book`] is the API: open a workbook (XLSX, XLSM, XLTX; XLS, XLSB and ODS read; password-
//! protected files), list its sheets, read a sheet's layout (column widths, row heights, merges,
//! frozen panes, charts) and windows of cells (display text and the effective style, resolved
//! to colours), run editing commands, and write the workbook as XLSX or CSV, or export PDF. It
//! is plain Rust so native tests cover it; the browser binding over it (`wasm`) is only type
//! conversion.
//!
//! Everything comes from the upstream gridcraft crates, unmodified: `engine::io` to open and
//! save, `Session::execute_headless` for commands, `engine::display` for the text a cell shows,
//! `engine::tables` and `engine::cf` for table styles and conditional formats, `gridcraft-chart`
//! for chart layout. Upstream has no headless grid painter (its grid is part of its desktop UI),
//! so the viewer draws the grid itself from what [`Book::window`] returns; the effective-style
//! merge below follows the upstream UI's `look` (ui-egui/src/grid.rs) rule for rule.
//!
//! Sizes are the engine's: CSS pixels at 100% zoom (96 dpi), as the upstream UI draws them.
//!
//! A panic aborts the wasm instance (the browser build is `panic = "abort"`), so the engine's
//! own catch-and-roll-back inside `execute` does not run there; the viewer's client replays the
//! commands that succeeded on a fresh instance instead, as Ubu Docs does.

#[cfg(target_arch = "wasm32")]
mod wasm;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use gridcraft_engine::cf::CfCache;
use gridcraft_engine::core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef, Value};
use gridcraft_engine::model::{
    BorderLine, BorderStyle, Color, HAlign, PatternType, Sheet, Style, StyleId, Theme, Underline, VAlign, Visibility, Workbook,
};
use gridcraft_engine::{DocState, Session, display, io, tables};
use serde::Serialize;
use serde_json::{Value as Json, json};

/// The most cells one [`Book::window`] call reads (a few screens' worth).
pub const MAX_WINDOW_CELLS: u64 = 40_000;

/// Why an operation failed. The string form (`code()`) is part of the TypeScript contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// Not a workbook the engine reads, or too damaged to read.
    Invalid,
    /// The workbook is encrypted and no password was given.
    NeedsPassword,
    /// The password given is wrong.
    WrongPassword,
    /// The file uses an encryption the engine cannot open (RC4, certificates).
    Unsupported,
    /// A sheet, cell or window outside the workbook, or a window too large.
    OutOfRange,
    /// An editing command was unknown, given bad parameters, not available, or failed.
    InvalidCommand,
    /// The workbook could not be written.
    SaveFailed,
    /// The workbook could not be written as PDF.
    ExportFailed,
}

impl ErrorCode {
    pub fn code(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::NeedsPassword => "needs_password",
            Self::WrongPassword => "wrong_password",
            Self::Unsupported => "unsupported",
            Self::OutOfRange => "out_of_range",
            Self::InvalidCommand => "invalid_command",
            Self::SaveFailed => "save_failed",
            Self::ExportFailed => "export_failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetsError {
    pub code: ErrorCode,
    pub message: String,
}

impl SheetsError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl fmt::Display for SheetsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.code(), self.message)
    }
}

impl std::error::Error for SheetsError {}

type Result<T> = std::result::Result<T, SheetsError>;

/// An open workbook in its own engine session.
pub struct Book {
    session: Session,
    warnings: Vec<String>,
}

/// What an editing command did.
#[derive(Clone, Debug, PartialEq)]
pub struct EditResult {
    /// The command's own JSON result.
    pub result: String,
    /// The workbook changed (cells, formats, sheets or view): windows read before are stale.
    pub changed: bool,
    /// The undo revision after the command (undoable edits bump it).
    pub rev: u64,
    /// The active sheet after the command.
    pub sheet: usize,
}

impl Book {
    /// Open a workbook; `name` gives the format by extension when the bytes don't say.
    pub fn open(bytes: &[u8], name: &str, password: Option<&str>) -> Result<Book> {
        let (wb, warnings) = io::open_bytes_with_password(name, bytes, password).map_err(|e| open_error(&e.to_string()))?;
        let mut session = Session::new();
        session.add_document(DocState::new(wb, None, name.to_string()));
        Ok(Book { session, warnings })
    }

    /// What opening noticed (a decrypted workbook, features read but not kept, …).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    fn doc(&self) -> &DocState {
        // open() always adds the one document, and no command here closes it (`file.close` and
        // `file.new` are refused in `run`).
        self.session.active().expect("a document is open")
    }

    fn wb(&self) -> &Arc<Workbook> {
        &self.doc().wb
    }

    fn sheet(&self, si: usize) -> Result<&Sheet> {
        self.wb().sheet(si).ok_or_else(|| SheetsError::new(ErrorCode::OutOfRange, format!("no sheet {si}")))
    }

    /// The sheets in order, as JSON: `[{"name", "visibility": "visible|hidden|veryHidden", "tabColor": "#RRGGBB"|null}]`.
    pub fn sheets(&self) -> String {
        let wb = self.wb();
        let list: Vec<Json> = wb
            .sheets
            .iter()
            .map(|sh| {
                let visibility = match sh.visibility {
                    Visibility::Visible => "visible",
                    Visibility::Hidden => "hidden",
                    Visibility::VeryHidden => "veryHidden",
                };
                json!({"name": sh.name, "visibility": visibility, "tabColor": sh.tab_color.and_then(|c| c.hex(&wb.theme))})
            })
            .collect();
        Json::Array(list).to_string()
    }

    pub fn active_sheet(&self) -> usize {
        self.wb().active_sheet
    }

    /// A sheet's layout, as JSON: default and custom sizes (CSS px at 100%; 0 = hidden), merges,
    /// frozen panes, the used range, view settings and the charts' anchors.
    pub fn layout(&self, si: usize) -> Result<String> {
        let wb = self.wb();
        let sh = self.sheet(si)?;
        let sizes = |lines: &std::collections::BTreeMap<u32, gridcraft_engine::model::LineInfo>, default: f32| -> Vec<Json> {
            lines
                .iter()
                .filter(|(_, i)| i.hidden || i.size.is_some_and(|s| (s - default).abs() > 0.01))
                .map(|(n, i)| json!([n, if i.hidden { 0.0 } else { i.size.unwrap_or(default) }]))
                .collect()
        };
        let range = |r: &RangeRef| json!([r.start.row, r.start.col, r.end.row, r.end.col]);
        let charts: Vec<Json> = sh
            .charts
            .iter()
            .map(|c| {
                let a = c.anchor;
                json!({"row": a.cell.row, "col": a.cell.col, "dx": a.dx, "dy": a.dy, "width": a.width, "height": a.height, "title": c.title})
            })
            .collect();
        let images: Vec<Json> = sh
            .images
            .iter()
            .map(|i| {
                let a = i.anchor;
                json!({"row": a.cell.row, "col": a.cell.col, "dx": a.dx, "dy": a.dy, "width": a.width, "height": a.height, "alt": i.alt})
            })
            .collect();
        Ok(json!({
            "defaultColWidth": sh.default_col_width,
            "defaultRowHeight": sh.default_row_height,
            "cols": sizes(&sh.cols, sh.default_col_width),
            "rows": sizes(&sh.rows, sh.default_row_height),
            "merges": sh.merges.iter().map(range).collect::<Vec<_>>(),
            "freeze": sh.freeze.map(|(r, c)| json!([r, c])),
            "used": sh.used_range().map(|r| range(&r)),
            "gridlines": sh.show_gridlines,
            "headings": sh.show_headings,
            "rightToLeft": sh.right_to_left,
            "zoom": sh.zoom,
            "charts": charts,
            "images": images,
            "hasPivots": !sh.pivots.is_empty(),
            "hasSlicers": !sh.slicers.is_empty(),
            "defaultStyle": look_json(&base_look(wb, wb.styles.get(StyleId::DEFAULT).clone(), None), &wb.theme),
        })
        .to_string())
    }

    /// The cells of a window (inclusive), as JSON `{"styles": [look…], "cells": [[row, col,
    /// text, style, flags, color?, fillChar?, fillAt?]…]}`. Only cells with content or a style
    /// of their own are listed. `digit_px` is the width of a digit in the sheet's default font
    /// (CSS px at 100%): General numbers are shortened to fit their column as Excel does, and
    /// `#` fills a column too narrow for any form of the number. 0 turns that off.
    ///
    /// Flags: 1 numeric (right-aligned by default), 2 formula, 4 checkbox, 8 checked, 16 error.
    pub fn window(&self, si: usize, r0: u32, c0: u32, r1: u32, c1: u32, digit_px: f32) -> Result<String> {
        let wb = self.wb();
        let sh = self.sheet(si)?;
        if r0 > r1 || c0 > c1 || r1 >= MAX_ROWS || c1 >= MAX_COLS {
            return Err(SheetsError::new(ErrorCode::OutOfRange, format!("window {r0},{c0}..{r1},{c1}")));
        }
        let cells = (r1 - r0 + 1) as u64 * (c1 - c0 + 1) as u64;
        if cells > MAX_WINDOW_CELLS {
            return Err(SheetsError::new(ErrorCode::OutOfRange, format!("a window holds at most {MAX_WINDOW_CELLS} cells, not {cells}")));
        }
        let mut cf = CfCache::new();
        let has_cf = !sh.cond_formats.is_empty();
        let mut styles: Vec<Json> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut out: Vec<Json> = Vec::new();
        // Walk the stored cells, not every address: a sheet is mostly empty. A row or column
        // style, a table or a conditional format also gives empty cells a look of their own.
        let styled_lines = sh.rows.range(r0..=r1).any(|(_, i)| i.style.is_some()) || sh.cols.range(c0..=c1).any(|(_, i)| i.style.is_some());
        let window = RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1));
        let dense = styled_lines || has_cf || sh.tables.iter().any(|t| t.range.intersects(&window)) || !sh.spill.is_empty();
        let mut visit = |c: CellRef| {
            let value = sh.value(c);
            let own_style = sh.style_id(c) != StyleId::DEFAULT;
            let in_table = sh.table_at(c).is_some();
            if value.is_empty() && !own_style && !in_table && !has_cf {
                return;
            }
            let (style, cfl) = effective_style(wb, si, sh, c, &mut cf);
            let look = base_look(wb, style.clone(), cfl.as_ref());
            if value.is_empty() && look.is_plain() {
                return;
            }
            let shown = shown_text(wb, sh, c, &value, &style, digit_px, cfl.as_ref().is_some_and(|l| l.hide_value));
            let key = serde_json::to_string(&look).unwrap_or_default();
            let next = styles.len();
            let id = *index.entry(key).or_insert_with(|| {
                styles.push(look_json(&look, &wb.theme));
                next
            });
            let mut flags = 0u32;
            if shown.numeric {
                flags |= 1;
            }
            if sh.cell(c).is_some_and(|x| x.formula.is_some()) {
                flags |= 2;
            }
            if shown.checkbox {
                flags |= 4;
                if matches!(value, Value::Bool(true)) {
                    flags |= 8;
                }
            }
            if value.is_error() {
                flags |= 16;
            }
            let mut row = vec![json!(c.row), json!(c.col), json!(shown.text), json!(id), json!(flags)];
            if shown.color.is_some() || shown.fill.is_some() {
                row.push(json!(shown.color));
            }
            if let Some((ch, at)) = shown.fill {
                row.push(json!(ch.to_string()));
                row.push(json!(at));
            }
            out.push(Json::Array(row));
        };
        if dense {
            for r in r0..=r1 {
                for c in c0..=c1 {
                    visit(CellRef::new(r, c));
                }
            }
        } else {
            let mut stored: Vec<CellRef> = sh.cells.iter_range(window).map(|(c, _)| c).filter(|c| window.contains(*c)).collect();
            stored.sort_unstable_by_key(|c| (c.row, c.col));
            for c in stored {
                visit(c);
            }
        }
        Ok(json!({"styles": styles, "cells": out}).to_string())
    }

    /// What the formula bar shows for a cell: its formula (`=…`) or the value as typed.
    pub fn cell_input(&self, si: usize, row: u32, col: u32) -> Result<String> {
        let wb = self.wb();
        let sh = self.sheet(si)?;
        if row >= MAX_ROWS || col >= MAX_COLS {
            return Err(SheetsError::new(ErrorCode::OutOfRange, format!("cell {row},{col}")));
        }
        let c = CellRef::new(row, col);
        if display::formula_hidden(wb, sh, c) {
            return Ok(String::new());
        }
        Ok(display::edit_text(wb, sh, c))
    }

    /// A chart laid out by the engine at `width` × `height` CSS px, as JSON drawing primitives
    /// (rectangles, lines, polygons, wedges, circles, text) for the viewer to paint. Text
    /// widths are approximate (the engine's own fixed-width measure).
    pub fn chart(&self, si: usize, index: usize, width: f32, height: f32) -> Result<String> {
        let wb = self.wb();
        let sh = self.sheet(si)?;
        let chart = sh.charts.get(index).ok_or_else(|| SheetsError::new(ErrorCode::OutOfRange, format!("no chart {index}")))?;
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 && width * height <= 16.0e6) {
            return Err(SheetsError::new(ErrorCode::OutOfRange, format!("chart size {width}x{height}")));
        }
        let data = gridcraft_chart::resolve(wb, si, chart);
        let prims = gridcraft_chart::render(chart, &data, width, height, &gridcraft_chart::ApproxMeasure);
        serde_json::to_string(&prims).map_err(|e| SheetsError::new(ErrorCode::Invalid, e.to_string()))
    }

    /// Run one of the engine's commands (`cell.set`, `selection.set`, `home.bold`, `edit.undo`,
    /// …) with its JSON parameters. Commands that would open a file, a dialog or another
    /// workbook are refused: the viewer owns files.
    pub fn run(&mut self, command: &str, params_json: &str) -> Result<EditResult> {
        const REFUSED: &[&str] = &["file.open", "file.new", "file.close", "file.save", "file.saveAs", "file.exportCsv", "file.exportHtml"];
        if REFUSED.contains(&command) {
            return Err(SheetsError::new(ErrorCode::InvalidCommand, format!("`{command}` is not available here")));
        }
        let params: Json = if params_json.trim().is_empty() {
            Json::Object(Default::default())
        } else {
            serde_json::from_str(params_json).map_err(|e| SheetsError::new(ErrorCode::InvalidCommand, format!("parameters are not JSON: {e}")))?
        };
        let before = self.wb().clone();
        let result = self.session.execute_headless(command, params).map_err(|e| SheetsError::new(ErrorCode::InvalidCommand, e.to_string()))?;
        let d = self.doc();
        Ok(EditResult { result: result.to_string(), changed: !Arc::ptr_eq(&before, &d.wb), rev: d.revision, sheet: d.wb.active_sheet })
    }

    /// The selection, as JSON: `{"sheet", "active": [row, col], "ranges": [[r0, c0, r1, c1]…]}`.
    pub fn selection(&self) -> String {
        let d = self.doc();
        let s = &d.selection;
        let ranges: Vec<Json> = s.ranges.iter().map(|r| json!([r.start.row, r.start.col, r.end.row, r.end.col])).collect();
        json!({"sheet": d.wb.active_sheet, "active": [s.active.row, s.active.col], "ranges": ranges}).to_string()
    }

    pub fn is_dirty(&self) -> bool {
        self.doc().is_dirty()
    }

    pub fn can_undo(&self) -> bool {
        !self.doc().undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.doc().redo.is_empty()
    }

    pub fn revision(&self) -> u64 {
        self.doc().revision
    }

    /// The workbook as XLSX (decision D22: a copy is always .xlsx, whatever was opened).
    pub fn save_xlsx(&self) -> Result<Vec<u8>> {
        io::save_bytes(self.wb(), "copy.xlsx").map_err(|e| SheetsError::new(ErrorCode::SaveFailed, e.to_string()))
    }

    /// One sheet as CSV (values as shown, comma separated, UTF-8).
    pub fn save_csv(&self, si: usize) -> Result<Vec<u8>> {
        self.sheet(si)?;
        let mut wb = (**self.wb()).clone();
        wb.active_sheet = si;
        io::save_bytes_as(&wb, "copy.csv", io::FileKind::Csv).map_err(|e| SheetsError::new(ErrorCode::SaveFailed, e.to_string()))
    }

    /// The first character on a sheet that the PDF export cannot write (its fonts are the PDF
    /// standard fonts with Windows-1252 text), or None when the whole sheet can be exported
    /// (decision D20: the viewer refuses rather than writing a PDF with missing text).
    pub fn pdf_unwritable(&self, si: usize) -> Result<Option<char>> {
        let wb = self.wb();
        let sh = self.sheet(si)?;
        for (c, _) in sh.cells.iter() {
            let text = display::cell_text(wb, sh, c);
            if let Some(ch) = text.chars().find(|ch| !win_ansi(*ch)) {
                return Ok(Some(ch));
            }
        }
        for chart in &sh.charts {
            for t in chart.title.iter().chain(chart.x_title.iter()).chain(chart.y_title.iter()) {
                if let Some(ch) = t.chars().find(|ch| !win_ansi(*ch)) {
                    return Ok(Some(ch));
                }
            }
        }
        Ok(None)
    }

    /// The active sheet as PDF (the engine's print layout).
    pub fn export_pdf(&mut self) -> Result<Vec<u8>> {
        let out = self.session.execute_headless("file.exportPdf", json!({})).map_err(|e| SheetsError::new(ErrorCode::ExportFailed, e.to_string()))?;
        let b64 = out.get("base64").and_then(Json::as_str).ok_or_else(|| SheetsError::new(ErrorCode::ExportFailed, "no PDF in the result"))?;
        io::base64_decode(b64).ok_or_else(|| SheetsError::new(ErrorCode::ExportFailed, "the PDF could not be decoded"))
    }
}

/// The engine's open error as a code: its message text is the only signal it gives.
fn open_error(message: &str) -> SheetsError {
    let code = if message.contains(io::PASSWORD_REQUIRED) {
        ErrorCode::NeedsPassword
    } else if message.contains(io::WRONG_PASSWORD) {
        ErrorCode::WrongPassword
    } else if message.contains("encryption, which") {
        ErrorCode::Unsupported
    } else {
        ErrorCode::Invalid
    };
    SheetsError::new(code, message)
}

/// Whether Windows-1252 has the character (what the PDF export's standard fonts can show).
pub fn win_ansi(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\r' | ' '..='~' | '\u{A0}'..='\u{FF}') || "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ".contains(ch)
}

/// A cell's style with its table style and conditional format applied: the upstream UI's
/// `look` (ui-egui/src/grid.rs), rule for rule.
fn effective_style(wb: &Workbook, si: usize, sh: &Sheet, c: CellRef, cf: &mut CfCache) -> (Style, Option<gridcraft_engine::cf::CfLook>) {
    let mut style = wb.styles.get(sh.style_id(c)).clone();
    if let Some(t) = sh.table_at(c) {
        style = tables::table_cell_style(wb, si, t, c);
    }
    let cfl = cf.look(wb, si, c);
    if let Some(l) = &cfl
        && let Some(st) = &l.style
    {
        if st.fill.pattern != PatternType::None {
            style.fill = st.fill;
        }
        if st.font.color != Color::Auto {
            style.font.color = st.font.color;
        }
        if st.font.bold {
            style.font.bold = true;
        }
        if st.font.italic {
            style.font.italic = true;
        }
        if st.font.underline != Underline::None {
            style.font.underline = st.font.underline;
        }
        if st.font.strike {
            style.font.strike = true;
        }
        // In a differential style General means "not set".
        if st.num_fmt.as_str() != "General" {
            style.num_fmt = st.num_fmt.clone();
        }
        for (dst, src) in [
            (&mut style.borders.left, st.borders.left),
            (&mut style.borders.right, st.borders.right),
            (&mut style.borders.top, st.borders.top),
            (&mut style.borders.bottom, st.borders.bottom),
        ] {
            if !src.is_none() {
                *dst = src;
            }
        }
    }
    (style, cfl)
}

/// The look of a cell as the viewer draws it, every colour resolved to RGB.
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Look {
    font: String,
    size: f32,
    bold: bool,
    italic: bool,
    underline: u8,
    strike: bool,
    color: Option<[u8; 3]>,
    fill: Option<[u8; 3]>,
    h: u8,
    v: u8,
    wrap: bool,
    shrink: bool,
    indent: u8,
    rotation: i16,
    borders: [(u8, Option<[u8; 3]>); 4],
    bar: Option<(f32, [u8; 3], bool)>,
    icon: Option<(String, usize)>,
}

impl Look {
    /// Nothing an empty cell would show (a font alone draws nothing without text).
    fn is_plain(&self) -> bool {
        self.fill.is_none() && self.borders.iter().all(|b| b.0 == 0) && self.bar.is_none() && self.icon.is_none()
    }
}

fn base_look(wb: &Workbook, style: Style, cfl: Option<&gridcraft_engine::cf::CfLook>) -> Look {
    let theme = &wb.theme;
    let mut fill = match style.fill.pattern {
        PatternType::None => None,
        PatternType::Solid => style.fill.fg.resolve(theme),
        _ => {
            // Patterns: a blend of foreground and background, as the upstream grid draws them.
            let fg = style.fill.fg.resolve(theme).unwrap_or([0, 0, 0]);
            let bg = style.fill.bg.resolve(theme).unwrap_or([255, 255, 255]);
            Some([0, 1, 2].map(|i| ((fg[i] as u16 + bg[i] as u16 * 3) / 4) as u8))
        }
    };
    if let Some(rgb) = cfl.and_then(|l| l.scale_fill) {
        fill = Some(rgb);
    }
    let border = |b: BorderLine| (border_code(b.style), b.color.resolve(theme));
    Look {
        font: style.font.name.clone(),
        size: style.font.size,
        bold: style.font.bold,
        italic: style.font.italic,
        underline: match style.font.underline {
            Underline::None => 0,
            Underline::Single | Underline::SingleAccounting => 1,
            Underline::Double | Underline::DoubleAccounting => 2,
        },
        strike: style.font.strike,
        color: style.font.color.resolve(theme),
        fill,
        h: match style.align.h {
            HAlign::General => 0,
            HAlign::Left => 1,
            HAlign::Center => 2,
            HAlign::Right => 3,
            HAlign::Fill => 4,
            HAlign::Justify => 5,
            HAlign::CenterAcross => 6,
            HAlign::Distributed => 7,
        },
        v: match style.align.v {
            VAlign::Top => 0,
            VAlign::Center => 1,
            VAlign::Bottom => 2,
            VAlign::Justify => 3,
            VAlign::Distributed => 4,
        },
        wrap: style.align.wrap,
        shrink: style.align.shrink,
        indent: style.align.indent,
        rotation: style.align.rotation,
        borders: [border(style.borders.left), border(style.borders.top), border(style.borders.right), border(style.borders.bottom)],
        bar: cfl.and_then(|l| l.bar),
        icon: cfl.and_then(|l| l.icon.clone()),
    }
}

/// Border styles as numbers: 0 none, 1 thin, 2 medium, 3 thick, 4 dashed, 5 dotted, 6 double,
/// 7 hair, 8 medium dashed, 9 dash-dot, 10 medium dash-dot, 11 dash-dot-dot, 12 medium
/// dash-dot-dot, 13 slant dash-dot.
fn border_code(s: BorderStyle) -> u8 {
    match s {
        BorderStyle::None => 0,
        BorderStyle::Thin => 1,
        BorderStyle::Medium => 2,
        BorderStyle::Thick => 3,
        BorderStyle::Dashed => 4,
        BorderStyle::Dotted => 5,
        BorderStyle::Double => 6,
        BorderStyle::Hair => 7,
        BorderStyle::MediumDashed => 8,
        BorderStyle::DashDot => 9,
        BorderStyle::MediumDashDot => 10,
        BorderStyle::DashDotDot => 11,
        BorderStyle::MediumDashDotDot => 12,
        BorderStyle::SlantDashDot => 13,
    }
}

fn hex(rgb: Option<[u8; 3]>) -> Json {
    match rgb {
        Some([r, g, b]) => json!(format!("#{r:02X}{g:02X}{b:02X}")),
        None => Json::Null,
    }
}

/// The JSON the viewer reads for a look (colours as `#RRGGBB`, null = automatic).
fn look_json(l: &Look, _theme: &Theme) -> Json {
    let borders: Vec<Json> = l.borders.iter().map(|(s, c)| if *s == 0 { Json::Null } else { json!([s, hex(*c)]) }).collect();
    json!({
        "font": l.font,
        "size": l.size,
        "bold": l.bold,
        "italic": l.italic,
        "underline": l.underline,
        "strike": l.strike,
        "color": hex(l.color),
        "fill": hex(l.fill),
        "h": l.h,
        "v": l.v,
        "wrap": l.wrap,
        "shrink": l.shrink,
        "indent": l.indent,
        "rotation": l.rotation,
        // left, top, right, bottom: null or [style, colour].
        "borders": borders,
        "bar": l.bar.map(|(f, rgb, gradient)| json!([f, hex(Some(rgb)), gradient])),
        "icon": l.icon.as_ref().map(|(set, i)| json!([set, i])),
    })
}

struct Shown {
    text: String,
    color: Option<String>,
    numeric: bool,
    checkbox: bool,
    fill: Option<(char, usize)>,
}

/// The text a cell shows in the grid: the upstream UI's `display_text`, with the column width
/// in place of its font measure.
fn shown_text(wb: &Workbook, sh: &Sheet, c: CellRef, v: &Value, st: &Style, digit_px: f32, hide_value: bool) -> Shown {
    let empty = |numeric| Shown { text: String::new(), color: None, numeric, checkbox: false, fill: None };
    if hide_value {
        return empty(false);
    }
    if sh.show_formulas
        && let Some(f) = sh.cell(c).and_then(|x| x.formula.as_ref())
        && !display::formula_hidden(wb, sh, c)
    {
        return Shown { text: format!("={}", f.text), color: None, numeric: false, checkbox: false, fill: None };
    }
    if !sh.show_zeros && v.as_f64() == Some(0.0) {
        return empty(true);
    }
    let code = st.num_fmt.as_str();
    if code == "checkbox" && matches!(v, Value::Bool(_) | Value::Empty) {
        return Shown { text: String::new(), color: None, numeric: false, checkbox: true, fill: None };
    }
    if code == "General"
        && let Value::Number(n) = v
        && digit_px > 0.0
    {
        let width = match sh.merge_at(c) {
            Some(m) if m.start == c => (m.start.col..=m.end.col).map(|col| sh.col_width(col)).sum::<f32>(),
            _ => sh.col_width(c.col),
        };
        let max_chars = ((width - 4.0) / digit_px).floor().max(1.0) as usize;
        let text = gridcraft_engine::numfmt::format_general_fit(*n, max_chars.min(11)).unwrap_or_else(|| "#".repeat(max_chars.max(1)));
        return Shown { text, color: None, numeric: true, checkbox: false, fill: None };
    }
    let f = display::format(v, code, wb);
    let color = f.color.map(|c| {
        use gridcraft_engine::numfmt::FormatColor as F;
        match c {
            F::Black | F::Indexed(_) => "#000000",
            F::Blue => "#0000FF",
            F::Cyan => "#00FFFF",
            F::Green => "#008000",
            F::Magenta => "#FF00FF",
            F::Red => "#FF0000",
            F::White => "#FFFFFF",
            F::Yellow => "#FFFF00",
        }
        .to_string()
    });
    Shown { text: f.text, color, numeric: f.numeric, checkbox: false, fill: f.fill }
}
