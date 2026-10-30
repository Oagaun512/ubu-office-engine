# Changelog

What changed in ubu-office-engine, grouped by milestone and written for people, not machines.
The engines are not released as numbered versions: Ubunex vendors builds of `main`, and each
build's `VERSION` file names the exact commit. Pull request numbers are in brackets.

Milestone letters follow the Ubu Office plan: **E** for the shared engine work and the PDF engine,
**D** for Docs (DOCX), **S** for Sheets (XLSX) and **P** for Slides (PPTX). Two labels were used
twice: "E1" (the workspace in #1, PDF password protection in #7, PDF editing in #20) and "P1"
(PDF text and search in #6, the presentation engine in #16). Steps of the plan that are missing
below (such as S3–S7, P3, P4 and P6) have no engine-side change in this repository.

## Unreleased

- **Launch documents** (2026-10-30): `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, this
  changelog, and a README section on where the engines run in Ubunex. No engine code changed
  since E1 (#20).
- In Ubunex (not this repository), Ubu Office got its own home at `/office`, and the PDF editing
  from #20 reached the app as "Organize pages" and "Comment". Both are still flagged admins-first.

## PDF editing (E1, 2026-10-24)

- **PDF editing** [#20]. A new editing interface for PDFs: rotate, delete, move and extract
  pages; insert pages from another PDF (its bookmarks go under one named after the file, and
  clashing form-field names are renamed); add comments (highlight, underline, strikeout, squiggly
  underline, sticky note, text box and freehand ink), each drawn so other viewers show it; set
  the document's Producer and Creator. Saving appends an incremental update, so the original
  bytes are untouched and existing digital signatures over them stay valid.

## Slides: the presentation engine (P-series, 2026-10-19)

- **P1: the presentation engine** [#16]. `ubu-slides` opens PPTX files, draws slides at any
  scale, and gives each slide's text, text positions, shapes and speaker notes. It runs editing
  commands with undo, saves PPTX and exports PDF. It is built without audio and video decoders, so
  media in a presentation is not played; CI checks no decoder slips into the browser build.
- **P2: round trip on a real-world corpus** [#17]. Every presentation in Apache POI's test data
  (100 PowerPoint 2007+ files and 145 old `.ppt` files) is opened, drawn, exported to PDF, saved and
  reopened, and the result is compared. Old `.ppt` files are refused cleanly. The report ships with
  each build.
- **P5: editing by pointer and keyboard** [#18]. Select, move, resize and rotate shapes, place the
  caret and select text with the pointer; type (including composed input for languages that need
  it), nudge, delete and use the command shortcuts from the keyboard. The engine also says what an
  editor should draw on top of the slide: selection handles, the caret, selected text and what the
  pointer is over.
- **P7: the slide show** [#19]. Start (from the beginning or any slide), next, back, go to a
  slide, and end. Hidden slides are skipped, click-by-click builds step through (each build jumps
  straight to its end state, without animation), and rehearsed timings are kept.

## Sheets: the spreadsheet engine (S-series, 2026-10-19)

- **S1: the spreadsheet engine** [#14]. `ubu-sheets` opens XLSX workbooks (including
  password-protected ones), lays out each sheet (column widths, row heights, merged cells, frozen
  panes, charts), returns windows of cells with their displayed text and effective style
  (including table and conditional formatting), draws charts, runs editing commands, saves XLSX
  and CSV, and exports PDF, refusing cleanly when the text cannot be written in the PDF's
  character set.
- **S2: round trip on a real-world corpus** [#15]. Every workbook in Apache POI's spreadsheet test
  data is laid out, drawn, exported, saved and reopened, and every cell's text and formula is
  compared. Each file is limited to 3 GB of memory so one runaway file cannot stop the run. The
  report ships with each build.

## Docs: the DOCX engine (D-series, 2026-10-18)

- **D1: the DOCX engine** [#8]. `ubu-docx` opens Word documents (including password-protected
  ones), draws pages (capped at 16 megapixels), gives page text, accepts extra fonts, saves DOCX
  and exports PDF. It lives in its own Cargo workspace so its upstream's patches cannot
  affect the other engines.
- **D3: text layer and find** [#10]. Every line of text on a page (body, header and footer) with
  its position, so text can be selected and copied, and search with case and whole-word options.
- **D4: round trip on a real-world corpus** [#11]. 130 Word files from Apache POI's test data,
  many from real bug reports, are opened, drawn, searched, exported to PDF, saved and reopened. The
  report lists crashes, failures, text that changed and rich content (charts, diagrams, embedded
  objects, media, comments) a saved copy would lose.
- **D5: editing** [#12]. Typing, deleting, new paragraphs, caret movement, selection, undo and redo,
  and copy, cut and paste, all on one shared layout used for both reading and editing.
- **D6: formatting toolbar state** [#13]. The engine reports the formatting at the caret or
  selection (and the paragraph's alignment and list type), so a toolbar can show which buttons are
  on.

## PDF engine and the shared build (E-series and P1, 2026-10-17 to 2026-10-18)

- **Bootstrap.** README, the MIT and Apache-2.0 licences, and NOTICE.
- **E1: workspace and the native PDF engine** [#1]. The PDF engine forks are added as pinned
  submodules, and `ubu-pdf` can open a PDF, report page sizes, draw pages (capped at 16
  megapixels), extract text and save. CI checks formatting, lints and that the forks are
  unmodified.
- **E2: the WebAssembly interface** [#2]. The browser API, a reviewed TypeScript contract it must
  match, and a CI job that builds, shrinks and size-checks the wasm.
- **E3: licence gate, SBOM, corpus round trip and the artifact bundle** [#3]. Only permissive
  licences and known sources are allowed; every build lists what it contains and ships each
  licence text. Every file in pdf.js's own test suite (997 files, including encrypted and
  damaged ones) is opened and saved in its own process with a 60-second timeout, and the report
  ships with the build.
- **E4: publishing into Ubunex** [#4]. A build of `main` opens or updates a pull request into
  Ubunex that carries only the built files.
- **Licence text clean-up** [#5, #9]. Trailing spaces are stripped from every copied licence
  file, so Ubunex's whitespace check passes.
- **P1: PDF text layer and search** [#6]. Text lines with their positions for selection, and
  search with case and whole-word options (up to 1000 matches a page). The publishing token
  moved into a GitHub environment that only `main` can use.
- **E1: PDF password protection** [#7]. Lock a PDF with AES-256 (with optional print, copy and
  modify permissions), unlock it, and report how a document is protected.
