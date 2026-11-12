# Export and output formats: design

Status: design for owner review. Not committed. No implementation plan yet.
Builds on: `docs/superpowers/specs/2026-11-12-cross-platform-pdf-design.md` (Typst approved,
`typst`, `typst-pdf`, `typst-render`, pinned `=0.15.0`).

## Problem

Sketch Rust can already write several outputs, but they grew independently:

| Output | Today | Where |
|---|---|---|
| Report PDF | Hand-written PDF writer (~1,900 lines) | `sketch-core/src/export_pdf.rs`, `report_fonts.rs` |
| Print Preview | WinRT `Windows.Data.Pdf` (Windows only) | `sketch-app/src/print.rs` |
| SVG | String builder over `ExportPage` | `sketch-core/src/export_svg.rs` |
| PNG | Separate tiny-skia drawing code (`render_png`), transparent background, size from a `"WxH"` string | `sketch-core/src/export_png.rs` |
| DXF | CAD text export | `sketch-core/src/export_dxf.rs` |
| Appraise-It report image | SVG (`report_sketch.rs`) rasterised with `resvg` and system fonts, then handed to the Windows-only Appraise-It bridge, which needs a 2615 x 4202 aspect | `sketch-app/src/report_sketch.rs` and the Appraise-It integration module |

Problems: the PNG and SVG paths are two separate drawers of the same `ExportPage`, so they
can drift; the app calls them from `main.rs` with ad-hoc options; there is no JPEG, no DPI
choice, no format picker, and nothing usable headless or from WASM. The owner now wants
"other typical image types, with an option to choose the format".

## Goals

- One export entry point in `sketch-core`, UI-free, that turns a document plus options into
  bytes for any supported format.
- A format choice in the export dialog (PDF, SVG, PNG, JPEG, DXF) with sensible options.
- Identical output on Windows, macOS, Linux and `wasm32-unknown-unknown`.
- Deterministic output: same input and options give the same bytes (or the same pixels).
- Bundled fonts only; no dependence on installed fonts for any cross-platform format.
- Vendor-neutral: no format or crate that ties the product to one vendor's runtime.

## Non-goals

- Replacing the Windows metafile bridge (EMF/WMF) or making it cross-platform.
- `sketchy.json` export. It is a sibling export target (data, not an image) handled
  separately; it plugs into the same `export()` entry point later as another format value.
- Changing DXF output. It stays as-is, a CAD export, not an image.
- Editing or filling existing PDFs; PDF import.
- Animated or multi-frame image formats.

## Constraints

- Four targets: Windows, macOS, Linux, WASM. Anything needing a native library per OS
  (PDFium, Core Graphics, GDI+, WIC) is out of the cross-platform path.
- Pure-Rust dependencies preferred; WASM must build with no C toolchain surprises.
- Typst pinned `=0.15.0`; upgrades are deliberate.
- Fonts are embedded in the binary (report fonts today are looked up from the installed
  system by `report_fonts.rs`; that goes away for cross-platform formats).
- Release binary size must be measured, not assumed (Typst plus fonts is large).

## Formats

| Format | Kind | Decision | Why |
|---|---|---|---|
| PDF | vector, paged | Yes, via Typst (`typst-pdf`) | Replaces the hand-written writer; layout, tables, fonts come with Typst |
| SVG | vector, single image | Yes, keep `export_svg.rs` | Already exists, deterministic text, also the embed format for Typst |
| PNG | raster | Yes | Default raster; lossless, supports transparency |
| JPEG | raster | Yes | Typical for sharing/attachments; no alpha, so a background colour is required |
| WebP | raster | No for now | Little user demand in appraisal workflows; adds an encoder and a second raster test matrix. Revisit if a web viewer asks for it |
| TIFF | raster | No for now | Used by some legacy document systems but large, many variants (compression, bit depth), and nothing asks for it. Add only on a concrete request |
| EMF / WMF | vector, Windows | Only as the Appraise-It bridge output, kept separate | Needed by the Windows-only Appraise-It integration; not a user-facing format and not offered in the dialog |
| DXF | CAD | Unchanged | Not an image; listed in the dialog only because it is an export |

Rule for adding a format later: it must run on all four targets, come from the single
scene, and have a golden test before it appears in the dialog.

## One render pipeline

The drift risk is real today (two drawers for SVG and PNG, plus a third resvg path in the
app). The design removes it by giving every format one source of truth for the drawing and
one for the page.

