//! Native tests of the Ubu Slides engine API (Phase 5, P1). Presentations are made with the
//! upstream engine itself and saved as PPTX bytes, so every test also goes through the reader.

use std::sync::Arc;

use deckcraft_engine::Session;
use deckcraft_engine::cmd::file;
use deckcraft_model::text::TextBody;
use serde_json::{Value, json};
use ubu_slides::{Deck, ErrorCode};

/// A new presentation (one title slide), the commands run on it, the first slide's notes set,
/// saved as PPTX.
fn pptx(commands: &[(&str, Value)], notes: Option<&str>) -> Vec<u8> {
    let mut s = Session::with_new();
    for (id, params) in commands {
        s.execute(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    if let Some(n) = notes {
        s.edit(|doc, _| {
            Arc::make_mut(&mut doc.slides[0]).notes = TextBody::from_text(n);
            Ok(())
        })
        .unwrap();
    }
    file::save_bytes(&s.active().unwrap().doc, "pptx").unwrap()
}

fn text_box(text: &str) -> (&'static str, Value) {
    ("insert.textBox", json!({"rect": [100, 100, 500, 80], "text": text}))
}

fn json(s: &str) -> Value {
    serde_json::from_str(s).unwrap()
}

#[test]
fn a_presentation_opens_with_its_size_slides_and_notes() {
    let bytes = pptx(&[text_box("Hello Ubu Slides"), ("slide.new", json!({"layout": "blank"}))], Some("Say hello"));
    let deck = Deck::open(&bytes, "t.pptx").unwrap();
    let info = json(&deck.info());
    assert_eq!(info["slides"], 2);
    assert!(info["width"].as_f64().unwrap() > info["height"].as_f64().unwrap(), "a wide slide: {info}");
    let slides = json(&deck.slides());
    assert_eq!(slides.as_array().unwrap().len(), 2);
    assert_eq!(slides[0]["notes"], true);
    assert_eq!(slides[1]["notes"], false);
    assert_eq!(deck.notes(0).unwrap(), "Say hello");
    assert!(deck.slide_text(0).unwrap().contains("Hello Ubu Slides"));
    assert!(deck.warnings().is_empty(), "{:?}", deck.warnings());
    assert!(!deck.is_dirty() && !deck.can_undo() && !deck.can_redo());
}

#[test]
fn a_slide_renders_with_ink_at_the_scale_asked() {
    let bytes = pptx(&[text_box("Ink")], None);
    let deck = Deck::open(&bytes, "t.pptx").unwrap();
    let info = json(&deck.info());
    let img = deck.render(0, 0.5).unwrap();
    assert_eq!(img.width, (info["width"].as_f64().unwrap() * 0.5).round() as u32);
    assert_eq!(img.pixels.len(), (img.width * img.height * 4) as usize);
    let first = &img.pixels[..4];
    assert!(img.pixels.chunks(4).any(|p| p != first), "something is drawn on the slide");
    assert_eq!(deck.render(5, 1.0).unwrap_err().code, ErrorCode::OutOfRange);
    assert_eq!(deck.render(0, 0.0).unwrap_err().code, ErrorCode::OutOfRange);
}

#[test]
fn the_text_layer_puts_each_run_on_its_baseline_in_slide_points() {
    let bytes = pptx(&[text_box("Hello Ubu Slides")], None);
    let deck = Deck::open(&bytes, "t.pptx").unwrap();
    let spans = json(&deck.text_layer(0).unwrap());
    let span = spans.as_array().unwrap().iter().find(|s| s["text"].as_str().unwrap().contains("Hello")).unwrap_or_else(|| panic!("{spans}"));
    let (x, y, x2, y2) = (span["x"].as_f64().unwrap(), span["y"].as_f64().unwrap(), span["x2"].as_f64().unwrap(), span["y2"].as_f64().unwrap());
    assert!((100.0..600.0).contains(&x) && (100.0..180.0).contains(&y), "inside the text box: {span}");
    assert!(x2 > x && (y2 - y).abs() < 0.01, "a level baseline running right: {span}");
    assert!(span["size"].as_f64().unwrap() > 4.0);
}

#[test]
fn shapes_come_with_their_boxes() {
    let bytes = pptx(&[text_box("Box")], None);
    let deck = Deck::open(&bytes, "t.pptx").unwrap();
    let shapes = json(&deck.shapes(0).unwrap());
    let b =
        shapes.as_array().unwrap().iter().map(|s| s["box"].clone()).find(|b| b[0] == 100.0 && b[1] == 100.0).unwrap_or_else(|| panic!("{shapes}"));
    assert_eq!(b[2], 600.0);
}

#[test]
fn edits_run_through_the_engine_with_undo() {
    let bytes = pptx(&[], None);
    let mut deck = Deck::open(&bytes, "t.pptx").unwrap();
    let r = deck.run("slide.new", r#"{"layout": "blank"}"#).unwrap();
    assert!(r.changed && r.slides == 2 && r.slide == 1, "{r:?}");
    assert!(deck.is_dirty() && deck.can_undo());
    let r = deck.run("edit.undo", "").unwrap();
    assert!(r.changed && r.slides == 1, "{r:?}");
    assert!(deck.can_redo());
    assert_eq!(json(&deck.selection())["slide"], 0);
    let bad = deck.run("no.suchCommand", "").unwrap_err();
    assert_eq!(bad.code, ErrorCode::InvalidCommand);
    for refused in ["file.open", "file.saveAs", "insert.video", "media.play", "transition.sound"] {
        assert_eq!(deck.run(refused, "{}").unwrap_err().code, ErrorCode::InvalidCommand, "{refused}");
    }
    assert_eq!(deck.run("slide.new", "{not json").unwrap_err().code, ErrorCode::InvalidCommand);
}

#[test]
fn a_saved_copy_opens_with_the_same_slides_and_text() {
    let bytes = pptx(&[text_box("Round trip")], Some("Notes survive"));
    let mut deck = Deck::open(&bytes, "t.pptx").unwrap();
    deck.run("slide.duplicate", "{}").unwrap();
    let copy = Deck::open(&deck.save_pptx().unwrap(), "copy.pptx").unwrap();
    assert_eq!(json(&copy.info())["slides"], 2);
    assert!(copy.slide_text(1).unwrap().contains("Round trip"));
    assert_eq!(copy.notes(0).unwrap(), "Notes survive");
}

#[test]
fn pdf_export_writes_one_page_per_slide() {
    let bytes = pptx(&[text_box("PDF"), ("slide.new", json!({"layout": "blank"}))], None);
    let pdf = Deck::open(&bytes, "t.pptx").unwrap().export_pdf().unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    let pages = pdf.windows(10).filter(|w| w == b"/Type /Pag" || w == b"/Type/Page").count();
    assert!(pages >= 2, "two pages, counted {pages}");
}

#[test]
fn what_cannot_be_opened_is_refused_in_words() {
    let text = Deck::open(b"just some text", "notes.pptx").unwrap_err();
    assert_eq!(text.code, ErrorCode::Invalid);
    let mut ppt = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    ppt.resize(4096, 0);
    let old = Deck::open(&ppt, "old.ppt").unwrap_err();
    assert_eq!(old.code, ErrorCode::Unsupported);
    assert!(old.message.contains(".ppt"), "{}", old.message);
    let zip_without_parts = Deck::open(b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0", "empty.pptx").unwrap_err();
    assert_eq!(zip_without_parts.code, ErrorCode::Invalid);
}

#[test]
fn fonts_used_are_listed_for_the_page_to_load() {
    let bytes = pptx(&[text_box("Fonts")], None);
    let deck = Deck::open(&bytes, "t.pptx").unwrap();
    let info = json(&deck.info());
    assert!(!info["fonts"].as_array().unwrap().is_empty(), "{info}");
    assert!(info["missingFonts"].is_array());
}
