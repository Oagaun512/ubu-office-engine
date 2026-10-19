# ubu-office-engine

Headless WebAssembly document engines for **Ubu Office**, the document suite of
[Ubunex](https://ubunex.com) (Beta).

| Engine | Crate | Status |
|---|---|---|
| PDF | `crates/ubu-pdf` (root workspace) | open, render, text, find, protect, save |
| Docs (DOCX) | `docx/ubu-docx` (own workspace) | open, render, text, find, edit, format, save, PDF |
| Sheets (XLSX) | `xlsx/ubu-sheets` (own workspace) | S1 — open, layout, cell windows, commands, save XLSX/CSV, PDF |
| Slides (PPTX) | — | planned |

Each engine with a different upstream has its own Cargo workspace (and `Cargo.lock`), so one
upstream's patches never reach another's dependencies.

## How it works

- The format engines are third-party open-source Rust code (MIT OR Apache-2.0), kept as
  **unmodified forks** mounted as git submodules under `upstream/`. See `NOTICE`.
- Each `crates/ubu-*` crate is a thin `wasm-bindgen` layer exposing the same verbs:
  open · render · edit · save · export.
- GitHub Actions on Ubuntu builds the wasm, gates licences (`cargo-deny`), writes an
  `SBOM.md`, checks the generated TypeScript bindings against the reviewed contract in
  `contract/`, measures sizes, runs round-trip tests on a real-world corpus, and opens a pull
  request into Ubunex that vendors **only the built artifacts**.

Nothing here is built inside Ubunex.

## Working on it

```sh
git clone --recurse-submodules https://github.com/Oagaun512/ubu-office-engine
scripts/setup-remotes.sh        # adds `upstream` remotes in each fork
cargo test -p ubu-pdf           # native tests (PDF)
cargo test --manifest-path docx/Cargo.toml -p ubu-docx
cargo test --manifest-path xlsx/Cargo.toml -p ubu-sheets
```

## Licence

MIT OR Apache-2.0, at your option. Third-party code keeps its own licence files.