```
 document -> ExportPage(s) (existing, UI-free scene)
                |
                +--> sketch SVG (export_svg.rs)  ---- the one drawing ----+
                |                                                          |
   single-image outputs                                   paged outputs   |
   SVG: the string itself                                 Typst document: |
   PNG/JPEG: SVG -> resvg/tiny-skia -> encode             title block, <--+ embeds the SVG
                                                          tables, comments
                                                          PDF: typst-pdf
                                                          page rasters: typst-render
```

Decisions:

1. **The sketch drawing is produced once, as SVG, by `export_svg.rs`.** It is the
   authoritative vector drawing for every format.
2. **Paged output (PDF, and page rasters for Print Preview or "PDF page as PNG") comes
   from Typst.** The Typst template embeds the SVG with `image(bytes, format: "svg")`, so
   geometry stays vector in the PDF. `typst-render` draws pages from the compiled document,
   never from parsed PDF bytes.
3. **Single-image outputs (SVG, PNG, JPEG) do not go through Typst.** A bare drawing needs
   no layout, and a Typst round trip would add cost and a margin/box model for no benefit.
   PNG and JPEG rasterise the same SVG with `resvg` (which uses `tiny-skia`, already a
   dependency) using the bundled fonts, then encode.
4. **Retire the hand-drawn `render_png` path** once SVG-to-raster output matches it in
   golden tests. After that, geometry exists in exactly one drawer (`export_svg.rs`), and
   the two rasterisers (resvg for single images, `typst-render` for pages) both consume that
   same SVG, so they cannot disagree about shapes. They can still differ in anti-aliasing
   details; golden tests allow a small pixel tolerance for that, not for geometry.
5. **Fonts are one bundled set**, loaded into both the Typst `World` and the resvg font
   database from the same embedded bytes. This also replaces the system-font loading in
   `report_sketch.rs::raster`.
6. **Open item for the spike:** `export_png`'s extras (reference-image backdrop,
   `saveImageWithSketch`) must be expressible in the SVG (an embedded `<image>`), or
   PNG keeps a thin tiny-skia composite step. The spike decides; the SVG route is preferred.

Appraise-It bridge: stays in `sketch-app` (Windows only). It continues to build its host
page SVG and produce the metafile the Appraise-It helper expects. It should reuse the shared
bundled fonts and `export_svg` drawing, but its 2615 x 4202 aspect and metafile writer are
bridge-specific and not part of `sketch-core`'s public export API.

## User-facing option

Export dialog (replaces the separate Export SVG / PNG / DXF / report entries with one
dialog; Print Preview keeps its own entry but uses the same engine):

- **Format:** PDF, SVG, PNG, JPEG, DXF. Last choice remembered per user.
- **Page / area:** all sketch pages, current page, or a page range (PDF); for image formats,
  one file per page (named `name-page-N.ext`) or the current page only. Area is the sketch
  bounds plus margin (the existing 40-unit margin); an optional "fit to paper" for PDF.
- **Size (raster only):** presets (Screen 1600 px wide, Print 300 DPI, Large 600 DPI) plus a
  custom width and height or DPI. Shows resulting pixels and a warning above a pixel cap.
  Replaces the free-text `"WxH"` image-size string.
- **JPEG quality:** slider, default 90. Hidden for other formats.
- **Background:** transparent (PNG and SVG only), white, or custom colour. JPEG forces an
  opaque colour (white by default) and says so.
- **Include area table / report content:** on for PDF by default; for image formats it adds
  the area summary and breakdown below the drawing (the same block `host_page` already draws).
  Off gives the drawing alone.
- **Paper (PDF):** paper size and orientation from the existing `ReportLayout`.

Options that only apply to some formats are hidden or disabled, never silently ignored.

### Headless API in `sketch-core`

A single module (suggested `sketch_core::export`) with these names:

- `ExportFormat`: `Pdf`, `Svg`, `Png`, `Jpeg`, `Dxf` (non-exhaustive; `Sketchy` joins later).
- `ExportOptions`: format; `pages` (`All`, `Current`, `Range`); `raster` (`RasterSize`:
  `Pixels{w,h}`, `Width(px)` or `Dpi(n)`); `jpeg_quality`; `background` (`Transparent`,
  `Color`); `include_area_table`; `paper` / `ReportLayout` for PDF; plus the existing drawing
  style settings (wall colours and widths, label font, text colour) carried as a nested
  `DrawingStyle` so they are not mixed up with output settings.
  Note: the name `ExportOptions` is already taken by the drawing-style struct in
  `export_model.rs`; rename that one to `DrawingStyle` as part of this work.
