# ubu-office-engine

Headless WebAssembly document engines for **Ubu Office**, the document suite of
[Ubunex](https://ubunex.com) (Beta).

| Engine | Crate | Status |
|---|---|---|
| PDF | `crates/ubu-pdf` (root workspace) | open, render, text, find, protect, edit (pages, comments; incremental save), save |
| Docs (DOCX) | `docx/ubu-docx` (own workspace) | open, render, text, find, edit, format, save, PDF |
| Sheets (XLSX) | `xlsx/ubu-sheets` (own workspace) | S2 — open, layout, cell windows, commands, save XLSX/CSV, PDF |
| Slides (PPTX) | `pptx/ubu-slides` (own workspace) | P7 — open, render slides, text layer, shapes, notes, commands, editing input (pointer, keys, typing), slide show, save PPTX, PDF (no media decoders; D23) |

Each engine with a different upstream has its own Cargo workspace (and `Cargo.lock`), so one
upstream's patches never reach another's dependencies.

## How it works

- The format engines are third-party open-source Rust code (MIT OR Apache-2.0), kept as
  **unmodified forks** mounted as git submodules under `upstream/`, except for one reviewed patch
  on the presentation engine's fork. See `NOTICE`.
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
cargo test --manifest-path pptx/Cargo.toml -p ubu-slides
```

## Where the engines run in Ubunex

Ubu Office has its own front door at `/office` (Home, Recent, Documents, Sheets, Slides, PDFs,
Templates). As of 2026-10-30 it is still flagged in Ubunex (`ubu_office`, with the `/office` home
behind `product_homes`), and the Office 2.0 features built on these engines ship admins-first:
PDF page organising and comments (`office_pdf_edit`, built on E1 #20) and the Office-style
ribbons (`office_ribbon_v2`). The third-party licences of everything linked here are shown on
Ubunex's `/legal/third-party` page.

## Contributing, security and support

See `CONTRIBUTING.md` (build, test, pull request checklist), `SECURITY.md` (report a vulnerability
privately), `CODE_OF_CONDUCT.md` and `CHANGELOG.md`. For help with Ubu Office itself (not this
repository), see Ubunex's `SUPPORT.md` or write to `support@ubunex.com`.

## Licence

MIT OR Apache-2.0, at your option. Third-party code keeps its own licence files.
