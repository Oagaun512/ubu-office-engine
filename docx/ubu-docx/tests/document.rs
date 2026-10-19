//! Native tests of the DOCX engine API on documents written here with wordcraft's own session,
//! so no fixture files are needed.

use serde_json::json;
use ubu_docx::{Document, ErrorCode, MAX_PIXELS};
use wordcraft_engine::{Region, Session, blank_document};

/// A DOCX whose pages hold the given paragraphs; `None` is a page break.
fn docx(parts: &[Option<&str>]) -> Vec<u8> {
    let mut s = Session::new(blank_document(&Region::default()));
    let mut first = true;
    for part in parts {
        match part {
            Some(text) => {
                if !first {
                    s.run("text.newParagraph", &json!({})).unwrap();
                }
                s.run("text.insert", &json!({ "text": text })).unwrap();
                first = false;
            }
            None => {
                s.run("insert.pageBreak", &json!({})).unwrap();
                first = true;
            }
        }
    }
    wordcraft_engine::io::save_bytes("test.docx", &s.doc).unwrap()
}

#[test]
fn opens_a_docx_and_lays_out_its_pages() {
    let doc = Document::open(&docx(&[Some("Hello Ubu 1"), None, Some("Hello Ubu 2")]), "test.docx", None).unwrap();
    assert_eq!(doc.page_count(), 2);
    let size = doc.page_size(0).unwrap();
    assert!(size.width_pt > 500.0 && size.height_pt > 700.0, "a Letter or A4 page in points: {size:?}");
}

#[test]
fn rejects_what_is_not_a_document() {
    let err = Document::open(b"PK\x03\x04 not really a zip", "broken.docx", None).err().unwrap();
    assert_eq!(err.code, ErrorCode::Invalid);
    let err = Document::open(b"%PDF-1.7", "file.pdf", None).err().unwrap();
    assert_eq!(err.code, ErrorCode::Invalid);
}

#[test]
fn extracts_each_pages_text() {
    let doc = Document::open(&docx(&[Some("First line"), Some("Second line"), None, Some("Next page")]), "t.docx", None).unwrap();
    assert_eq!(doc.page_text(0).unwrap().trim(), "First line\nSecond line");
    assert_eq!(doc.page_text(1).unwrap().trim(), "Next page");
    assert_eq!(doc.page_text(2).err().unwrap().code, ErrorCode::OutOfRange);
}

#[test]
fn renders_straight_alpha_pixels_with_ink_on_the_page() {
    let doc = Document::open(&docx(&[Some("Ink on paper")]), "t.docx", None).unwrap();
    let size = doc.page_size(0).unwrap();
    let r = doc.render_page(0, 1.0).unwrap();
    assert_eq!((r.width, r.height), (size.width_pt.round() as u32, size.height_pt.round() as u32));
    assert_eq!(r.rgba.len(), (r.width * r.height * 4) as usize);
    let pixels = r.rgba.as_chunks::<4>().0;
    assert!(pixels.iter().all(|px| px[3] == 255), "an opaque white page");
    let dark = pixels.iter().filter(|px| px[0] < 100).count();
    assert!(dark > 50, "the text left ink ({dark} dark pixels)");
}

#[test]
fn render_rejects_bad_pages_and_scales_and_caps_huge_ones() {
    let doc = Document::open(&docx(&[Some("x")]), "t.docx", None).unwrap();
    assert_eq!(doc.render_page(1, 1.0).err().unwrap().code, ErrorCode::OutOfRange);
    assert_eq!(doc.render_page(0, 0.0).err().unwrap().code, ErrorCode::OutOfRange);
    assert_eq!(doc.render_page(0, f32::NAN).err().unwrap().code, ErrorCode::OutOfRange);
    let s = doc.effective_scale(0, 100.0).unwrap();
    let size = doc.page_size(0).unwrap();
    assert!(size.width_pt * s * size.height_pt * s <= MAX_PIXELS * 1.001);
}

#[test]
fn exports_pdf() {
    let doc = Document::open(&docx(&[Some("To PDF"), None, Some("Page two")]), "t.docx", None).unwrap();
    let pdf = doc.export_pdf().unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(pdf.windows(5).filter(|w| *w == b"/Page").count() >= 2, "two page objects");
}

#[test]
fn saves_docx_that_reopens_with_the_same_text() {
    let doc = Document::open(&docx(&[Some("Round trip"), None, Some("Again")]), "t.docx", None).unwrap();
    let saved = doc.save_docx().unwrap();
    assert!(saved.starts_with(b"PK"), "a DOCX is a zip");
    let again = Document::open(&saved, "t.docx", None).unwrap();
    assert_eq!(again.page_count(), 2);
    assert_eq!(again.page_text(1).unwrap().trim(), "Again");
}

