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
    assert!(r.rgba.chunks_exact(4).all(|px| px[3] == 255), "an opaque white page");
    let dark = r.rgba.chunks_exact(4).filter(|px| px[0] < 100).count();
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
