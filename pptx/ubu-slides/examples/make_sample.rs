//! Write a small presentation for the browser-build smoke test (scripts/pptx-worker-smoke.mjs):
//! `cargo run -p ubu-slides --example make_sample -- out.pptx`. Written with deckcraft's own
//! session, so no fixture file is kept in the repository: a text box on the first slide, speaker
//! notes, and a second, blank slide.

use std::sync::Arc;

use deckcraft_engine::Session;
use deckcraft_engine::cmd::file;
use deckcraft_model::text::TextBody;
use serde_json::json;

fn main() {
    let out = std::env::args().nth(1).expect("usage: make_sample <out.pptx>");
    let mut s = Session::with_new();
    let steps = [
        ("insert.textBox", json!({"rect": [100, 100, 500, 80], "text": "Ubu Slides"})),
        ("slide.new", json!({"layout": "blank"})),
        ("slide.go", json!({"index": 0})),
    ];
    for (id, params) in steps {
        s.execute(id, &params).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    s.edit(|doc, _| {
        Arc::make_mut(&mut doc.slides[0]).notes = TextBody::from_text("Speaker notes");
        Ok(())
    })
    .unwrap();
    std::fs::write(&out, file::save_bytes(&s.active().unwrap().doc, "pptx").unwrap()).unwrap();
}