#[test]
fn a_misnamed_file_opens_by_its_content() {
    let doc = Document::open(&docx(&[Some("Named wrong")]), "upload.bin", None);
    // wordcraft sniffs the content when the extension is unknown or contradicts it.
    let doc = doc.unwrap();
    assert_eq!(doc.page_text(0).unwrap().trim(), "Named wrong");
}

#[test]
fn adding_a_font_and_relaying_out_keeps_the_document() {
    let mut doc = Document::open(&docx(&[Some("Fonts")]), "t.docx", None).unwrap();
    assert_eq!(ubu_docx::add_font(b"not a font".to_vec()), 0, "garbage is ignored");
    doc.relayout();
    assert_eq!(doc.page_text(0).unwrap().trim(), "Fonts");
}

#[test]
fn error_codes_are_the_contract() {
    let codes = [
        ErrorCode::Invalid,
        ErrorCode::NeedsPassword,
        ErrorCode::WrongPassword,
        ErrorCode::OutOfRange,
        ErrorCode::ExportFailed,
        ErrorCode::SaveFailed,
    ];
    assert_eq!(codes.map(ErrorCode::code), ["invalid", "needs_password", "wrong_password", "out_of_range", "export_failed", "save_failed"]);
}

// ── Text layer and find (Ubu Office Phase 3, D3) ─────────────────────────────────────────────

#[test]
fn text_layer_has_each_line_with_a_box_inside_the_page() {
    let doc = Document::open(&docx(&[Some("First line"), Some("Second line"), None, Some("Next page")]), "t.docx", None).unwrap();
    let size = doc.page_size(0).unwrap();
    let lines = doc.page_text_layer(0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["First line", "Second line"]);
    for l in &lines {
        let [x0, y0, x1, y1] = l.rect;
        assert!(0.0 <= x0 && x0 < x1 && x1 <= size.width_pt, "x inside the page: {:?}", l.rect);
        assert!(0.0 <= y0 && y0 < y1 && y1 <= size.height_pt, "y inside the page: {:?}", l.rect);
        assert_eq!(l.angle, 0.0);
    }
    assert!(lines[0].rect[1] < lines[1].rect[1], "the second line is lower");
    assert_eq!(doc.page_text_layer(1).unwrap()[0].text, "Next page");
    assert_eq!(doc.page_text_layer(2).err().unwrap().code, ErrorCode::OutOfRange);
}

#[test]
fn find_honours_case_and_whole_words_with_boxes_inside_their_line() {
    let doc = Document::open(&docx(&[Some("Ubu reads ubu and Ubuntu")]), "t.docx", None).unwrap();
    let line = doc.page_text_layer(0).unwrap()[0].rect;
    let all = doc.find_text(0, "ubu", false, false).unwrap();
    assert_eq!(all.len(), 3, "Ubu, ubu, and the start of Ubuntu");
    assert_eq!(doc.find_text(0, "ubu", true, false).unwrap().len(), 1, "match case");
    assert_eq!(doc.find_text(0, "ubu", false, true).unwrap().len(), 2, "whole words leave out Ubuntu");
    assert!(doc.find_text(0, "   ", false, false).unwrap().is_empty());
    let mut last_x = f32::NEG_INFINITY;
    for m in &all {
        let [x0, y0, x1, y1] = m.rects[0];
        assert!(x0 >= line[0] - 0.01 && x1 <= line[2] + 0.01 && x0 < x1, "inside the line: {:?} in {line:?}", m.rects[0]);
        assert_eq!((y0, y1), (line[1], line[3]));
        assert!(x0 > last_x, "matches in reading order");
        last_x = x0;
    }
    let ubuntu = doc.find_text(0, "Ubuntu", true, true).unwrap();
    assert_eq!(ubuntu.len(), 1);
    assert!(ubuntu[0].rects[0][2] - ubuntu[0].rects[0][0] > all[0].rects[0][2] - all[0].rects[0][0], "a longer word has a wider box");
    assert_eq!(doc.find_text(1, "x", false, false).err().unwrap().code, ErrorCode::OutOfRange);
}

#[test]
fn text_layer_agrees_with_page_text() {
    let doc = Document::open(&docx(&[Some("Alpha beta"), Some("Gamma delta")]), "t.docx", None).unwrap();
    let joined = doc.page_text_layer(0).unwrap().into_iter().map(|l| l.text).collect::<Vec<_>>().join("\n");
    assert_eq!(joined.trim(), doc.page_text(0).unwrap().trim());
}