- `ExportInput` (or the document plus subject): the pages, area codes, symbol library and
  `ReportSubject` the exporters need today.
- `export(input, options) -> Result<ExportOutput, ExportError>`, where `ExportOutput` is a
  list of named files (`name_suffix`, `mime`, `bytes`): one entry for most formats, one per
  page for images.
- `suggested_file_name(...)`, `supported_formats()` for the UI.
- `preview_pages(input, options) -> Vec<RasterPage>` for Print Preview, using `typst-render`.

No file system, no UI types, no global state. The CLI or a future web viewer calls the same
function. `ExportError` covers: nothing to export, size over cap, font missing (should be
impossible with bundled fonts), render failure.

## Testing

- **PDF text checks:** extract text from the generated PDF and assert the report content
  (subject block, area labels, totals, comments). Also assert page count and that fonts are
  embedded. Fast, exact, catches layout regressions without images.
- **Golden images:** render each fixture (single family, condo, commercial, a curved/solid
  wall case) to PNG at a fixed size and compare with committed goldens with a small
  per-channel tolerance and a maximum mismatch ratio; write a diff image on failure.
  Page rasters come from `typst-render`, drawing rasters from resvg.
- **SVG:** compare normalised SVG text against goldens (exact, since it is generated).
- **JPEG:** decode and compare against the PNG with a looser tolerance (lossy); assert
  dimensions and no alpha.
- **Determinism test:** export twice (and with PDF metadata date fixed) and assert equal bytes.
- **Parity test during migration:** old `export_pdf.rs` versus Typst output for the same
  fixtures, page by page, until the old writer is deleted.
- **Cross-target CI:** build and run the export tests on Windows, macOS, Linux; build-only
  for `wasm32-unknown-unknown`, plus one wasm test run if practical.
- **Bridge:** the Windows-only metafile tests stay in `sketch-app`.

## Risks

- **Binary size:** Typst, embedded fonts, resvg and encoders. Measure the release exe
  before and after; strip unused Typst features and subset fonts if needed.
- **Visual parity:** the current report is hand-tuned (form layouts, metafile sizes). Expect
  to iterate the Typst template against goldens; some pixel-level differences may be
  accepted by the owner.
- **Anti-aliasing differences** between `resvg` and `typst-render` mean rasters from the two
  paths are close, not identical. Keep one path per format and avoid mixing in one file.
- **Typst version churn:** pin `=0.15.0`; upgrade as a deliberate task with golden review.
- **Compile time and memory in WASM** for large documents; set a pixel/size cap.
- **Large rasters:** 600 DPI legal pages are big; the pixel cap and warning are required.
- **Text in SVG/PNG:** SVG text depends on the viewer's fonts when opened elsewhere. Option
  to convert text to outlines for portable SVG is worth evaluating in the spike.
- **Bridge coupling:** the Appraise-It helper's fixed aspect must not leak into the general export
  options.

## Open questions for the owner

1. Should the area table appear in PNG/JPEG exports by default, or only the drawing?
2. Multi-page sketches to images: one file per page, or a single stacked image option?
3. Is transparent-background PNG still needed, or is white the better default?
4. Is a PDF/A or other archival profile required for the PDF (Typst can target it)?
5. Is outlined-text SVG wanted for portability?
6. Any known downstream system that needs TIFF or WebP now (this would change the table)?

## Next step

**Next step — implementation plan not written yet. It must be written separately by a
subagent (superpowers:writing-plans), outside the main session.**

## Owner decisions (2026-11-12)

1. Raster and vector image exports (PNG, JPEG, SVG) include the area tables and report
   content by default, the same as the PDF, not just the drawing.
2. Multi-page sketches exported as images produce one file per page.
3. PNG background defaults to white.
4. PDF/A or another archival profile: undecided. Default is plain PDF; keep the Typst
   PDF options open so PDF/A can be added as an option later without redesign.
5. Outlined-text SVG: undecided. Default keeps real text (searchable, smaller); outlined
   text may be added later as an export option.
6. TIFF/WebP: no request — stays "not now".
