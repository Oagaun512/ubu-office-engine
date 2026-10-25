# Security policy

ubu-office-engine builds the WebAssembly engines that Ubu Office (part of
[Ubunex](https://ubunex.com)) uses to open, render, edit and save PDF, DOCX, XLSX and PPTX files.
Those files often come from someone else, so a bug in how an engine reads one can be a security
bug. Thank you for reporting it privately.

## How to report

**Please do not open a public issue, pull request or discussion for a vulnerability.**

1. **Preferred: GitHub private vulnerability reporting.** On this repository, open the
   **Security** tab and choose **Report a vulnerability**. Only the maintainers see the report,
   and we can work on a fix and an advisory with you there.

   > Maintainer note: private vulnerability reporting has to be switched on by the repository
   > owner (Settings → Advanced Security, called "Code security" on some accounts → Private
   > vulnerability reporting → Enable). Until it is, the button does not appear; use the email
   > address below.

2. **Email:** `support@ubunex.com`, with "Security: ubu-office-engine" in the subject. Do not
   attach a malicious file to an unencrypted email if it contains anyone's personal data; describe
   it and we will arrange a private way to send it.

Please include:

- which engine (`ubu-pdf`, `ubu-docx`, `ubu-sheets`, `ubu-slides`) and the commit you tested;
- the smallest input file that reproduces it, or a script that generates one;
- what happens (crash, hang, memory growth, wrong output, data leaving where it should not)
  and what you expected;
- whether you think it is reachable from a file a person opens in Ubu Office.

We aim to acknowledge a report within 3 working days and to give a first assessment within 10.
We will tell you when a fix lands and credit you in the advisory if you would like.

## Supported versions

The engines are not published as crates or packages. Ubunex vendors builds of the `main` branch
(each build's `VERSION` file names the exact commit). Only the latest `main` is supported: fixes
land there and reach Ubunex through the next vendored build. There are no maintained release
branches.

| Version | Supported |
|---|---|
| `main` (latest commit) | yes |
| any older commit | no |

## Scope

**In scope**

- **Parsing untrusted documents.** This is the main attack surface: every engine reads PDF,
  DOCX, XLSX or PPTX bytes that may be damaged or malicious. Memory-safety bugs, panics, unbounded
  memory or CPU use (a "zip bomb" or a file that never finishes laying out), and any way a file
  can make the engine read or write outside the document it was given.
- The `wasm-bindgen` layers in `crates/ubu-pdf`, `docx/ubu-docx`, `xlsx/ubu-sheets` and
  `pptx/ubu-slides`, and the TypeScript contracts in `contract/`.
- Password protection in `ubu-pdf` (AES-256 lock and unlock): weak keys, passwords that leak, or
  a document that claims to be protected and is not.
- Saved output: a save or PDF export that silently loses or corrupts content in a way someone
  could exploit (for example, an "incremental" PDF save that changes the signed original bytes).
- The build and supply chain in this repository: the GitHub Actions workflows, the licence and
  source gate (`deny.toml`), and the job that opens the artifact pull request into Ubunex.

**Out of scope here** (report them, but they are fixed elsewhere)

- The Ubu Office web interface and the rest of Ubunex. Report those to the same address; they
  live in the Ubunex application, not this repository.
- Bugs in the third-party engines under `upstream/` that do not reach a build of ours. If one
  does reach our build, it is in scope, and we will also tell the upstream project.
- Old PowerPoint `.ppt` files, which the presentation engine refuses by design.

## What protects you today

These are the existing defences, so you know what a report would be getting past:

- **Rust, compiled to WebAssembly, run in a browser worker.** The engines have no file-system or
  network access of their own; they see only the bytes they are handed.
- **`panic = "abort"`** in release builds, so a panic ends the engine instance instead of
  continuing in a broken state.
- **Resource limits** in the engines, such as a 16-megapixel cap on a rendered page (PDF, DOCX)
  and at most 1000 search matches per page.
- **Corpus round trips in CI.** Every engine is run over a pinned public corpus of real-world and
  deliberately broken files (pdf.js's test suite for PDF; Apache POI's test data for DOCX, XLSX
  and PPTX; see `corpus/README.md`). Each file runs in its own process with a 60-second timeout,
  and the spreadsheet and presentation runs cap each file at 3 GB of memory, so a crash, hang or
  runaway file is recorded rather than hidden. The report (`ROUNDTRIP.md`) is read by a person;
  it does not fail the build.
- **No fuzzing yet.** There is no `cargo-fuzz` target or property-based test in this repository.
  Fuzz targets for the four `open` paths would be a welcome contribution (see
  `CONTRIBUTING.md`).
- **Licence and source gate.** `cargo deny` allows crates.io and our own forks only, and the
  forks are checked to be at their pinned commits with no unreviewed changes.
- **Least-privilege publishing.** The token that opens pull requests into Ubunex lives in a
  GitHub environment only `main` can deploy to, so pull requests and other branches cannot read
  it.
