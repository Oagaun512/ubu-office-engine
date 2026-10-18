# ubu-office-engine

Headless WebAssembly document engines for **Ubu Office**, the document suite of
[Ubunex](https://ubunex.com) (Beta).

| Engine | Crate | Status |
|---|---|---|
| PDF | `crates/ubu-pdf` | M1 — open, render, extract text, save |
| Docs (DOCX) | `crates/ubu-docs` | planned |
| Sheets (XLSX) | `crates/ubu-sheets` | planned |
| Slides (PPTX) | `crates/ubu-slides` | planned |

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
cargo test -p ubu-pdf           # native tests
```

## Licence

MIT OR Apache-2.0, at your option. Third-party code keeps its own licence files.
