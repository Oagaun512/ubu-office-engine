//! Write a small two-page DOCX for the browser-build smoke test (scripts/docx-worker-smoke.mjs):
//! `cargo run -p ubu-docx --example make_sample -- out.docx`. Written with wordcraft's own
//! session, so no fixture file is kept in the repository.

use serde_json::json;
use wordcraft_engine::{Region, Session, blank_document};

fn main() {
    let out = std::env::args().nth(1).expect("usage: make_sample <out.docx>");
    let mut s = Session::new(blank_document(&Region::default()));
    s.run("text.insert", &json!({ "text": "Hello Ubu 1" })).unwrap();
    s.run("insert.pageBreak", &json!({})).unwrap();
    s.run("text.insert", &json!({ "text": "Hello Ubu 2" })).unwrap();
    std::fs::write(&out, wordcraft_engine::io::save_bytes("sample.docx", &s.doc).unwrap()).unwrap();
}
