//! Native tests of the engine API on PDFs generated here, so no fixture files are needed.

use ubu_pdf::{Allow, Document, ErrorCode, MAX_PIXELS, unpremultiply};

/// A valid PDF with one page per entry of `pages`, each `[w, h]` points, showing `text` in
/// Helvetica from 40 pt below the top. Each `\n` in `text` starts a new line 30 pt lower; the
/// last line ends with the page number. Offsets in the cross-reference table are computed.
fn pdf(pages: &[[u32; 2]], text: &str) -> Vec<u8> {
    pdf_rotated(pages, text, 0)
}

/// [`pdf`] with every page carrying `/Rotate rotate`.
fn pdf_rotated(pages: &[[u32; 2]], text: &str, rotate: u32) -> Vec<u8> {
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
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Rotate {rotate} /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >>"
        ));
        let lines: Vec<&str> = text.split('\n').collect();
        let shown = lines
            .iter()
            .enumerate()
            .map(|(n, l)| if n + 1 == lines.len() { format!("({l} {}) Tj", i + 1) } else { format!("({l}) Tj 0 -30 Td") })
            .collect::<Vec<_>>()
            .join(" ");
        let stream = format!("BT /F1 24 Tf 20 {} Td {shown} ET", h.saturating_sub(40));
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

#[test]
fn text_layer_has_lines_in_reading_order_with_boxes_in_view_space() {
    let mut doc = Document::open(pdf(&[[612, 792]], "First line\nSecond line"), None).unwrap();
    let lines = doc.page_text_layer(0).unwrap();
    let texts: Vec<&str> = lines.iter().map(|l| l.text.trim()).collect();
    assert_eq!(texts, ["First line", "Second line 1"]);
    let [x0, y0, x1, y1] = lines[0].rect;
    assert!((x0 - 20.0).abs() < 3.0, "first line starts at x = 20 pt, got {x0}");
    assert!(y0 >= 0.0 && y1 <= 80.0 && y0 < y1, "first line sits near the top (y down): {:?}", lines[0].rect);
    assert!(x1 > x0 && x1 <= 612.0);
    assert!(lines[1].rect[1] > lines[0].rect[1], "the second line is lower on the page");
    assert!(lines.iter().all(|l| l.angle.abs() < 1e-3), "horizontal text has angle 0");
}

#[test]
fn text_layer_of_a_rotated_page_stays_inside_the_rotated_page() {
    let mut doc = Document::open(pdf_rotated(&[[612, 792]], "Turned", 90), None).unwrap();
    let size = doc.page_size(0).unwrap();
    assert_eq!((size.width_pt, size.height_pt), (792.0, 612.0), "/Rotate 90 swaps the displayed size");
    let lines = doc.page_text_layer(0).unwrap();
    assert_eq!(lines.len(), 1);
    let [x0, y0, x1, y1] = lines[0].rect;
    assert!(x0 >= 0.0 && y0 >= 0.0 && x1 <= 792.0 && y1 <= 612.0, "{:?}", lines[0].rect);
    assert!(lines[0].angle.abs() > 1.0, "rotated text is not horizontal: angle {}", lines[0].angle);
}

#[test]
fn find_text_honours_case_and_whole_word_options() {
    let mut doc = Document::open(pdf(&[[612, 792]], "Hello world hello"), None).unwrap();
    assert_eq!(doc.find_text(0, "hello", false, false).unwrap().len(), 2);
    assert_eq!(doc.find_text(0, "hello", true, false).unwrap().len(), 1);
    assert_eq!(doc.find_text(0, "hell", false, false).unwrap().len(), 2);
    assert_eq!(doc.find_text(0, "hell", false, true).unwrap().len(), 0, "whole words only");
    assert!(doc.find_text(0, "absent", false, false).unwrap().is_empty());
    assert!(doc.find_text(0, "   ", false, false).unwrap().is_empty(), "a blank query finds nothing");
}

