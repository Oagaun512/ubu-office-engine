# Licence texts for crates published without one

A few crates are published to crates.io without their licence file (it sits at the root of their
repository instead). `scripts/licenses.mjs` reproduces the text from `licenses/<crate>/` for
those, so every linked crate's licence still ships with the engine.

| Crate | Licence | Text taken from |
|---|---|---|
| `wuff` 0.2.9 (WOFF/WOFF2 decoding, linked by the DOCX engine through wordcraft-fonts) | MIT | `LICENSE` at tag `v0.2.9` of https://github.com/nicoburns/wuff |
