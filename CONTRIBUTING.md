# Contributing to ubu-office-engine

Thank you for helping. This repository builds the headless WebAssembly engines behind Ubu Office:
`ubu-pdf`, `ubu-docx`, `ubu-sheets` and `ubu-slides`. Each is a thin `wasm-bindgen` layer over a
third-party open-source format engine. Please read this page before opening a pull request.

Found a security problem? Do not open an issue or pull request; follow `SECURITY.md`.

## How the repository is laid out

| Path | What |
|---|---|
| `crates/ubu-pdf` | The PDF engine, in the root Cargo workspace (`Cargo.toml`, `Cargo.lock`). |
| `docx/` | The DOCX engine (`docx/ubu-docx`), its own workspace and lockfile. |
| `xlsx/` | The spreadsheet engine (`xlsx/ubu-sheets`), its own workspace and lockfile. |
| `pptx/` | The presentation engine (`pptx/ubu-slides`), its own workspace and lockfile. |
| `upstream/` | The third-party format engines, as git submodules pinned to our forks. |
| `contract/` | The reviewed TypeScript API of each engine. CI fails if the generated bindings differ. |
| `corpus/` | How the test corpora are fetched (nothing is committed; see `corpus/README.md`). |
| `scripts/` | Corpus fetchers, round-trip runners, the export check, the SBOM and licence tools. |
| `licenses/` | Licence texts for crates published without one. |

Each engine with a different upstream has its own workspace, so one upstream's patches never reach
another's dependencies. Run each engine's commands from its own manifest, as below.

## Build and test

