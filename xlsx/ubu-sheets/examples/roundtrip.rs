//! Round trip of one workbook through the spreadsheet engine API, for the corpus run (Ubu
//! Office Phase 4, S2): open → every sheet's layout, a first screen of cells and its charts →
//! export PDF → save XLSX → reopen the copy → same sheets, and every cell's text and formula the
//! same (`Book::contents`, the measure the S7 save check uses).
//!
//!   cargo run --example roundtrip -- <file.xlsx>
//!
//! Prints one JSON line. `scripts/roundtrip-xlsx.mjs` runs it per file under a timeout (a hang
//! or a crash is a result too) and writes ROUNDTRIP.md. The `lost` list names the kinds of
//! package part the original has and the saved copy does not (charts, pivots, slicers, macros…):
//! what the S7 save check must warn about (decision D21).

use std::time::Instant;

use ubu_sheets::Book;

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

/// The kinds of rich content an XLSX package can hold, by part name.
const KINDS: [(&str, &str); 10] = [
    ("charts", "xl/charts/"),
    ("drawings", "xl/drawings/"),
    ("pivot tables", "xl/pivotTables/"),
    ("pivot caches", "xl/pivotCache/"),
    ("slicers", "xl/slicers/"),
    ("timelines", "xl/timelines/"),
    ("macros", "xl/vbaProject.bin"),
    ("comments", "xl/comments"),
    ("external links", "xl/externalLinks/"),
    ("embedded objects", "xl/embeddings/"),
];

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: roundtrip <file.xlsx>");
        std::process::exit(2);
    };
    let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let start = Instant::now();
    let report = |stage: &str, sheets: usize, cells: usize, open_ms: u128, extra: &str, error: Option<String>| {
        println!(
            "{{\"stage\":{},\"ok\":{},\"sheets\":{},\"cells\":{},\"openMs\":{},\"ms\":{}{},\"error\":{}}}",
            json_str(stage),
            error.is_none(),
            sheets,
            cells,
            open_ms,
            start.elapsed().as_millis(),
            extra,
            error.map(|e| json_str(&e)).unwrap_or_else(|| "null".into())
        );
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return report("read", 0, 0, 0, "", Some(e.to_string())),
    };
    let mut book = match Book::open(&bytes, &name, None) {
        Ok(b) => b,
        Err(e) => return report("open", 0, 0, start.elapsed().as_millis(), "", Some(e.to_string())),
    };
    let open_ms = start.elapsed().as_millis();
    let sheets: Vec<serde_json::Value> = serde_json::from_str(&book.sheets()).unwrap_or_default();
    let n = sheets.len();
    let mut before = Vec::new();
    let mut cells = 0;
    for si in 0..n {
        // What the viewer does on opening a sheet: its layout, the first screen, its charts.
        let layout: serde_json::Value = match book.layout(si).map(|l| serde_json::from_str(&l)) {
            Ok(Ok(l)) => l,
            Ok(Err(e)) => return report("layout", n, cells, open_ms, "", Some(e.to_string())),
            Err(e) => return report("layout", n, cells, open_ms, "", Some(e.to_string())),
        };
        if let Err(e) = book.window(si, 0, 0, 59, 25, 7.0) {
            return report("window", n, cells, open_ms, "", Some(e.to_string()));
        }
        let charts = layout["charts"].as_array().map(Vec::len).unwrap_or(0);
        for ci in 0..charts {
            if let Err(e) = book.chart(si, ci, 480.0, 288.0) {
                return report("chart", n, cells, open_ms, "", Some(e.to_string()));
            }
        }
        match book.contents(si) {
            Ok(c) => {
                cells += c.len();
                before.push(c);
            }
            Err(e) => return report("contents", n, cells, open_ms, "", Some(e.to_string())),
        }
    }
    let pdf_ok = match book.pdf_unwritable(book.active_sheet()) {
        Ok(None) => match book.export_pdf() {
            Ok(_) => "true",
            Err(e) => return report("pdf", n, cells, open_ms, "", Some(e.to_string())),
        },
        // D20: the viewer refuses this PDF; not a failure of the round trip.
        Ok(Some(_)) => "\"refused\"",
        Err(e) => return report("pdf", n, cells, open_ms, "", Some(e.to_string())),
    };
    let saved = match book.save_xlsx() {
        Ok(s) => s,
        Err(e) => return report("save", n, cells, open_ms, "", Some(e.to_string())),
    };
    let (orig, copy) = (zip_names(&bytes), zip_names(&saved));
    let lost: Vec<String> = KINDS
        .iter()
        .filter(|(_, prefix)| orig.iter().any(|x| x.starts_with(prefix)) && !copy.iter().any(|x| x.starts_with(prefix)))
        .map(|(kind, _)| json_str(kind))
        .collect();
    let lost = format!(",\"lost\":[{}],\"pdf\":{pdf_ok}", lost.join(","));
    let again = match Book::open(&saved, "copy.xlsx", None) {
        Ok(b) => b,
        Err(e) => return report("reopen", n, cells, open_ms, &lost, Some(e.to_string())),
    };
    let sheets_after = serde_json::from_str::<Vec<serde_json::Value>>(&again.sheets()).map(|s| s.len()).unwrap_or(0);
    // Cells whose text or formula differ, and the first one for the report.
    let mut changed = 0usize;
    let mut first: Option<String> = None;
    for (si, b) in before.iter().enumerate() {
        let a = again.contents(si).unwrap_or_default();
        let mut i = 0;
        let mut j = 0;
        while i < b.len() || j < a.len() {
            match (b.get(i), a.get(j)) {
                (Some(x), Some(y)) if (x.0, x.1) == (y.0, y.1) => {
                    if x != y {
                        changed += 1;
                        // Name the part that differs: the formula (or typed value), or the value shown.
                        first.get_or_insert_with(|| {
                            if x.3 != y.3 {
                                format!("sheet {si} R{}C{} input: {:?} → {:?}", x.0 + 1, x.1 + 1, x.3, y.3)
                            } else {
                                format!("sheet {si} R{}C{} value of {:?}: {:?} → {:?}", x.0 + 1, x.1 + 1, x.3, x.2, y.2)
                            }
                        });
                    }
                    i += 1;
                    j += 1;
                }
                (Some(x), Some(y)) if (x.0, x.1) < (y.0, y.1) => {
                    changed += 1;
                    first.get_or_insert_with(|| format!("sheet {si} R{}C{}: {:?} lost", x.0 + 1, x.1 + 1, x.3));
                    i += 1;
                }
                (Some(x), None) => {
                    changed += 1;
                    first.get_or_insert_with(|| format!("sheet {si} R{}C{}: {:?} lost", x.0 + 1, x.1 + 1, x.3));
                    i += 1;
                }
                (_, Some(y)) => {
                    changed += 1;
                    first.get_or_insert_with(|| format!("sheet {si} R{}C{}: {:?} added", y.0 + 1, y.1 + 1, y.3));
                    j += 1;
                }
                (None, None) => break,
            }
        }
    }
    let extra = format!(
        "{lost},\"sheetsAfter\":{sheets_after},\"cellsChanged\":{changed},\"firstChange\":{}",
        first.map(|f| json_str(&f)).unwrap_or_else(|| "null".into())
    );
    report("done", n, cells, open_ms, &extra, None);
}
