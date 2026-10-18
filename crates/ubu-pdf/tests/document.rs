//! Native tests of the engine API on PDFs generated here, so no fixture files are needed.

use ubu_pdf::{Document, ErrorCode, MAX_PIXELS, unpremultiply};

/// A valid PDF with one page per entry of `pages`, each `[w, h]` points, showing `text` in
/// Helvetica. Offsets in the cross-reference table are computed, not hand-typed.
fn pdf(pages: &[[u32; 2]], text: &str) -> Vec<u8> {
    let n = pages.len();
    // Objects: 1 catalog, 2 page tree, 3 font, then (page, content) pairs.
    let mut objects: Vec<String> = Vec::new();
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    let kids = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect::<Vec<_>>().join(" ");
    objects.push(format!("<< /Type /Pages /Kids [{kids}] /Count {n} >>"));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for (i, [w, h]) in pages.iter().enumerate() {
        let content_id = 5 + 2 * i;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >>"
        ));
        let stream = format!("BT /F1 24 Tf 20 {} Td ({text} {}) Tj ET", h.saturating_sub(40), i + 1);
        objects.push(format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()));
    }
    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

#[test]
fn opens_and_counts_pages() {
    let doc = Document::open(pdf(&[[612, 792], [842, 595]], "Hello"), None).unwrap();
    assert_eq!(doc.page_count(), 2);
    let a = doc.page_size(0).unwrap();
    let b = doc.page_size(1).unwrap();
    assert_eq!((a.width_pt, a.height_pt), (612.0, 792.0));
    assert_eq!((b.width_pt, b.height_pt), (842.0, 595.0));
}

#[test]
fn rejects_non_pdf() {
    let err = Document::open(b"this is not a pdf".to_vec(), None).err().unwrap();
    assert_eq!(err.code, ErrorCode::Invalid);
    let err = Document::open(Vec::new(), None).err().unwrap();
    assert_eq!(err.code, ErrorCode::Invalid);
}

#[test]
fn renders_with_expected_dimensions_and_straight_alpha() {
    let mut doc = Document::open(pdf(&[[200, 100]], "Ubu"), None).unwrap();
    let r = doc.render_page(0, 2.0).unwrap();
    assert_eq!((r.width, r.height), (400, 200));
    assert_eq!(r.rgba.len(), 400 * 200 * 4);
    // A page clears to opaque white; the text draws dark pixels on it.
    assert!(r.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255), "page raster is opaque");
    assert!(r.rgba.as_chunks::<4>().0.iter().any(|p| p[0] < 128), "the text left ink on the page");
}

#[test]
fn render_rejects_bad_index_and_scale() {
    let mut doc = Document::open(pdf(&[[200, 100]], "Ubu"), None).unwrap();
    assert_eq!(doc.render_page(1, 1.0).err().unwrap().code, ErrorCode::OutOfRange);
    assert_eq!(doc.render_page(0, 0.0).err().unwrap().code, ErrorCode::OutOfRange);
    assert_eq!(doc.render_page(0, f32::NAN).err().unwrap().code, ErrorCode::OutOfRange);
    assert_eq!(doc.render_page(0, -1.0).err().unwrap().code, ErrorCode::OutOfRange);
}

#[test]
fn huge_scale_is_capped_to_the_pixel_budget() {
    let mut doc = Document::open(pdf(&[[612, 792]], "Big"), None).unwrap();
    let r = doc.render_page(0, 100.0).unwrap();
    assert!((r.width as f32) * (r.height as f32) <= MAX_PIXELS * 1.01, "{}x{}", r.width, r.height);
}

#[test]
fn extracts_text_per_page() {
    let mut doc = Document::open(pdf(&[[612, 792], [612, 792]], "Hello Ubu"), None).unwrap();
    assert_eq!(doc.page_text(0).unwrap().trim(), "Hello Ubu 1");
    assert_eq!(doc.page_text(1).unwrap().trim(), "Hello Ubu 2");
    assert_eq!(doc.page_text(2).err().unwrap().code, ErrorCode::OutOfRange);
}

#[test]
fn save_round_trips() {
    let original = pdf(&[[300, 300], [300, 300], [300, 300]], "Round trip");
    let doc = Document::open(original, None).unwrap();
    let saved = doc.save().unwrap();
    assert!(saved.starts_with(b"%PDF-"));
    let mut reopened = Document::open(saved, None).unwrap();
    assert_eq!(reopened.page_count(), 3);
    assert_eq!(reopened.page_text(2).unwrap().trim(), "Round trip 3");
}

#[test]
fn unpremultiply_restores_straight_alpha() {
    // 50% alpha red stored premultiplied is (128, 0, 0, 128).
    let mut px = [128, 0, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0, 64, 32, 16, 64];
    unpremultiply(&mut px);
    assert_eq!(&px[0..4], &[255, 0, 0, 128]);
    assert_eq!(&px[4..8], &[10, 20, 30, 255], "opaque pixels are untouched");
    assert_eq!(&px[8..12], &[0, 0, 0, 0], "transparent pixels are untouched");
    assert_eq!(&px[12..16], &[255, 128, 64, 64]);
}

#[test]
fn error_codes_are_the_contract_strings() {
    let all = [
        (ErrorCode::Invalid, "invalid"),
        (ErrorCode::NeedsPassword, "needs_password"),
        (ErrorCode::WrongPassword, "wrong_password"),
        (ErrorCode::Unsupported, "unsupported"),
        (ErrorCode::OutOfRange, "out_of_range"),
        (ErrorCode::RenderFailed, "render_failed"),
        (ErrorCode::SaveFailed, "save_failed"),
    ];
    for (code, s) in all {
        assert_eq!(code.code(), s);
    }
}
