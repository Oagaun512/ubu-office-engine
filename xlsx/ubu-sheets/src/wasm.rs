//! The browser binding: `wasm-bindgen` classes over [`crate::Book`]. Type conversion only;
//! every behaviour lives in the plain-Rust API so native tests cover it.
//!
//! The generated TypeScript must match `contract/ubu_sheets.d.ts` (checked in CI), so renaming
//! anything here is a contract change for Ubunex.

use wasm_bindgen::prelude::*;

use crate::{Book, SheetsError};

/// A thrown `Error` with `name = "SheetsError"` and a machine-readable `code` (see `ErrorCode`).
fn js_error(e: SheetsError) -> JsValue {
    let err = js_sys::Error::new(&e.message);
    err.set_name("SheetsError");
    // Setting a property on a fresh Error object cannot fail.
    let _ = js_sys::Reflect::set(&err, &JsValue::from_str("code"), &JsValue::from_str(e.code.code()));
    err.into()
}

/// An open workbook. Call `free()` when done.
#[wasm_bindgen]
pub struct SheetBook {
    inner: Book,
}

#[wasm_bindgen]
impl SheetBook {
    /// Open a workbook; `name` gives the format by extension when the bytes don't say. Throws a
    /// `SheetsError` (`code`: invalid, needs_password, wrong_password, unsupported).
    pub fn open(bytes: &[u8], name: &str, password: Option<String>) -> Result<SheetBook, JsValue> {
        Book::open(bytes, name, password.as_deref()).map(|inner| SheetBook { inner }).map_err(js_error)
    }

    /// What opening noticed (a decrypted workbook, features read but not kept, …).
    pub fn warnings(&self) -> Vec<String> {
        self.inner.warnings().to_vec()
    }

    /// The sheets, as JSON: `[{"name", "visibility", "tabColor"}]`.
    pub fn sheets(&self) -> String {
        self.inner.sheets()
    }

    #[wasm_bindgen(getter, js_name = activeSheet)]
    pub fn active_sheet(&self) -> u32 {
        self.inner.active_sheet() as u32
    }

    /// A sheet's layout, as JSON: sizes (CSS px at 100%), merges, frozen panes, used range,
    /// view settings, chart and image anchors, the default style.
    pub fn layout(&self, sheet: u32) -> Result<String, JsValue> {
        self.inner.layout(sheet as usize).map_err(js_error)
    }

    /// The cells of a window (inclusive), as JSON `{"styles", "cells"}`; `digitPx` is the width
    /// of a digit in the sheet's default font, for fitting General numbers (0 = off).
    pub fn window(&self, sheet: u32, row0: u32, col0: u32, row1: u32, col1: u32, digit_px: f32) -> Result<String, JsValue> {
        self.inner.window(sheet as usize, row0, col0, row1, col1, digit_px).map_err(js_error)
    }

    /// Every stored cell of a sheet with content, as JSON `[[row, col, text, input]…]` in
    /// reading order (the save check compares a workbook with its saved copy by it).
    pub fn contents(&self, sheet: u32) -> Result<String, JsValue> {
        let cells = self.inner.contents(sheet as usize).map_err(js_error)?;
        serde_json::to_string(&cells).map_err(|e| js_error(SheetsError::new(crate::ErrorCode::Invalid, e.to_string())))
    }

    /// What the formula bar shows for a cell.
    #[wasm_bindgen(js_name = cellInput)]
    pub fn cell_input(&self, sheet: u32, row: u32, col: u32) -> Result<String, JsValue> {
        self.inner.cell_input(sheet as usize, row, col).map_err(js_error)
    }

    /// A chart laid out at `width` × `height` CSS px, as JSON drawing primitives.
    pub fn chart(&self, sheet: u32, index: u32, width: f32, height: f32) -> Result<String, JsValue> {
        self.inner.chart(sheet as usize, index as usize, width, height).map_err(js_error)
    }

    /// Run an engine command (`cell.set`, `selection.set`, `home.bold`, `edit.undo`, …) with its
    /// JSON parameters. Returns JSON: `{"result": <the command's result>, "changed": bool, "rev":
    /// n, "sheet": n}`. Throws a `SheetsError` (`code`: invalid_command).
    pub fn run(&mut self, command: &str, params_json: &str) -> Result<String, JsValue> {
        let r = self.inner.run(command, params_json).map_err(js_error)?;
        Ok(format!("{{\"result\":{},\"changed\":{},\"rev\":{},\"sheet\":{}}}", r.result, r.changed, r.rev, r.sheet))
    }

    /// The selection, as JSON: `{"sheet", "active": [row, col], "ranges": [[r0, c0, r1, c1]…]}`.
    pub fn selection(&self) -> String {
        self.inner.selection()
    }

    #[wasm_bindgen(getter, js_name = isDirty)]
    pub fn is_dirty(&self) -> bool {
        self.inner.is_dirty()
    }

    #[wasm_bindgen(getter, js_name = canUndo)]
    pub fn can_undo(&self) -> bool {
        self.inner.can_undo()
    }

    #[wasm_bindgen(getter, js_name = canRedo)]
    pub fn can_redo(&self) -> bool {
        self.inner.can_redo()
    }

    /// The workbook as XLSX.
    #[wasm_bindgen(js_name = saveXlsx)]
    pub fn save_xlsx(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.save_xlsx().map_err(js_error)
    }

    /// One sheet as CSV (UTF-8).
    #[wasm_bindgen(js_name = saveCsv)]
    pub fn save_csv(&self, sheet: u32) -> Result<Vec<u8>, JsValue> {
        self.inner.save_csv(sheet as usize).map_err(js_error)
    }

    /// The first character on a sheet the PDF export cannot write, or undefined when it can
    /// write the whole sheet.
    #[wasm_bindgen(js_name = pdfUnwritable)]
    pub fn pdf_unwritable(&self, sheet: u32) -> Result<Option<String>, JsValue> {
        self.inner.pdf_unwritable(sheet as usize).map(|c| c.map(String::from)).map_err(js_error)
    }

    /// The active sheet as PDF.
    #[wasm_bindgen(js_name = exportPdf)]
    pub fn export_pdf(&mut self) -> Result<Vec<u8>, JsValue> {
        self.inner.export_pdf().map_err(js_error)
    }
}

/// The engine build: this repository's commit and the gridcraft fork's commit.
#[wasm_bindgen(js_name = engineVersion)]
pub fn engine_version() -> String {
    format!(
        "ubu-office-engine {} (sheets engine {})",
        option_env!("UBU_ENGINE_COMMIT").unwrap_or("dev"),
        option_env!("UBU_SHEETS_UPSTREAM_COMMIT").unwrap_or("dev")
    )
}
