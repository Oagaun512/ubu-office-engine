//! Round trip of one presentation through the presentation engine API, for the corpus run (Ubu
//! Office Phase 5, P2): open → every slide rendered (at 0.5 px per point), its text layer,
//! shapes, text and notes → export PDF → save PPTX → reopen the copy → same slides, and each
//! slide's text, notes and shape count the same.
//!
//!   cargo run --example roundtrip -- <file.pptx>
//!
//! Prints one JSON line. `scripts/roundtrip-pptx.mjs` runs it per file under a timeout (a hang
//! or a crash is a result too) and writes ROUNDTRIP.md. The `lost` list names the kinds of
//! package part the original has and the saved copy does not (charts, diagrams, media, macros…):
//! what the P8 save check must warn about (decision D28).

use std::time::Instant;

use ubu_slides::Deck;

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

/// The entry names of a ZIP's central directory (names only). Empty when the bytes are not a ZIP.
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

/// The kinds of rich content a PowerPoint package can hold, by part name.
const KINDS: [(&str, &str); 8] = [
    ("charts", "ppt/charts/"),
    ("diagrams (SmartArt)", "ppt/diagrams/"),
    ("media and pictures", "ppt/media/"),
    ("embedded objects", "ppt/embeddings/"),
    ("macros", "ppt/vbaProject.bin"),
    ("comments", "ppt/comments/"),
    ("ink", "ppt/ink/"),
    ("embedded fonts", "ppt/fonts/"),
];

/// What one slide reads as, for comparing a presentation with its saved copy.
#[derive(PartialEq)]
struct SlideRead {
    text: String,
    notes: String,
    shapes: usize,
}

fn read_slide(deck: &Deck, i: usize) -> Result<SlideRead, String> {
    let shapes: Vec<serde_json::Value> = serde_json::from_str(&deck.shapes(i).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(SlideRead { text: deck.slide_text(i).map_err(|e| e.to_string())?, notes: deck.notes(i).map_err(|e| e.to_string())?, shapes: shapes.len() })
}

fn slide_count(deck: &Deck) -> usize {
    serde_json::from_str::<serde_json::Value>(&deck.info()).ok().and_then(|v| v["slides"].as_u64()).unwrap_or(0) as usize
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: roundtrip <file.pptx>");
        std::process::exit(2);
    };
    let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let start = Instant::now();
    let report = |stage: &str, slides: usize, open_ms: u128, extra: &str, error: Option<String>| {
        println!(
            "{{\"stage\":{},\"ok\":{},\"slides\":{},\"openMs\":{},\"ms\":{}{},\"error\":{}}}",
            json_str(stage),
            error.is_none(),
            slides,
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
    let deck = match Deck::open(&bytes, &name) {
        Ok(d) => d,
        // A refusal says which kind: `unsupported` (old .ppt, password-protected) or `invalid`.
        Err(e) => return report("open", 0, start.elapsed().as_millis(), &format!(",\"code\":{}", json_str(e.code.code())), Some(e.message)),
    };
    let open_ms = start.elapsed().as_millis();
    let n = slide_count(&deck);
    let warnings = format!(",\"warnings\":{}", deck.warnings().len());
    // What the viewer does on opening each slide: render it, its text layer, shapes, text, notes.
    let mut before = Vec::with_capacity(n);
    let mut blank = 0usize;
    let mut render_ms = 0u128;
    for i in 0..n {
        let t = Instant::now();
        let img = match deck.render(i, 0.5, false) {
            Ok(img) => img,
            Err(e) => return report("render", n, open_ms, &warnings, Some(format!("slide {}: {e}", i + 1))),
        };
        render_ms += t.elapsed().as_millis();
        let first = img.pixels.get(..4).unwrap_or(&[]);
        if img.pixels.chunks(4).all(|p| p == first) {
            blank += 1;
        }
        if let Err(e) = deck.text_layer(i) {
            return report("text", n, open_ms, &warnings, Some(format!("slide {}: {e}", i + 1)));
        }
        match read_slide(&deck, i) {
            Ok(r) => before.push(r),
            Err(e) => return report("text", n, open_ms, &warnings, Some(format!("slide {}: {e}", i + 1))),
        }
    }
    let viewed = format!("{warnings},\"blank\":{blank},\"renderMs\":{render_ms}");
    if let Err(e) = deck.export_pdf() {
        return report("pdf", n, open_ms, &viewed, Some(e.to_string()));
    }
    let saved = match deck.save_pptx() {
        Ok(s) => s,
        Err(e) => return report("save", n, open_ms, &viewed, Some(e.to_string())),
    };
    let (orig, copy) = (zip_names(&bytes), zip_names(&saved));
    let lost: Vec<String> = KINDS
        .iter()
        .filter(|(_, prefix)| orig.iter().any(|x| x.starts_with(prefix)) && !copy.iter().any(|x| x.starts_with(prefix)))
        .map(|(kind, _)| json_str(kind))
        .collect();
    let lost = format!("{viewed},\"lost\":[{}]", lost.join(","));
    let again = match Deck::open(&saved, "copy.pptx") {
        Ok(d) => d,
        Err(e) => return report("reopen", n, open_ms, &lost, Some(e.to_string())),
    };
    let slides_after = slide_count(&again);
    // Slides whose text, notes or shape count differ, and the first difference for the report.
    let mut changed = 0usize;
    let mut first: Option<String> = None;
    for (i, b) in before.iter().enumerate() {
        let a = match read_slide(&again, i) {
            Ok(a) => a,
            Err(_) if i >= slides_after => break,
            Err(e) => return report("reopen", n, open_ms, &lost, Some(format!("slide {}: {e}", i + 1))),
        };
        if a == *b {
            continue;
        }
        changed += 1;
        first.get_or_insert_with(|| {
            if a.text != b.text {
                format!("slide {} text: {:?} → {:?}", i + 1, b.text.chars().take(80).collect::<String>(), a.text.chars().take(80).collect::<String>())
            } else if a.notes != b.notes {
                format!(
                    "slide {} notes: {:?} → {:?}",
                    i + 1,
                    b.notes.chars().take(80).collect::<String>(),
                    a.notes.chars().take(80).collect::<String>()
                )
            } else {
                format!("slide {} shapes: {} → {}", i + 1, b.shapes, a.shapes)
            }
        });
    }
    let extra = format!(
        "{lost},\"slidesAfter\":{slides_after},\"slidesChanged\":{changed},\"firstChange\":{}",
        first.map(|f| json_str(&f)).unwrap_or_else(|| "null".into())
    );
    report("done", n, open_ms, &extra, None);
}
