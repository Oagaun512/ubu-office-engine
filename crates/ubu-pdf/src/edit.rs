//! Editing (Ubu Office 2.0, E1): page operations and comments over the upstream object layer,
//! written back as an incremental update so the original bytes, and any signature over them,
//! stay exactly as they were.
//!
//! Page operations come from `pdfcraft-organize` and comments from `pdfcraft-annot`; neither is
//! modified here. This module only maps view-space geometry (points, origin top-left, y down,
//! after `/Rotate`: the space of [`crate::PageSize`] and the text layer) to each page's user
//! space, and maps upstream errors to [`ErrorCode::EditFailed`].

use std::sync::Arc;

use pdfcraft_annot::{Markup, Meta, NOTE_SIZE, NewAnnotation, NoteIcon, Shape, Style};
use pdfcraft_render::PageInfo;

use crate::{ErrorCode, PdfError};

/// A set of edits to one document. Each call changes it in place; [`Edit::save`] writes the
/// original bytes followed by one incremental revision holding every change.
pub struct Edit {
    doc: pdfcraft_cos::Document,
    /// The pages as opened: comments are placed with this geometry.
    geometry: Vec<PageInfo>,
    /// A page operation has changed the page list or a page's rotation, so `geometry` no longer
    /// describes the pages and comments are refused until the result is reopened.
    pages_changed: bool,
    /// The PDF date (`D:…`) stamped on new comments and on `/ModDate`.
    date: Option<String>,
    /// Prefix of each new comment's unique name (`/NM`).
    id_prefix: String,
    next_id: u64,
    /// Document-information entries set so far, repeated on extracted copies.
    info: Vec<(String, String)>,
}

impl Edit {
    pub(crate) fn new(doc: pdfcraft_cos::Document, geometry: Vec<PageInfo>, date: Option<&str>, id_prefix: &str) -> Self {
        let date = date.map(str::trim).filter(|d| !d.is_empty()).map(str::to_owned);
        Self { doc, geometry, pages_changed: false, date, id_prefix: id_prefix.to_owned(), next_id: 0, info: Vec::new() }
    }

    /// Pages in the document as edited so far.
    pub fn page_count(&self) -> usize {
        pdfcraft_organize::page_count(&self.doc).unwrap_or(0)
    }

    /// Rotate pages by a multiple of 90 degrees, clockwise.
    pub fn rotate_pages(&mut self, pages: &[u32], degrees: i64) -> Result<(), PdfError> {
        pdfcraft_organize::rotate_pages(&mut self.doc, &indices(pages), degrees).map_err(edit_error)?;
        self.pages_changed = true;
        Ok(())
    }

    /// Delete pages. Deleting every page is refused.
    pub fn delete_pages(&mut self, pages: &[u32]) -> Result<(), PdfError> {
        pdfcraft_organize::delete_pages(&mut self.doc, &indices(pages)).map_err(edit_error)?;
        self.pages_changed = true;
        Ok(())
    }

    /// Move pages (kept in their relative order) so they start at position `to` of the result.
    pub fn move_pages(&mut self, pages: &[u32], to: u32) -> Result<(), PdfError> {
        pdfcraft_organize::move_pages(&mut self.doc, &indices(pages), to as usize).map_err(edit_error)?;
        self.pages_changed = true;
        Ok(())
    }

    /// Insert pages of another PDF at position `at` (0 = before the first page; past the end =
    /// after the last). `pages` empty inserts every page. As Acrobat's Insert from File does, form
    /// fields whose names are taken are renamed, and the file's bookmarks come along under one
    /// bookmark titled `name` (nothing is added when it has none). Returns how many pages were
    /// inserted.
    pub fn insert_pages(&mut self, bytes: Vec<u8>, password: Option<&str>, name: &str, pages: &[u32], at: u32) -> Result<usize, PdfError> {
        let bytes = Arc::new(bytes);
        let password = password.filter(|p| !p.is_empty());
        // The renderer's inspection classifies a file that is not a PDF or needs a password, the
        // same way `Document::open` reports them.
        let info = pdfcraft_render::inspect(bytes.clone(), password)?;
        let src = pdfcraft_cos::Document::open_with_password(bytes, password).map_err(|e| PdfError::new(ErrorCode::Invalid, e.to_string()))?;
        let wanted = if pages.is_empty() { (0..info.pages.len()).collect() } else { indices(pages) };
        let at = (at as usize).min(self.page_count());
        let added = pdfcraft_organize::insert_file_pages(&mut self.doc, &src, name, &wanted, at, pdfcraft_organize::FieldConflicts::Rename)
            .map_err(edit_error)?;
        self.pages_changed = true;
        Ok(added.pages.len())
    }

