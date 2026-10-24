//! Native tests of editing (E1): page operations and comments, each saved, reopened and checked.

use std::sync::Arc;

use ubu_pdf::{Document, ErrorCode};

const DATE: &str = "D:20261024120000Z";

/// A valid PDF with one page per entry of `pages`, each `[w, h]` points with `/Rotate rotate`,
/// showing "`label` n" (n = 1-based page number) in Helvetica 40 pt below the top.
fn pdf(pages: &[[u32; 2]], label: &str, rotate: u32) -> Vec<u8> {
    let n = pages.len();
    let mut objects: Vec<String> = Vec::new();
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    let kids = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect::<Vec<_>>().join(" ");
    objects.push(format!("<< /Type /Pages /Kids [{kids}] /Count {n} >>"));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for (i, [w, h]) in pages.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Rotate {rotate} /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            5 + 2 * i
        ));
        let stream = format!("BT /F1 24 Tf 20 {} Td ({label} {}) Tj ET", h.saturating_sub(40), i + 1);
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

fn texts(bytes: &[u8]) -> Vec<String> {
    let mut d = Document::open(bytes.to_vec(), None).expect("the saved file reopens");
    (0..d.page_count()).map(|i| d.page_text(i).expect("text").trim().to_owned()).collect()
}

fn inspect(bytes: &[u8]) -> pdfcraft_render::DocInfo {
    pdfcraft_render::inspect(Arc::new(bytes.to_vec()), None).expect("the saved file inspects")
}

#[test]
fn edit_failed_is_a_contract_string() {
    assert_eq!(ErrorCode::EditFailed.code(), "edit_failed");
}

#[test]
fn reorder_rotate_and_delete_round_trip_as_an_incremental_update() {
    let original = pdf(&[[612, 792]; 3], "Page", 0);
    let doc = Document::open(original.clone(), None).unwrap();
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    ed.move_pages(&[2], 0).unwrap(); // 3 1 2
    ed.rotate_pages(&[0], 90).unwrap(); // page "3" turned a quarter
    ed.delete_pages(&[1]).unwrap(); // 3 2
    assert_eq!(ed.page_count(), 2);
    let saved = ed.save().unwrap();
    assert!(saved.starts_with(&original), "the original bytes are kept and the edits appended");
    assert_eq!(texts(&saved), ["Page 3", "Page 2"]);
    let reopened = Document::open(saved.clone(), None).unwrap();
    let turned = reopened.page_size(0).unwrap();
    assert_eq!((turned.width_pt, turned.height_pt), (792.0, 612.0), "the rotation is in the file");
    let upright = reopened.page_size(1).unwrap();
    assert_eq!((upright.width_pt, upright.height_pt), (612.0, 792.0));
    let info = inspect(&saved);
    assert_eq!(info.pages.iter().map(|p| p.rotation).collect::<Vec<_>>(), [90, 0]);
}

#[test]
fn rotation_accumulates_and_can_be_negative() {
    let doc = Document::open(pdf(&[[612, 792]], "Page", 90), None).unwrap();
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    ed.rotate_pages(&[0], -90).unwrap();
    assert_eq!(inspect(&ed.save().unwrap()).pages[0].rotation, 0);
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    assert_eq!(ed.rotate_pages(&[0], 45).unwrap_err().code, ErrorCode::EditFailed);
}

#[test]
fn deleting_every_page_or_a_missing_page_is_refused() {
    let doc = Document::open(pdf(&[[612, 792]; 2], "Page", 0), None).unwrap();
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    assert_eq!(ed.delete_pages(&[0, 1]).unwrap_err().code, ErrorCode::EditFailed);
    assert_eq!(ed.delete_pages(&[7]).unwrap_err().code, ErrorCode::EditFailed);
    assert_eq!(ed.page_count(), 2, "a refused edit changes nothing");
}

