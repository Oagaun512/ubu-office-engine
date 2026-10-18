//! Round trip of one PDF through the engine API, for the corpus run:
//! open → render page 1 → text of page 1 → save → reopen → same page count.
//!
//!   cargo run --example roundtrip -- <file.pdf> [password]
//!
//! Prints one JSON line. `scripts/roundtrip.mjs` runs it per file under a timeout (a hang or a
//! crash is a result too) and writes ROUNDTRIP.md.

use std::time::Instant;

use ubu_pdf::Document;

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: roundtrip <file.pdf> [password]");
        std::process::exit(2);
    };
    let password = args.next();
    let start = Instant::now();
    let report = |stage: &str, pages: usize, error: Option<String>| {
        println!(
            "{{\"stage\":{},\"ok\":{},\"pages\":{},\"ms\":{},\"error\":{}}}",
            json_str(stage),
            error.is_none(),
            pages,
            start.elapsed().as_millis(),
            error.map(|e| json_str(&e)).unwrap_or_else(|| "null".into())
        );
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return report("read", 0, Some(e.to_string())),
    };
    let mut doc = match Document::open(bytes, password.as_deref()) {
        Ok(d) => d,
        Err(e) => return report("open", 0, Some(e.to_string())),
    };
    let pages = doc.page_count();
    if pages > 0 {
        if let Err(e) = doc.render_page(0, 1.0) {
            return report("render", pages, Some(e.to_string()));
        }
        if let Err(e) = doc.page_text(0) {
            return report("text", pages, Some(e.to_string()));
        }
    }
    let saved = match doc.save() {
        Ok(s) => s,
        Err(e) => return report("save", pages, Some(e.to_string())),
    };
    match Document::open(saved, password.as_deref()) {
        Ok(again) if again.page_count() == pages => report("done", pages, None),
        Ok(again) => report("reopen", pages, Some(format!("page count changed from {pages} to {}", again.page_count()))),
        Err(e) => report("reopen", pages, Some(e.to_string())),
    }
}