#[test]
fn a_break_at_the_end_of_a_line_is_not_copied() {
    // The paragraph before a page break ends with the break character (U+000C).
    let doc = Document::open(&docx(&[Some("Before"), None, Some("After")]), "t.docx", None).unwrap();
    for page in 0..doc.page_count() {
        for l in doc.page_text_layer(page).unwrap() {
            assert!(!l.text.chars().any(|c| c.is_control() && c != '\t'), "no control characters: {:?}", l.text);
        }
    }
}

// ── Editing (Ubu Office Phase 3, D5) ─────────────────────────────────────────────────────────

#[test]
fn typing_changes_the_document_lays_it_out_again_and_undoes() {
    let mut doc = Document::open(&docx(&[Some("Hello")]), "t.docx", None).unwrap();
    assert!(!doc.is_dirty() && !doc.can_undo());
    let moved = doc.run("caret.docEnd", "{}").unwrap();
    assert!(!moved.changed, "moving the caret changes nothing");
    let typed = doc.run("text.insert", r#"{"text":" world"}"#).unwrap();
    assert!(typed.changed && typed.rev > moved.rev);
    assert_eq!(doc.page_text(0).unwrap().trim(), "Hello world");
    assert_eq!(doc.page_text_layer(0).unwrap()[0].text, "Hello world", "the text layer follows the edit");
    assert!(doc.is_dirty() && doc.can_undo());
    doc.run("edit.undo", "").unwrap();
    assert_eq!(doc.page_text(0).unwrap().trim(), "Hello");
    assert!(doc.can_redo());
    doc.run("edit.redo", "").unwrap();
    assert_eq!(doc.page_text(0).unwrap().trim(), "Hello world");
}

#[test]
fn enter_and_backspace_split_and_join_paragraphs() {
    let mut doc = Document::open(&docx(&[Some("AB")]), "t.docx", None).unwrap();
    doc.run("caret.docStart", "{}").unwrap();
    doc.run("caret.right", "{}").unwrap();
    doc.run("text.newParagraph", "{}").unwrap();
    assert_eq!(doc.page_text_layer(0).unwrap().iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    doc.run("text.backspace", "{}").unwrap();
    assert_eq!(doc.page_text(0).unwrap().trim(), "AB");
}

#[test]
fn the_caret_and_selection_are_boxes_on_the_page() {
    let mut doc = Document::open(&docx(&[Some("Select me")]), "t.docx", None).unwrap();
    let line = doc.page_text_layer(0).unwrap()[0].rect;
    doc.run("caret.docStart", "{}").unwrap();
    let start = doc.caret().expect("a caret on the first line");
    assert_eq!(start.page, 0);
    assert!((start.x - line[0]).abs() < 1.0 && start.top >= line[1] - 1.0 && start.height > 0.0);
    assert!(doc.selection_rects().is_empty(), "a plain caret has no selection");
    doc.run("caret.end", r#"{"extend":true}"#).unwrap();
    let rects = doc.selection_rects();
    assert!(!rects.is_empty());
    assert_eq!(rects[0].0, 0);
    assert!(rects[0].1[2] > rects[0].1[0] + 10.0, "the selection covers the words");
    // A click: caret.set by page point lands on the line it hits.
    let mid = [(line[0] + line[2]) / 2.0, (line[1] + line[3]) / 2.0];
    doc.run("caret.set", &format!(r#"{{"page":0,"x":{},"y":{}}}"#, mid[0], mid[1])).unwrap();
    let c = doc.caret().unwrap();
    assert!(c.x > line[0] && c.x < line[2], "the caret lands inside the line");
    assert!(doc.selection_rects().is_empty());
}

#[test]
fn copy_puts_the_selection_on_the_clipboard_and_paste_inserts_text() {
    let mut doc = Document::open(&docx(&[Some("Copy this")]), "t.docx", None).unwrap();
    doc.run("select.all", "{}").unwrap();
    doc.run("edit.copy", "{}").unwrap();
    assert_eq!(doc.clipboard_text().trim(), "Copy this");
    doc.run("caret.docEnd", "{}").unwrap();
    doc.run("edit.paste", r#"{"text":" and that"}"#).unwrap();
    assert_eq!(doc.page_text(0).unwrap().trim(), "Copy this and that");
}

#[test]
fn bad_commands_are_errors_not_panics() {
    let mut doc = Document::open(&docx(&[Some("x")]), "t.docx", None).unwrap();
    assert_eq!(doc.run("no.such.command", "{}").err().unwrap().code, ErrorCode::InvalidCommand);
    assert_eq!(doc.run("text.insert", "not json").err().unwrap().code, ErrorCode::InvalidCommand);
    assert_eq!(doc.run("text.insert", r#"{"wrong":1}"#).err().unwrap().code, ErrorCode::InvalidCommand);
    assert_eq!(ErrorCode::Unsupported.code(), "unsupported");
    assert_eq!(ErrorCode::InvalidCommand.code(), "invalid_command");
}