#[test]
fn find_text_boxes_sit_inside_their_line() {
    let mut doc = Document::open(pdf(&[[612, 792]], "Hello world"), None).unwrap();
    let line = doc.page_text_layer(0).unwrap()[0].rect;
    let found = doc.find_text(0, "world", false, false).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].rects.len(), 1);
    let [x0, y0, x1, y1] = found[0].rects[0];
    let eps = 0.5;
    assert!(x0 >= line[0] - eps && y0 >= line[1] - eps && x1 <= line[2] + eps && y1 <= line[3] + eps, "{:?} in {line:?}", found[0].rects[0]);
    assert!(x0 > line[0] + 20.0, "\"world\" is not at the start of the line");
}

#[test]
fn a_match_across_a_line_break_gets_one_box_per_line() {
    let mut doc = Document::open(pdf(&[[612, 792]], "alpha beta\ngamma"), None).unwrap();
    let found = doc.find_text(0, "beta gamma", false, false).unwrap();
    assert_eq!(found.len(), 1, "a line break counts as one space");
    assert_eq!(found[0].rects.len(), 2);
    assert!(found[0].rects[1][1] > found[0].rects[0][1], "the second box is on the lower line");
}

#[test]
fn text_layer_and_search_reject_bad_pages_and_agree_with_page_text() {
    let mut doc = Document::open(pdf(&[[612, 792]], "Cached text"), None).unwrap();
    assert_eq!(doc.page_text_layer(1).err().unwrap().code, ErrorCode::OutOfRange);
    assert_eq!(doc.find_text(1, "x", false, false).err().unwrap().code, ErrorCode::OutOfRange);
    let first = doc.page_text(0).unwrap();
    let layer = doc.page_text_layer(0).unwrap();
    assert_eq!(doc.page_text(0).unwrap(), first, "the cached text is the same text");
    assert_eq!(layer.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n").trim(), first.trim());
}

// ── Password protection (Ubu Office Phase 2b, E1; decisions D11 and D12) ─────────────────────

/// Fixed entropy, so the tests are deterministic; the browser binding uses getrandom.
const RANDOM: [u8; 64] = [7; 64];
const NOTHING: Allow = Allow { print: false, copy: false, modify: false };

fn page_texts(doc: &mut Document) -> Vec<String> {
    (0..doc.page_count()).map(|i| doc.page_text(i).unwrap().trim().to_owned()).collect()
}

#[test]
fn a_plain_document_is_unprotected_and_may_be_protected() {
    let doc = Document::open(pdf(&[[300, 300]], "Plain"), None).unwrap();
    let p = doc.protection().unwrap();
    assert!(!p.encrypted && p.opened_as_owner && !p.restricted && p.can_change());
    assert_eq!(p.allow, Allow::ALL);
}

#[test]
fn protect_then_reopen_needs_the_open_password() {
    let mut original = Document::open(pdf(&[[300, 300], [300, 300]], "Secret"), None).unwrap();
    let protected = original.save_protected("open-pw", None, Allow::ALL, &RANDOM).unwrap();
    assert!(protected.starts_with(b"%PDF-"));
    assert!(protected.windows(5).any(|w| w == b"AESV3"), "AES-256 (PDF 2.0 crypt filter)");

    assert_eq!(Document::open(protected.clone(), None).err().unwrap().code, ErrorCode::NeedsPassword);
    assert_eq!(Document::open(protected.clone(), Some("nope")).err().unwrap().code, ErrorCode::WrongPassword);
    let mut reopened = Document::open(protected, Some("open-pw")).unwrap();
    assert_eq!(page_texts(&mut reopened), page_texts(&mut original), "same pages, same text");

    // No restrictions and no permissions password: the open password is also the owner password.
    let p = reopened.protection().unwrap();
    assert!(p.encrypted && p.opened_as_owner && !p.restricted && p.can_change());
}

#[test]
fn restrictions_without_a_permissions_password_cannot_be_lifted() {
    let doc = Document::open(pdf(&[[300, 300]], "Locked"), None).unwrap();
    let protected = doc.save_protected("open-pw", None, NOTHING, &RANDOM).unwrap();
    let reopened = Document::open(protected.clone(), Some("open-pw")).unwrap();
    let p = reopened.protection().unwrap();
    assert!(p.encrypted && !p.opened_as_owner && p.restricted && !p.can_change());
    assert_eq!(p.allow, NOTHING);

    // D11: the open password alone never strips the restrictions, by unlocking or re-protecting.
    assert_eq!(reopened.save_unprotected().err().unwrap().code, ErrorCode::OwnerPasswordRequired);
    assert_eq!(reopened.save_protected("other", None, Allow::ALL, &RANDOM).err().unwrap().code, ErrorCode::OwnerPasswordRequired);

    // D12: the random owner password is the hex of the second half of the entropy, never shown;
    // nothing else opens the file as owner.
    assert_eq!(Document::open(protected, Some("owner")).err().unwrap().code, ErrorCode::WrongPassword);
}

#[test]
fn the_permissions_password_lifts_restrictions_and_removes_protection() {
    let mut original = Document::open(pdf(&[[300, 300], [300, 300]], "Owned"), None).unwrap();
    let allow = Allow { print: true, copy: false, modify: false };
    let protected = original.save_protected("open-pw", Some("owner-pw"), allow, &RANDOM).unwrap();

    let as_user = Document::open(protected.clone(), Some("open-pw")).unwrap();
    let p = as_user.protection().unwrap();
    assert!(p.restricted && !p.opened_as_owner && !p.can_change());
    assert_eq!(p.allow, allow);

    let as_owner = Document::open(protected, Some("owner-pw")).unwrap();
    let p = as_owner.protection().unwrap();
    assert!(p.encrypted && p.opened_as_owner && p.restricted && p.can_change());
    assert_eq!(p.allow, Allow::ALL, "the owner may do everything");

    let unprotected = as_owner.save_unprotected().unwrap();
    let mut plain = Document::open(unprotected, None).unwrap();
    assert!(!plain.protection().unwrap().encrypted);
    assert_eq!(page_texts(&mut plain), page_texts(&mut original));
}

#[test]
fn an_unrestricted_file_opened_with_its_open_password_may_be_unprotected() {
    let doc = Document::open(pdf(&[[300, 300]], "Open"), None).unwrap();
    let protected = doc.save_protected("open-pw", Some("owner-pw"), Allow::ALL, &RANDOM).unwrap();
    let as_user = Document::open(protected, Some("open-pw")).unwrap();
    let p = as_user.protection().unwrap();
    assert!(!p.opened_as_owner && !p.restricted && p.can_change());
    let mut plain = Document::open(as_user.save_unprotected().unwrap(), None).unwrap();
    assert_eq!(page_texts(&mut plain), vec!["Open 1".to_owned()]);
}

#[test]
fn password_rules_are_enforced() {
    let doc = Document::open(pdf(&[[300, 300]], "Rules"), None).unwrap();
    assert_eq!(doc.save_protected("", None, Allow::ALL, &RANDOM).err().unwrap().code, ErrorCode::InvalidPassword);
    assert_eq!(doc.save_protected("same", Some("same"), NOTHING, &RANDOM).err().unwrap().code, ErrorCode::InvalidPassword);
    // An empty permissions password means "none given".
    assert!(doc.save_protected("open-pw", Some(""), NOTHING, &RANDOM).is_ok());
}

#[test]
fn allow_flags_map_to_the_p_bits() {
    let bit = |p: i32, n: u32| (p >> (n - 1)) & 1 == 1;
    assert_eq!(Allow::ALL.p_bits(), -1);
    let p = Allow { print: false, ..Allow::ALL }.p_bits();
    assert!(!bit(p, 3) && !bit(p, 12) && bit(p, 4) && bit(p, 5));
    let p = Allow { copy: false, ..Allow::ALL }.p_bits();
    assert!(!bit(p, 5) && bit(p, 10), "copying denied, accessibility extraction kept");
    let p = Allow { modify: false, ..Allow::ALL }.p_bits();
    assert!(!bit(p, 4) && !bit(p, 6) && !bit(p, 9) && !bit(p, 11) && bit(p, 3) && bit(p, 5));
}

#[test]
fn error_codes_for_protection_are_part_of_the_contract() {
    assert_eq!(ErrorCode::InvalidPassword.code(), "invalid_password");
    assert_eq!(ErrorCode::OwnerPasswordRequired.code(), "owner_password_required");
}