    /// Text markup over view-space rectangles (4 numbers each: x0, y0, x1, y1), usually the
    /// selected text's line boxes. `kind`: highlight, underline, strikeout or squiggly.
    #[allow(clippy::too_many_arguments)]
    pub fn markup(
        &mut self,
        page: u32,
        kind: &str,
        rects: &[f32],
        color: &[f32],
        opacity: f64,
        contents: &str,
        author: &str,
    ) -> Result<(), PdfError> {
        let kind = match kind.to_ascii_lowercase().as_str() {
            "highlight" => Markup::Highlight,
            "underline" => Markup::Underline,
            "strikeout" | "strike" => Markup::StrikeOut,
            "squiggly" => Markup::Squiggly,
            other => return Err(PdfError::new(ErrorCode::EditFailed, format!("unknown text markup {other:?}"))),
        };
        let g = self.geometry(page)?;
        let (boxes, rest) = rects.as_chunks::<4>();
        if boxes.is_empty() || !rest.is_empty() {
            return Err(PdfError::new(ErrorCode::EditFailed, "text markup needs one or more rectangles of 4 numbers"));
        }
        let quads = boxes.iter().map(|r| g.view_rect_to_quad(*r)).collect();
        self.add(page, Shape::TextMarkup { kind, quads }, color, opacity, None, contents, author)
    }

    /// A sticky note whose icon's top-left corner, as displayed, is at (`x`, `y`).
    pub fn note(&mut self, page: u32, x: f32, y: f32, contents: &str, author: &str, color: &[f32]) -> Result<(), PdfError> {
        let g = self.geometry(page)?;
        let size = NOTE_SIZE as f32;
        // Under /Rotate the displayed top-left corner of the icon is another corner of its
        // user-space square, so the square is converted, not the point.
        let r = g.view_rect_to_user([x, y, x + size, y + size]);
        let at = [f64::from(r[0]), f64::from(r[3])];
        self.add(page, Shape::Note { at, icon: NoteIcon::default() }, color, 1.0, None, contents, author)
    }

    /// A text box showing `contents` in the view-space rectangle `rect` (x0, y0, x1, y1).
    pub fn text_box(&mut self, page: u32, rect: &[f32], font_size: f64, contents: &str, author: &str, color: &[f32]) -> Result<(), PdfError> {
        let g = self.geometry(page)?;
        let r = rect4(rect)?;
        let u = g.view_rect_to_user(r).map(f64::from);
        self.add(page, Shape::TextBox { rect: u, font_size }, color, 1.0, None, contents, author)
    }

    /// Freehand ink: `points` holds x, y pairs in view space, `counts` how many points each
    /// stroke has, in order.
    #[allow(clippy::too_many_arguments)]
    pub fn ink(&mut self, page: u32, points: &[f32], counts: &[u32], width: f64, color: &[f32], opacity: f64, author: &str) -> Result<(), PdfError> {
        let g = self.geometry(page)?;
        let (pairs, rest) = points.as_chunks::<2>();
        let total: usize = counts.iter().map(|c| *c as usize).sum();
        if !rest.is_empty() || total != pairs.len() || counts.is_empty() || counts.contains(&0) {
            return Err(PdfError::new(ErrorCode::EditFailed, "ink needs x, y pairs and a point count for each stroke"));
        }
        let mut strokes = Vec::with_capacity(counts.len());
        let mut at = 0;
        for &n in counts {
            let stroke = pairs[at..at + n as usize]
                .iter()
                .map(|[x, y]| {
                    let [ux, uy] = g.view_to_user(*x, *y);
                    [f64::from(ux), f64::from(uy)]
                })
                .collect();
            strokes.push(stroke);
            at += n as usize;
        }
        self.add(page, Shape::Ink { strokes }, color, opacity, Some(width), "", author)
    }

