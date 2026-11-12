#!/usr/bin/env bash
# tools/package.sh — build the release exe and lay out a runnable folder:
#   dist/sketch-app.exe
#   dist/symbols.json      <- the EXTERNAL symbol library (never embedded;
#                             sketch_core::symbols::resolve finds it beside
#                             the exe). Replace it to update the library
#                             without rebuilding.
set -eu
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
# A release must include the Apex compatibility component. For an Apex-less
# dev package, run with SKETCH_REQUIRE_APEX=0.
export SKETCH_REQUIRE_APEX="${SKETCH_REQUIRE_APEX:-1}"
cargo build --release -p sketch-app
mkdir -p dist
cp target/release/sketch-app.exe dist/
cp assets/symbols.json dist/
ls -l dist
