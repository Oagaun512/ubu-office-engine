//! Native tests of the Ubu Sheets engine API (Phase 4, S1). Workbooks are made with the
//! upstream engine itself and saved as XLSX bytes, so every test also goes through the reader.

use gridcraft_engine::{Session, io};
use serde_json::{Value, json};
use ubu_sheets::{Book, ErrorCode, win_ansi};

/// A workbook built by running engine commands on a new one, saved as XLSX.
fn xlsx(commands: &[(&str, Value)]) -> Vec<u8> {
    let mut s = Session::new();
    s.new_workbook();
    for (id, params) in commands {
        s.execute_headless(id, params.clone()).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    io::save_bytes(&s.active().unwrap().wb, "t.xlsx").unwrap()
}

fn set(cell: &str, input: &str) -> (&'static str, Value) {
    ("cell.set", json!({"cell": cell, "input": input}))
}

fn window(book: &Book, r1: u32, c1: u32) -> Value {
    serde_json::from_str(&book.window(0, 0, 0, r1, c1, 7.0).unwrap()).unwrap()
}

/// The cell at (row, col) in a window, as its array.
fn cell(w: &Value, row: u32, col: u32) -> Option<Vec<Value>> {
    w["cells"].as_array().unwrap().iter().find(|c| c[0] == row && c[1] == col).map(|c| c.as_array().unwrap().clone())
}

#[test]
fn a_workbook_opens_with_its_sheets_values_and_formulas() {
    let bytes = xlsx(&[set("A1", "Hello"), set("B1", "=1+2"), set("A2", "2026-10-19"), ("home.insertSheet", json!({}))]);
    let book = Book::open(&bytes, "t.xlsx", None).unwrap();
    let sheets: Value = serde_json::from_str(&book.sheets()).unwrap();
    assert_eq!(sheets.as_array().unwrap().len(), 2);
    assert_eq!(sheets[0]["visibility"], "visible");
    let w = window(&book, 5, 5);
    let a1 = cell(&w, 0, 0).unwrap();
    assert_eq!(a1[2], "Hello");
    let b1 = cell(&w, 0, 1).unwrap();
    assert_eq!(b1[2], "3", "the formula is calculated");
    assert_eq!(b1[4].as_u64().unwrap() & 3, 3, "numeric and a formula");
    assert!(cell(&w, 3, 3).is_none(), "empty cells are not listed");
    assert_eq!(book.cell_input(0, 0, 1).unwrap(), "=1+2", "the formula bar shows the formula");
    let a2 = cell(&w, 1, 0).unwrap();
    assert!(a2[2].as_str().unwrap().contains("2026") || a2[2].as_str().unwrap().contains("10"), "a date shows as a date: {a2:?}");
}

#[test]
fn general_numbers_fit_their_column_and_hashes_fill_a_column_too_narrow() {
    let bytes = xlsx(&[set("A1", "123456789.123456"), ("home.columnWidth", json!({"cols": "B:B", "width": 10})), set("B1", "123456789")]);
    let book = Book::open(&bytes, "t.xlsx", None).unwrap();
    let w = window(&book, 0, 1);
    let a1 = cell(&w, 0, 0).unwrap()[2].as_str().unwrap().to_string();
    // A default column (64 px) holds 8 digits of 7 px: Excel shows this number as 1.23E+08.
    assert_eq!(a1, "1.23E+08", "General narrows to the column");
    let b1 = cell(&w, 0, 1).unwrap()[2].as_str().unwrap().to_string();
    assert!(b1.chars().all(|c| c == '#'), "too narrow for the number: {b1}");
    let full: Value = serde_json::from_str(&book.window(0, 0, 0, 0, 0, 0.0).unwrap()).unwrap();
    assert_eq!(cell(&full, 0, 0).unwrap()[2], "123456789.123456", "digit width 0 turns fitting off");
}

#[test]
fn styles_merges_and_frozen_panes_reach_the_viewer() {
    let bytes = xlsx(&[
        set("A1", "Title"),
        ("home.bold", json!({"range": "A1"})),
        ("home.fillColor", json!({"range": "A1", "color": "#FFFF00"})),
        ("home.mergeCells", json!({"range": "A1:C1"})),
        ("home.columnWidth", json!({"cols": "D:D", "width": 30})),
        ("view.freezePanes", json!({"cell": "B2"})),
    ]);
    let book = Book::open(&bytes, "t.xlsx", None).unwrap();
    let layout: Value = serde_json::from_str(&book.layout(0).unwrap()).unwrap();
    assert_eq!(layout["merges"], json!([[0, 0, 0, 2]]));
    assert_eq!(layout["freeze"], json!([1, 1]));
    assert!(layout["cols"].as_array().unwrap().iter().any(|c| c[0] == 3), "a custom column width");
    assert!(layout["defaultStyle"]["font"].as_str().is_some());
    let w = window(&book, 0, 3);
    let a1 = cell(&w, 0, 0).unwrap();
    let style = &w["styles"][a1[3].as_u64().unwrap() as usize];
    assert_eq!(style["bold"], true);
    assert_eq!(style["fill"], "#FFFF00");
}

#[test]
fn commands_edit_recalculate_and_undo() {
    let bytes = xlsx(&[set("A1", "2"), set("B1", "=A1*10")]);
    let mut book = Book::open(&bytes, "t.xlsx", None).unwrap();
    assert!(!book.is_dirty() && !book.can_undo());
    let rev = book.revision();
    let r = book.run("cell.set", r#"{"cell":"A1","input":"5"}"#).unwrap();
    assert!(r.changed && r.rev > rev);
    assert_eq!(cell(&window(&book, 0, 1), 0, 1).unwrap()[2], "50", "the dependent cell recalculates");
    assert!(book.is_dirty() && book.can_undo());
    book.run("edit.undo", "").unwrap();
    assert_eq!(cell(&window(&book, 0, 1), 0, 1).unwrap()[2], "20");
    assert!(book.can_redo());
    let moved = book.run("selection.set", r#"{"range":"B1"}"#).unwrap();
    assert!(!moved.changed || moved.rev == book.revision(), "selecting is not an edit");
    let sel: Value = serde_json::from_str(&book.selection()).unwrap();
    assert_eq!(sel["active"], json!([0, 1]));
}

#[test]
fn bad_commands_and_files_are_errors_not_panics() {
    let mut book = Book::open(&xlsx(&[set("A1", "x")]), "t.xlsx", None).unwrap();
    assert_eq!(book.run("no.such.command", "{}").unwrap_err().code, ErrorCode::InvalidCommand);
    assert_eq!(book.run("cell.set", "not json").unwrap_err().code, ErrorCode::InvalidCommand);
    assert_eq!(book.run("file.open", "{}").unwrap_err().code, ErrorCode::InvalidCommand, "the viewer owns files");
    assert_eq!(book.window(0, 0, 0, 1_000, 1_000, 7.0).unwrap_err().code, ErrorCode::OutOfRange, "a window is capped");
    assert_eq!(book.layout(9).unwrap_err().code, ErrorCode::OutOfRange);
    assert_eq!(Book::open(b"not a workbook at all", "x.xlsx", None).err().unwrap().code, ErrorCode::Invalid, "text named .xlsx");
    assert_eq!(Book::open(b"PK broken zip", "x.xlsx", None).err().unwrap().code, ErrorCode::Invalid, "a broken package");
    assert!(
        Book::open(
            b"a,b
1,2
",
            "x.csv",
            None
        )
        .is_ok(),
        "a CSV still opens as text"
    );
    assert_eq!(ErrorCode::NeedsPassword.code(), "needs_password");
    assert_eq!(ErrorCode::Unsupported.code(), "unsupported");
}

#[test]
fn saving_round_trips_and_csv_and_pdf_are_written() {
    let mut book = Book::open(&xlsx(&[set("A1", "Name"), set("B1", "=2*21")]), "t.xlsx", None).unwrap();
    book.run("cell.set", r#"{"cell":"A2","input":"Edited"}"#).unwrap();
    let saved = book.save_xlsx().unwrap();
    assert_eq!(&saved[..2], b"PK");
    let again = Book::open(&saved, "copy.xlsx", None).unwrap();
    let w = window(&again, 1, 1);
    assert_eq!(cell(&w, 1, 0).unwrap()[2], "Edited");
    assert_eq!(again.cell_input(0, 0, 1).unwrap(), "=2*21");
    let csv = String::from_utf8(book.save_csv(0).unwrap()).unwrap();
    assert!(csv.contains("Name") && csv.contains("42") && csv.contains("Edited"), "{csv}");
    assert_eq!(book.pdf_unwritable(0).unwrap(), None);
    let pdf = book.export_pdf().unwrap();
    assert_eq!(&pdf[..5], b"%PDF-");
}

#[test]
fn text_the_pdf_cannot_write_is_found_first() {
    let book = Book::open(&xlsx(&[set("A1", "Café €5"), set("A2", "東京")]), "t.xlsx", None).unwrap();
    assert_eq!(book.pdf_unwritable(0).unwrap(), Some('東'));
    assert!(win_ansi('é') && win_ansi('€') && win_ansi('“') && !win_ansi('東') && !win_ansi('Ω'));
}

#[test]
fn a_chart_is_laid_out_as_drawing_primitives_with_its_text() {
    let mut book =
        Book::open(&xlsx(&[set("A1", "Q"), set("B1", "Sales"), set("A2", "Q1"), set("B2", "10"), set("A3", "Q2"), set("B3", "20")]), "t.xlsx", None)
            .unwrap();
    book.run("insert.chart", r#"{"range":"A1:B3","type":"column","title":"Sales"}"#).unwrap();
    let layout: Value = serde_json::from_str(&book.layout(0).unwrap()).unwrap();
    let charts = layout["charts"].as_array().unwrap();
    assert_eq!(charts.len(), 1);
    let prims: Value = serde_json::from_str(&book.chart(0, 0, 400.0, 300.0).unwrap()).unwrap();
    let prims = prims.as_array().unwrap();
    assert!(prims.iter().any(|p| p.get("Rect").is_some()), "bars");
    assert!(prims.iter().any(|p| p.get("Text").is_some_and(|t| t["text"] == "Sales")), "the title is text the viewer draws");
    assert_eq!(book.chart(0, 5, 400.0, 300.0).unwrap_err().code, ErrorCode::OutOfRange);
}