You need git, [rustup](https://rustup.rs), and Node.js 22 for the scripts. The Rust toolchain is
pinned in `rust-toolchain.toml`, with `rustfmt`, `clippy` and the `wasm32-unknown-unknown` target;
run `rustup toolchain install` inside the repository to get it, as CI does.

```sh
git clone --recurse-submodules https://github.com/Oagaun512/ubu-office-engine
cd ubu-office-engine
scripts/setup-remotes.sh        # adds an `upstream` remote in each fork under upstream/
```

Native tests, one per engine:

```sh
cargo test --locked -p ubu-pdf
cargo test --locked --manifest-path docx/Cargo.toml -p ubu-docx
cargo test --locked --manifest-path xlsx/Cargo.toml -p ubu-sheets
cargo test --locked --manifest-path pptx/Cargo.toml -p ubu-slides
```

Formatting and lints, as CI runs them (shown for the PDF engine; for the others, run the same
commands inside `docx/`, `xlsx/` or `pptx/` with `-p ubu-docx`, `-p ubu-sheets` or `-p ubu-slides`):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --locked -p ubu-pdf --target wasm32-unknown-unknown -- -D warnings
node scripts/check-patches.mjs  # our [patch.crates-io] tables still match the upstream engines'
```

### What CI does

Four workflows in `.github/workflows/` (`build.yml` for PDF, `build-docx.yml`, `build-xlsx.yml`,
`build-pptx.yml`) run on every pull request. For each engine they:

1. check that the forks under `upstream/` sit at their pinned commits with no local changes;
2. run `cargo fmt --check`, `clippy -D warnings` (native and wasm32) and the native tests;
3. build the release wasm, run `wasm-bindgen --target web` and `wasm-opt -Oz`, report sizes
   (with a size gate for DOCX, XLSX and PPTX), and run `scripts/check-exports.mjs`, which fails
   if the generated `.d.ts` differs from `contract/`;
4. for DOCX, XLSX and PPTX, run a worker smoke test (the engine inside a worker, as Ubunex
   runs it);
5. run `cargo deny check licenses bans sources` and write `SBOM.md`, failing on any copyleft or
   unknown licence or unexpected source;
6. fetch the pinned corpus and write the round-trip report `ROUNDTRIP.md` (report-only).

On `main` only, a last job assembles the built files and opens a pull request into Ubunex.

### Corpus round trips (optional locally)

The round trips need the corpus (downloaded, never committed) and a Linux-like shell. For
example, for presentations:

```sh
sh scripts/fetch-pptx-corpus.sh
(cd pptx && cargo build --locked --profile corpus -p ubu-slides --example roundtrip)
node scripts/roundtrip-pptx.mjs pptx/target/corpus/examples/roundtrip \
  corpus/cache/poi-slideshow/test-data/slideshow > ROUNDTRIP.md
```

The PDF, DOCX and XLSX runs follow the same pattern (`fetch-corpus.sh`, `fetch-docx-corpus.sh`,
`fetch-xlsx-corpus.sh` with `roundtrip.mjs`, `roundtrip-docx.mjs`, `roundtrip-xlsx.mjs`); the
exact arguments are in each workflow's `roundtrip` job.

## Changing an engine

- **The forks under `upstream/` stay unmodified.** We do not edit third-party code in this
  repository. A fix to a format engine goes to its upstream project first; the fork's pin then
  moves to that commit in a pull request here. The one exception is the reviewed patch on the
  presentation engine's fork described in `NOTICE`, and CI checks that nothing else changed.
- **The API is a contract.** If your change adds, removes or changes an exported function, update
  the matching file in `contract/` in the same pull request, and say why in its description.
  Ubunex depends on these signatures.
- **No new copyleft or unknown-licence dependencies.** `deny.toml` lists what is allowed. A new
  bundled asset (a font, a colour profile, character-mapping data) that `cargo deny` cannot see
  must also be added to `NOTICE` and its licence text kept with it.
- **Keep the browser build small and self-contained.** No threads, no system access, nothing the
  browser cannot run in a worker. The wasm32 dependency report in CI shows whether `getrandom`,
  `rayon` or `tokio` entered the tree.
- **Do not commit test files that hold anyone's personal data**, and do not commit corpus files at
  all; `corpus/README.md` explains where they come from.
- Never use the upstream projects' product names, wordmarks or logos in the engines' interface,
  messages or documentation for Ubu Office. Their names appear only where licences and notices
  require them.

Contributions toward fuzzing (for example `cargo-fuzz` targets for each engine's `open`) are
welcome; there are none yet.

## Licence of contributions

This repository is licensed under either of

- the Apache License, Version 2.0 (`LICENSE-APACHE`), or
- the MIT License (`LICENSE-MIT`),

at your option. Unless you explicitly state otherwise, any contribution you intentionally submit
for inclusion in this repository, as defined in the Apache-2.0 licence, is dual licensed as above,
without any additional terms or conditions.

Third-party code keeps its own licence. Do not remove or change `LICENSE-APACHE`, `LICENSE-MIT`,
`NOTICE`, the files under `licenses/`, or any licence, notice or attribution file inside
`upstream/`.

### Sign-off (Developer Certificate of Origin)

Please sign off each commit to certify that you wrote it or otherwise have the right to submit it
under the licences above, as described by the [Developer Certificate of Origin
1.1](https://developercertificate.org/):

```sh
git commit -s -m "Your message"
```

This adds a `Signed-off-by: Your Name <you@example.com>` line. It is not checked by CI today, but
maintainers may ask you to add it before merging.

## Pull request checklist

- [ ] One topic per pull request, with a description of what changed and why.
- [ ] `cargo fmt --all --check` and `cargo clippy ... -D warnings` pass for each engine you touched
      (native and `--target wasm32-unknown-unknown`).
- [ ] `cargo test --locked` passes for each engine you touched, and new behaviour has a test.
- [ ] Any change to an exported API is reflected in `contract/`.
- [ ] No change inside `upstream/` other than a reviewed pin move.
- [ ] No new dependency with a licence outside `deny.toml`; new bundled assets are in `NOTICE`.
- [ ] Lockfiles change only when dependencies really changed, and `--locked` still builds.
- [ ] No corpus files, personal data, secrets or built artifacts (`*.wasm`, `pkg/`, `dist/`) committed.
- [ ] Commits are signed off (`git commit -s`).
- [ ] If the round-trip report changes for the worse, the description says how and why.

## Conduct

Everyone taking part is expected to follow `CODE_OF_CONDUCT.md`.
