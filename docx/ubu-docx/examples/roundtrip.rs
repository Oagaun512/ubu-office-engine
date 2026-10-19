//! Round trip of one document through the DOCX engine API, for the corpus run (Ubu Office
//! Phase 3, D4): open → render page 1 → text layer and a search on page 1 → export PDF →
//! save DOCX → reopen the copy → same page count, same text on every page.
//!
//!   cargo run --example roundtrip -- <file.docx>
//!
//! Prints one JSON line. `scripts/roundtrip-docx.mjs` runs it per file under a timeout (a hang
//! or a crash is a result too) and writes ROUNDTRIP.md. The `lost` list names the kinds of
//! package part (charts, diagrams, embedded objects, media) the original has and the saved copy
//! does not: what decision D15's content-loss check must warn about before a download.

use std::time::Instant;

use ubu_docx::Document;

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

/// The entry names of a ZIP's central directory (no decompression: names only). Empty when the
/// bytes are not a ZIP.
fn zip_names(bytes: &[u8]) -> Vec<String> {
    let u16_at = |i: usize| bytes.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |i: usize| bytes.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let Some(eocd) = (0..bytes.len().saturating_sub(21)).rev().take(22 + 0xffff).find(|&i| u32_at(i) == Some(0x0605_4b50)) else {
        return Vec::new();
    };
    let (Some(count), Some(mut at)) = (u16_at(eocd + 10), u32_at(eocd + 16)) else { return Vec::new() };
    let mut names = Vec::new();
    for _ in 0..count {
        if u32_at(at) != Some(0x0201_4b50) {
            break;
        }
        let (Some(n), Some(x), Some(c)) = (u16_at(at + 28), u16_at(at + 30), u16_at(at + 32)) else { break };
        let Some(name) = bytes.get(at + 46..at + 46 + n) else { break };
        names.push(String::from_utf8_lossy(name).into_owned());
        at += 46 + n + x + c;
    }
    names
}

/// The kinds of rich content a DOCX package can hold, by part name.
const KINDS: [(&str, &str); 5] = [
    ("charts", "word/charts/"),
    ("diagrams", "word/diagrams/"),
    ("embedded objects", "word/embeddings/"),
    ("media", "word/media/"),
    ("comments", "word/comments.xml"),
];

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: roundtrip <file.docx>");
        std::process::exit(2);
    };
    let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let start = Instant::now();
    let report = |stage: &str, pages: usize, open_ms: u128, extra: &str, error: Option<String>| {
        println!(
            "{{\"stage\":{},\"ok\":{},\"pages\":{},\"openMs\":{},\"ms\":{}{},\"error\":{}}}",
            json_str(stage),
            error.is_none(),
            pages,
            open_ms,
            start.elapsed().as_millis(),
            extra,
            error.map(|e| json_str(&e)).unwrap_or_else(|| "null".into())
        );
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return report("read", 0, 0, "", Some(e.to_string())),
    };
    let doc = match Document::open(&bytes, &name, None) {
        Ok(d) => d,
        Err(e) => return report("open", 0, start.elapsed().as_millis(), "", Some(e.to_string())),
    };
    let open_ms = start.elapsed().as_millis();
    let pages = doc.page_count();
    if pages > 0 {
        if let Err(e) = doc.render_page(0, 1.0) {
            return report("render", pages, open_ms, "", Some(e.to_string()));
        }
        if let Err(e) = doc.page_text_layer(0) {
            return report("textlayer", pages, open_ms, "", Some(e.to_string()));
        }
        if let Err(e) = doc.find_text(0, "the", false, false) {
            return report("find", pages, open_ms, "", Some(e.to_string()));
        }
    }
    let texts: Vec<String> = (0..pages).map(|p| doc.page_text(p).unwrap_or_default()).collect();
    if let Err(e) = doc.export_pdf() {
        return report("pdf", pages, open_ms, "", Some(e.to_string()));
    }
    let saved = match doc.save_docx() {
        Ok(s) => s,
        Err(e) => return report("save", pages, open_ms, "", Some(e.to_string())),
    };
    // What the original holds that the copy lost (D15).
    let (before, after) = (zip_names(&bytes), zip_names(&saved));
    let lost: Vec<String> = KINDS
        .iter()
        .filter(|(_, prefix)| before.iter().any(|n| n.starts_with(prefix)) && !after.iter().any(|n| n.starts_with(prefix)))
        .map(|(kind, _)| json_str(kind))
        .collect();
    let lost = format!(",\"lost\":[{}]", lost.join(","));
    let again = match Document::open(&saved, "copy.docx", None) {
        Ok(d) => d,
        Err(e) => return report("reopen", pages, open_ms, &lost, Some(e.to_string())),
    };
    let pages_after = again.page_count();
    let text_same = pages_after == pages && (0..pages).all(|p| again.page_text(p).unwrap_or_default() == texts[p]);
    let extra = format!("{lost},\"pagesAfter\":{pages_after},\"textSame\":{text_same}");
    report("done", pages, open_ms, &extra, None);
}
