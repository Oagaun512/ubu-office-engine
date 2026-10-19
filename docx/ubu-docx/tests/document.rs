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