    /// Set (or clear, with an empty value) a document-information entry such as `Producer`. The
    /// XMP metadata, when the file has it, is kept in step.
    pub fn set_info(&mut self, key: &str, value: &str) -> Result<(), PdfError> {
        pdfcraft_organize::set_info(&mut self.doc, key, value).map_err(edit_error)?;
        self.info.retain(|(k, _)| k != key);
        self.info.push((key.to_owned(), value.to_owned()));
        Ok(())
    }

    /// The original bytes followed by one incremental update holding every edit (ISO 32000-2
    /// §7.5.6). A file that had to be reconstructed on open cannot be appended to and is
    /// written in full instead (upstream `write_incremental`).
    pub fn save(&self) -> Result<Vec<u8>, PdfError> {
        let opts = pdfcraft_cos::SaveOptions { mod_date: self.date.clone(), ..pdfcraft_cos::SaveOptions::default() };
        pdfcraft_cos::write_incremental(&self.doc, &opts).map_err(|e| PdfError::new(ErrorCode::SaveFailed, e.to_string()))
    }

    /// A new PDF of the given pages (in document order) of the document as edited so far (a full
    /// write), carrying the document-information entries set on this edit.
    pub fn extract(&self, pages: &[u32]) -> Result<Vec<u8>, PdfError> {
        if pages.is_empty() {
            return Err(PdfError::new(ErrorCode::EditFailed, "no pages to extract"));
        }
        let mut wanted = indices(pages);
        wanted.sort_unstable();
        wanted.dedup();
        let mut out = pdfcraft_organize::extract_pages(&self.doc, &wanted).map_err(edit_error)?;
        for (k, v) in &self.info {
            pdfcraft_organize::set_info(&mut out, k, v).map_err(edit_error)?;
        }
        let opts = pdfcraft_cos::SaveOptions { mod_date: self.date.clone(), ..pdfcraft_cos::SaveOptions::default() };
        pdfcraft_cos::write_full(&out, &opts).map_err(|e| PdfError::new(ErrorCode::SaveFailed, e.to_string()))
    }

    /// The geometry of `page` as opened, for placing a comment.
    fn geometry(&self, page: u32) -> Result<PageInfo, PdfError> {
        if self.pages_changed {
            return Err(PdfError::new(
                ErrorCode::EditFailed,
                "comments are placed on the pages as opened: save the page changes and open the result first",
            ));
        }
        let n = self.geometry.len();
        self.geometry.get(page as usize).cloned().ok_or_else(|| crate::out_of_range(page as usize, n))
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        page: u32,
        shape: Shape,
        color: &[f32],
        opacity: f64,
        width: Option<f64>,
        contents: &str,
        author: &str,
    ) -> Result<(), PdfError> {
        let mut style = Style::default_for(&shape);
        if let [r, g, b] = color {
            style.color = [f64::from(*r), f64::from(*g), f64::from(*b)].map(|c| c.clamp(0.0, 1.0));
        } else if !color.is_empty() {
            return Err(PdfError::new(ErrorCode::EditFailed, "a colour is 3 numbers (red, green, blue, 0 to 1)"));
        }
        if opacity.is_finite() {
            style.opacity = opacity.clamp(0.0, 1.0);
        }
        if let Some(w) = width.filter(|w| w.is_finite() && *w > 0.0) {
            style.width = w.min(100.0);
        }
        self.next_id += 1;
        let meta = Meta { date: self.date.clone(), id: format!("{}-{}", self.id_prefix, self.next_id) };
        let new = NewAnnotation { page: page as usize, shape, style, contents: contents.to_owned(), author: author.to_owned() };
        pdfcraft_annot::add_annotation(&mut self.doc, &new, &meta).map(|_| ()).map_err(edit_error)
    }
}

fn indices(pages: &[u32]) -> Vec<usize> {
    pages.iter().map(|p| *p as usize).collect()
}

fn rect4(r: &[f32]) -> Result<[f32; 4], PdfError> {
    match r {
        [x0, y0, x1, y1] if [x0, y0, x1, y1].iter().all(|v| v.is_finite()) => Ok([*x0, *y0, *x1, *y1]),
        _ => Err(PdfError::new(ErrorCode::EditFailed, "a rectangle is 4 numbers: x0, y0, x1, y1")),
    }
}

fn edit_error(e: impl std::fmt::Display) -> PdfError {
    PdfError::new(ErrorCode::EditFailed, e.to_string())
}
