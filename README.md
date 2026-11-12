# Sketch Rust

A native Windows floor-plan sketching program for appraisers: draw walls to scale,
define areas, and get living-area and building-area calculations. It is a Rust port
of the browser-based Appraisal Sketch app, and can stand in for Apex as the sketch
program Appraise-It Pro opens.

> **Status: development preview.** Appraise-It integration has been checked with
> Appraise-It Pro legacy and UAD 3.6 reports only. See
> [docs/APEX_REPLACEMENT.md](docs/APEX_REPLACEMENT.md) for what is and is not covered.

## Install

Download the latest release and use one of:

- **Installer** — `SketchRust-<version>-x64.msi` (a Windows Installer package built with
  WiX Toolset 5). Installs for the current Windows user (no administrator permission), with
  a licence page, a choice of folder, an optional desktop shortcut and a Start Menu shortcut.
  Later versions upgrade it in place, and it is removed from Windows Settings > Apps >
  Installed apps. Silent install: `msiexec /i SketchRust-<version>-x64.msi /qn`.
- **Portable zip** — `SketchRust-<version>-native-preview-<date>-windows-x64.zip`.
  Extract it anywhere and run `sketch-app.exe`.

The downloads are not code-signed, so Windows SmartScreen may warn before running
them. Each has a `.sha256` file to check the download against.

Requires 64-bit Windows 10 or 11.

## Using it

Help is built in: **Help > User Guide** covers drawing, areas, symbols, printing and
saving; **Help > Keyboard Shortcuts** (F1) lists every key; **Help > Integration
Guide** explains Appraise-It setup and use.

Quick start: press `E`, click a start point, type a length and press an arrow key for
each wall, press `A` to close the area, then pick what the area is in the Define Area
panel (`F4`).

Sketches save as `.sketchy` files in the open Sketchy format, and every save is kept as
a version. `.sketchy.json` files open too.

### Appraise-It integration

1. Install Sketch where it will stay.
2. Open **Help > Appraise-It Integration** and click **Enable integration**. Allow the
   Windows administrator prompt if one appears.
3. Restart Appraise-It Pro and use its usual Apex sketch button.

**Restore previous provider** in the same panel switches the report back to Apex.
Setup changes the shared Apex launch setting; it does not modify Appraise-It Pro.

## Build from source

Requires the Rust toolchain (stable, MSVC target) and the Windows SDK (`rc.exe`).

```
cargo build --release -p sketch-app
cargo test --workspace
```

- `tools\package.ps1 -NativePreview` builds the portable zip into `dist\`.
- `tools\installer.ps1` builds the zip and then `dist\SketchRust-<version>-x64.msi`
  (source in `installer\`). Run it once with `-FetchWix` to download WiX (it needs a .NET
  runtime, not the SDK); add `-Test` to install, check and remove the result.

## Layout

| Path | What it is |
|---|---|
| `crates/sketch-core` | Geometry, calculations, file formats, symbol import |
| `crates/sketch-app` | The application (iced UI), Appraise-It/Apex compatibility |
| `installer/` | The WiX source of the Windows installer |
| `assets/` | Symbol library, icons, cursors |
| `refs/` | The original web app's source and UI measurements the port is checked against |
| `docs/` | Release notes, build validation, compatibility and symbol-library notes |
| `tools/` | Packaging and test scripts |

`STATUS.md` is the running change ledger; `ACCEPTANCE.md` records the parity checks
against the original app.

## Contributing

Humans and AI agents welcome. Please:

- **Use test-driven development.** Write a failing test first, make it pass, then tidy up.
  Every change ships with tests.
- **Use an agent skills framework.** [Superpowers](https://github.com/obra/superpowers) is
  recommended: it gives agents brainstorming, planning, TDD and code-review workflows. Put
  designs in `docs/superpowers/specs/` and plans in `docs/superpowers/plans/`.
- **Keep changes focused,** and run `cargo test --workspace` and
  `cargo clippy --workspace -- -D warnings` before opening a pull request.
- **Review what your agent wrote** before you submit it. You're responsible for the change.

## License

MIT — see [LICENSE](LICENSE). The CADdillo residential symbols are CC0 1.0; the
Phosphor icons are MIT. See [docs/SYMBOL_LIBRARY.md](docs/SYMBOL_LIBRARY.md).