#[test]
fn insert_pages_from_another_file_all_or_some() {
    let a = pdf(&[[612, 792]; 2], "A", 0);
    let b = pdf(&[[595, 842]; 3], "B", 0);
    let doc = Document::open(a.clone(), None).unwrap();
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    assert_eq!(ed.insert_pages(b.clone(), None, "b.pdf", &[], 1).unwrap(), 3);
    assert_eq!(ed.insert_pages(b.clone(), None, "b.pdf", &[2], 99).unwrap(), 1, "past the end appends");
    let saved = ed.save().unwrap();
    assert!(saved.starts_with(&a));
    assert_eq!(texts(&saved), ["A 1", "B 1", "B 2", "B 3", "A 2", "B 3"]);
    let sizes = Document::open(saved, None).unwrap();
    let s = sizes.page_size(1).unwrap();
    assert_eq!((s.width_pt, s.height_pt), (595.0, 842.0), "an inserted page keeps its own size");
}

#[test]
fn inserting_something_that_is_not_a_pdf_is_refused_with_its_open_code() {
    let doc = Document::open(pdf(&[[612, 792]], "A", 0), None).unwrap();
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    assert_eq!(ed.insert_pages(b"not a pdf".to_vec(), None, "x.pdf", &[], 0).unwrap_err().code, ErrorCode::Invalid);
}

#[test]
fn extract_writes_a_new_file_of_the_chosen_pages() {
    let doc = Document::open(pdf(&[[612, 792]; 4], "Page", 0), None).unwrap();
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    ed.set_info("Producer", "Ubu PDF").unwrap();
    let out = ed.extract(&[3, 1]).unwrap();
    assert_eq!(texts(&out), ["Page 2", "Page 4"], "extracted pages keep document order");
    assert_eq!(inspect(&out).producer.as_deref(), Some("Ubu PDF"));
    assert_eq!(ed.extract(&[]).unwrap_err().code, ErrorCode::EditFailed);
}

#[test]
fn comments_are_real_annotations_and_the_text_stays() {
    let original = pdf(&[[612, 792]], "Hello world", 0);
    let mut doc = Document::open(original.clone(), None).unwrap();
    let line = doc.page_text_layer(0).unwrap().remove(0);
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    ed.markup(0, "highlight", &line.rect, &[1.0, 0.9, 0.0], 0.5, "", "Ada").unwrap();
    ed.markup(0, "underline", &line.rect, &[], 1.0, "", "Ada").unwrap();
    ed.markup(0, "strikeout", &line.rect, &[], 1.0, "", "Ada").unwrap();
    ed.note(0, 300.0, 100.0, "Check this", "Ada", &[]).unwrap();
    ed.text_box(0, &[100.0, 200.0, 300.0, 240.0], 14.0, "Typed note", "Ada", &[0.0, 0.0, 0.0]).unwrap();
    ed.ink(0, &[100.0, 300.0, 150.0, 320.0, 200.0, 300.0, 120.0, 400.0, 180.0, 400.0], &[3, 2], 2.0, &[0.0, 0.3, 0.9], 1.0, "Ada").unwrap();
    ed.set_info("Producer", "Ubu PDF").unwrap();
    let saved = ed.save().unwrap();
    assert!(saved.starts_with(&original), "comments are appended, not rewritten");

    let info = inspect(&saved);
    let kinds: Vec<&str> = info.annotations.iter().map(|a| a.subtype.as_str()).filter(|s| *s != "Popup").collect();
    assert_eq!(kinds, ["Highlight", "Underline", "StrikeOut", "Text", "FreeText", "Ink"]);
    for a in info.annotations.iter().filter(|a| a.subtype != "Popup") {
        assert_eq!(a.page, 0);
        assert_eq!(a.author.as_deref(), Some("Ada"));
        assert!(a.name.as_deref().is_some_and(|n| n.starts_with("t-")), "each comment has a unique name");
    }
    let highlight = &info.annotations[0];
    assert_eq!(highlight.quads.len(), 1);
    // The line sits 40 pt below the top of a 792 pt page: its highlight is there in user space.
    assert!(highlight.rect[3] > 740.0 && highlight.rect[1] < 760.0, "{:?}", highlight.rect);
    let note = info.annotations.iter().find(|a| a.subtype == "Text").unwrap();
    assert_eq!(note.contents.as_deref(), Some("Check this"));
    let text_box = info.annotations.iter().find(|a| a.subtype == "FreeText").unwrap();
    assert_eq!(text_box.contents.as_deref(), Some("Typed note"));
    assert_eq!(info.producer.as_deref(), Some("Ubu PDF"));

    let mut reopened = Document::open(saved, None).unwrap();
    assert!(reopened.page_text(0).unwrap().contains("Hello world 1"), "the page text is still extractable");
    // The comments are drawn: rendering still works with their appearance streams.
    assert!(reopened.render_page(0, 0.5).is_ok());
}

