# Licence texts for crates published without one

A few crates are published to crates.io without their licence file (it sits at the root of their
repository instead). `scripts/licenses.mjs` reproduces the text from `licenses/<crate>/` for
those, so every linked crate's licence still ships with the engine.

| Crate | Licence | Text taken from |
|---|---|---|
| `wuff` 0.2.9 (WOFF/WOFF2 decoding, linked by the DOCX engine through wordcraft-fonts) | MIT | `LICENSE` at tag `v0.2.9` of https://github.com/nicoburns/wuff |
| `i_overlay` 9.0.0 (polygon boolean operations, linked by the slides engine) | MIT OR Apache-2.0 | `LICENSE-MIT`, `LICENSE-APACHE` at tag `9.0.0` of https://github.com/iShape-Rust/iOverlay |
| `i_key_sort` 0.11.0 (sorting, linked through i_overlay) | MIT | `LICENSE` at tag `0.11.0` of https://github.com/iShape-Rust/iKeySort |
| `epaint_default_fonts` 0.36.2 (the slides engine's last-resort face, Ubuntu Light) | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | `LICENSE-MIT`, `LICENSE-APACHE` at tag `0.36.2` of https://github.com/emilk/egui; `LICENSE-UFL-1.0.txt` and `LICENSE-OFL-1.1.txt` are the crate's own `fonts/UFL.txt` and `fonts/OFL.txt` |
