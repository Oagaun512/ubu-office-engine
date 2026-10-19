//! Write a small workbook for the browser-build smoke test (scripts/xlsx-worker-smoke.mjs):
//! `cargo run -p ubu-sheets --example make_sample -- out.xlsx`. Written with gridcraft's own
//! session, so no fixture file is kept in the repository: a bold, filled title merged over three
//! columns, a frozen header row, numbers and a formula, a second sheet.

use gridcraft_engine::{Session, io};
use serde_json::json;

fn main() {
    let out = std::env::args().nth(1).expect("usage: make_sample <out.xlsx>");
    let mut s = Session::new();
    s.new_workbook();
    let steps = [
        ("cell.set", json!({"cell": "A1", "input": "Ubu Sheets"})),
        ("home.bold", json!({"range": "A1"})),
        ("home.fillColor", json!({"range": "A1", "color": "#FFFF00"})),
        ("home.mergeCells", json!({"range": "A1:C1"})),
        ("cell.set", json!({"cell": "A2", "input": "2"})),
        ("cell.set", json!({"cell": "B2", "input": "3"})),
        ("cell.set", json!({"cell": "C2", "input": "=A2*B2"})),
        ("view.freezePanes", json!({"cell": "A2"})),
        ("home.insertSheet", json!({})),
        ("sheet.activate", json!({"sheet": 0})),
    ];
    for (id, params) in steps {
        s.execute_headless(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    std::fs::write(&out, io::save_bytes(&s.active().unwrap().wb, "sample.xlsx").unwrap()).unwrap();
}