#[test]
fn a_note_on_a_rotated_page_lands_where_it_was_placed_on_screen() {
    let doc = Document::open(pdf(&[[612, 792]], "Turned", 90), None).unwrap();
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    ed.note(0, 50.0, 60.0, "here", "Ada", &[]).unwrap();
    let info = inspect(&ed.save().unwrap());
    let page = &info.pages[0];
    let note = info.annotations.iter().find(|a| a.subtype == "Text").unwrap();
    let [x0, y0, x1, y1] = note.rect;
    let corners = [page.user_to_view(x0, y0), page.user_to_view(x1, y1)];
    let left = corners[0][0].min(corners[1][0]);
    let top = corners[0][1].min(corners[1][1]);
    assert!((left - 50.0).abs() < 1.0 && (top - 60.0).abs() < 1.0, "displayed top-left {left}, {top}");
}

#[test]
fn comments_after_a_page_operation_in_the_same_edit_are_refused() {
    let doc = Document::open(pdf(&[[612, 792]; 2], "Page", 0), None).unwrap();
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    ed.rotate_pages(&[1], 90).unwrap();
    assert_eq!(ed.note(0, 10.0, 10.0, "x", "Ada", &[]).unwrap_err().code, ErrorCode::EditFailed);
}

#[test]
fn bad_comment_input_is_refused() {
    let doc = Document::open(pdf(&[[612, 792]], "Page", 0), None).unwrap();
    let mut ed = doc.edit(Some(DATE), "t").unwrap();
    assert_eq!(ed.note(3, 10.0, 10.0, "x", "Ada", &[]).unwrap_err().code, ErrorCode::OutOfRange);
    assert_eq!(ed.markup(0, "sparkle", &[0.0, 0.0, 10.0, 10.0], &[], 1.0, "", "Ada").unwrap_err().code, ErrorCode::EditFailed);
    assert_eq!(ed.markup(0, "highlight", &[0.0, 0.0, 10.0], &[], 1.0, "", "Ada").unwrap_err().code, ErrorCode::EditFailed);
    assert_eq!(ed.text_box(0, &[0.0, 0.0], 12.0, "x", "Ada", &[]).unwrap_err().code, ErrorCode::EditFailed);
    assert_eq!(ed.ink(0, &[0.0, 0.0, 1.0, 1.0], &[3], 1.0, &[], 1.0, "Ada").unwrap_err().code, ErrorCode::EditFailed);
    assert_eq!(ed.note(0, 10.0, 10.0, "x", "Ada", &[1.0, 0.0]).unwrap_err().code, ErrorCode::EditFailed);
}

#[test]
fn a_restricted_file_opened_with_its_open_password_cannot_be_edited() {
    let doc = Document::open(pdf(&[[612, 792]], "Page", 0), None).unwrap();
    let locked = doc.save_protected("open", None, ubu_pdf::Allow { print: true, copy: true, modify: false }, &[7; 64]).unwrap();
    let restricted = Document::open(locked, Some("open")).unwrap();
    assert_eq!(restricted.edit(Some(DATE), "t").err().map(|e| e.code), Some(ErrorCode::OwnerPasswordRequired));
}
