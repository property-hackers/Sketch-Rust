# Sketchy Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Sketch Rust's `.skx` storage with the open `sketchy.json` format, stored as versioned revisions in a SQLite `.sketchy` file, while legacy `.skx`/`.json` files still open.

**Architecture:** The app's in-memory state is serialised to a "save model" (`serde_json::Value`, the original web app's JSON shape) by `App::build_save_model` and restored by `App::apply_model` (`crates/sketch-app/src/main.rs`). We keep that seam. A new `sketch_core::sketchy` module converts save model ⇄ Sketchy document losslessly, computes RFC 8785 content hashes, and stores revisions in SQLite. The app's Save/Open call the store instead of `skx`.

**Tech Stack:** Rust 2021 workspace, `serde`/`serde_json`, `rusqlite` (feature `bundled`), `uuid` (feature `v7`), existing `sketch_core::skx::sha256_hex`.

**Spec:** `https://github.com/property-hackers/sketchy` — `SPEC.md` (draft 0.1) and `DICTIONARY.md`. Local clone: `~/Development/ph/sketchy`.

## Global Constraints

- Toolchain: run cargo with `ASDF_RUST_VERSION=1.97.0` exported (asdf shim). Tests: `cargo test -p sketch-core`, `cargo test -p sketch-app`. Both build on macOS; `build.rs` Windows-only steps are skipped there.
- Do not modify or remove Appraise-It integration code (the integration modules in `crates/sketch-app/src/`). The Appraise-It bridge keeps using `skx` internally.
- Do not change the save model shape that `build_save_model` produces or `apply_model` accepts. The converter adapts to it.
- Extension namespace for app-specific data in `extras`: `io.github.garmonbz.sketchrust` (constant `NS` in `sketchy/convert.rs`).
- Sketchy constants: `"format": "sketchy"`, `"format_version": "0.1"`, `length_unit` `"ft"` (the app's world unit is feet; wall `thickness` in the save model is INCHES → divide by 12).
- Store file extension `.sketchy`; SQLite `PRAGMA application_id = 0x534B4359` ("SKCY"), `PRAGMA user_version = 1`.
- Generator: `{"name": "Sketch Rust", "version": env!("CARGO_PKG_VERSION")}`.
- **NO COMMITS, NO PUSHES, NO PULL REQUESTS** in either repository. Leave every change uncommitted in the working tree for the owner to review. Never `git add`, `git commit`, `git stash` or `git push`.
- Rust practice: no `unwrap`/`expect` outside tests on file or user data; errors are `thiserror`-free plain enums implementing `Display` + `std::error::Error` (crate has no `thiserror`); `#[must_use]` on pure constructors; `cargo clippy -p sketch-core -- -D warnings` clean for new code.

## Review Focus

1. A `.sketchy` file written by another program, with fields Sketch Rust doesn't model, keeps those fields after open → edit → save (must-preserve). Test in Task 1 and Task 3.
2. Another program edits a wall's `bulge` or a space's points while our `extras` residual still holds the old `bowAngle`/points: the core value must win on open. Test in Task 3.
3. Save fails part-way (read-only file, disk error): the previous revision stays intact and the user sees an error, never a half-written file. Test in Task 5.
4. A `final` revision is never changed in place; saving edits creates a new revision whose parent is the final one. Test in Task 5.
5. Opening a non-Sketchy SQLite file, a truncated file or random bytes named `.sketchy` gives a clear error, never a panic. Test in Task 5 and Task 6.

---

### Task 1: Sketchy document types with must-preserve

**Files:**
- Create: `crates/sketch-core/src/sketchy/mod.rs`, `crates/sketch-core/src/sketchy/model.rs`
- Modify: `crates/sketch-core/src/lib.rs` (add `pub mod sketchy;`)
- Test data: copy `~/Development/ph/sketchy/examples/*.sketchy.json` to `crates/sketch-core/tests/data/sketchy/`
- Test: `crates/sketch-core/tests/sketchy_model.rs`

**Interfaces:**
- Produces: `sketchy::model::{Document, Revision, Structure, Level, Space, Shape, Wall, Opening, Feature, Void, Unit, Classification, ClassificationScheme, Presentation, Annotation}`; `Document::from_json(&str) -> Result<Document, SketchyError>`; `Document::to_value(&self) -> serde_json::Value`; `sketchy::SketchyError` (enum: `Json(String)`, `NotSketchy`, `UnsupportedVersion(String)`, `Store(String)`, `Convert(String)`), `sketchy::FORMAT = "sketchy"`, `sketchy::FORMAT_VERSION = "0.1"`.

Model notes: snake_case serde (spec keys are already snake_case, so no rename_all needed except `$schema` → field `schema` with `#[serde(rename = "$schema")]`). Every struct has `#[serde(flatten)] pub other: serde_json::Map<String, Value>` so unknown keys round-trip. Optional fields are `Option<T>` with `skip_serializing_if = "Option::is_none"`; vectors `#[serde(default, skip_serializing_if = "Vec::is_empty")]`. `extras`, `attributes` are `BTreeMap`/`Map` options. Points are `[f64; 2]`. `Shape` is `#[serde(tag = "type", rename_all = "snake_case")]` enum `Polygon{points, bulges: Option<Vec<f64>>, holes: Option<Vec<Ring>>}`, `Circle{center, radius}`, `Ellipse{center, radius_x, radius_y, rotation: Option<f64>}`, `Polyline{points, bulges}`, `Arc{center, radius, start_angle, end_angle}`. Dictionary fields stay `String` (unknown values must be readable as "other"); expose `Space::use_or_other(&self) -> &str` etc. only if a later task needs it (YAGNI). Presentation and annotations may be modelled as `serde_json::Value` maps where typing adds nothing.

- [x] **Step 1: Write failing tests** in `tests/sketchy_model.rs`:
  - `examples_round_trip_unchanged`: for each fixture, `Document::from_json(text)?.to_value() == serde_json::from_str::<Value>(text)?`.
  - `unknown_fields_are_preserved`: insert `"future_field": {"a": 1}` at document root and inside the first space; round trip keeps both.
  - `rejects_non_sketchy`: `{"format":"other",...}` → `Err(SketchyError::NotSketchy)`; `"format_version":"1.0"` → `Err(UnsupportedVersion)`; `"0.7"` (newer minor) → `Ok`.
- [x] **Step 2: Run** `cargo test -p sketch-core --test sketchy_model` → FAIL (module missing). *(RED reproduced after implementation by disabling the module — noted in the task report.)*
- [x] **Step 3: Implement** the types and `from_json` (parse, check `format == FORMAT`, accept major `0`).
- [x] **Step 4: Run** the test → PASS; `cargo test -p sketch-core` all green.
- [x] **Step 5:** Leave changes uncommitted (no `git add`/`git commit`). *(Nothing committed; the owner stages files themselves.)*

### Task 2: Canonical JSON and content hash

**Files:**
- Create: `crates/sketch-core/src/sketchy/canonical.rs`
- Test: `crates/sketch-core/tests/sketchy_hash.rs`

**Interfaces:**
- Consumes: `sketch_core::skx::sha256_hex(&[u8]) -> String`.
- Produces: `sketchy::canonical::canonical_json(v: &Value) -> String` (RFC 8785); `sketchy::canonical::content_hash(doc: &Value) -> String` returning `"sha256:<hex>"` of the canonical form of `doc` with root keys `revision` and `history` removed.

Number formatting follows RFC 8785 §3.2.2.3 (ECMAScript `Number.prototype.toString`): integral values print without a fraction (`10.0` → `10`), `-0` → `0`, shortest round-trip digits; exponent form only when `|x| >= 1e21` or `|x| < 1e-6` (`1e+21`, `1e-7`). Object keys sorted by UTF-16 code units; strings escaped per RFC 8785 (only `"`, `\`, and control chars; `\b \f \n \r \t` short forms; others `\u00XX` lowercase hex).

- [x] **Step 1: Write failing tests:**
  - `numbers_match_rfc8785`: `canonical_json(&json!([10.0, -0.0, 0.5, 1e21, 1e-7, 123456789.0, 14.4, 0.0104]))` == `"[10,0,0.5,1e+21,1e-7,123456789,14.4,0.0104]"`.
  - `keys_sorted_and_compact`: `canonical_json(&json!({"b":1,"a":{"d":[1,2],"c":"x"}}))` == `r#"{"a":{"c":"x","d":[1,2]},"b":1}"#`.
  - `string_escapes`: `json!("a\"b\\c\n\u{1}é")` → `"\"a\\\"b\\\\c\\n\\u0001é\""`.
  - `example_hashes_match_recorded`: for each fixture in `tests/data/sketchy/`, `content_hash(&v) == v["revision"]["content_hash"]`.
- [x] **Step 2: Run** `cargo test -p sketch-core --test sketchy_hash` → FAIL.
- [x] **Step 3: Implement** both functions.
- [x] **Step 4: Run** → PASS.
- [x] **Step 5:** Leave changes uncommitted (no `git add`/`git commit`). *(Nothing committed; the owner stages files themselves.)*

### Task 3: Convert save model ⇄ Sketchy (geometry core)

**Files:**
- Create: `crates/sketch-core/src/sketchy/convert.rs`
- Test data: `crates/sketch-core/tests/data/save-models/rich.json` (copy of `/private/tmp/claude-501/-Users-chasepursley-Development-af-Sketch-Rust/d28f814b-9296-46c6-9db8-9564bd53b63b/scratchpad/rich-save-model.json`)
- Test: `crates/sketch-core/tests/sketchy_convert.rs`

**Interfaces:**
- Consumes: Task 1 types.
- Produces: `sketchy::convert::to_sketchy(model: &Value, document_id: &str) -> Result<Document, SketchyError>`; `sketchy::convert::from_sketchy(doc: &Document) -> Result<Value, SketchyError>`; `pub const NS: &str = "io.github.garmonbz.sketchrust";`.

**Residual rule (lossless by construction):** for every save-model object we convert (document root, page, line, polygon), the keys we map into Sketchy core fields are removed from a clone of the object, and whatever remains is stored as `extras[NS]` (a JSON object) on the produced element. `from_sketchy` rebuilds the object as residual ⊕ values derived from core fields, with **core winning**: a residual value is only reused when the core value it came from is unchanged (for example keep residual `bowAngle` only if `tan(bowAngle·π/720)` equals the core `bulge` within `1e-9`; otherwise derive `bowAngle = atan(bulge)·720/π`, and drop residual `arcHeight` so `apply_model` recomputes it).

Mapping in this task:

| Save model | Sketchy |
|---|---|
| root `fileName` | `title`; `structures[0]` = `{id: "building", kind: "other", name: fileName}` |
| root `subjectInfo.propertyAddress` | `property.address` (keep whole `subjectInfo` in root residual too) |
| `pages[i]` | `structures[0].levels[i]` = `{id: "page-{i+1}", name: "Page {i+1}", grade: "above", sequence: i+1}` |
| `lines[]` `{id,start,end,type,thickness,bowAngle}` | `walls[]` `{id: "w-{id}", start:[x,y], end:[x,y], kind: exterior\|interior, thickness: inches/12, bulge: tan(bowAngle·π/720)}` (omit `bulge` when 0) |
| `polygons[]` `{id,points,type,name,factor,measureTo,subtractMode,subtractTargetId,attributes,edges[].bowAngle}` | `spaces[]` `{id: "a-{id}", name, shape: polygon(points, bulges from edges when any non-zero), classifications: [{scheme: "{NS}.area-codes", code: type}], area_factor (omit when 1), measured_to: interior→interior_face, exterior→exterior_face, center→centerline, else unspecified, attributes (omit when empty)}`; when `subtractMode == "always"` and `subtractTargetId` is set: `within: "a-{target}"`, `subtract: true` |

**Axes and curve direction.** The app draws in screen coordinates (+y down); Sketchy's frame is +y up. Convert every point with `y_sketchy = -y_app` (and back), so other readers see the plan right way up. Mirroring flips curve direction, so negate the bulge as well. First confirm how a positive `bow_angle` bends a line in the app (search `bow_angle` in `crates/sketch-core/src/geometry.rs`) and pin it with the `bowed_wall_bulge` test, so that a wall that bows left of start→end on screen also bows left of start→end in Sketchy's flipped frame. Record the reasoning in a short comment.

- [x] **Step 1: Write failing tests:**
  - `rich_model_round_trips_exactly`: `from_sketchy(&to_sketchy(&rich, "doc-1")?)? == rich`.
  - `walls_map_to_core`: rich line 5 → wall `w-5`, `start [0,0]`, `end [10,0]`, `kind "exterior"`, `thickness` ≈ `4/12`.
  - `y_axis_is_flipped`: a polygon `[(0,0),(10,0),(10,8)]` becomes points `[[0,0],[10,0],[10,-8]]`.
  - `bowed_wall_bulge`: a line with `bowAngle: 90` → `bulge` == `-tan(90·π/720)` (sign after flip; adjust only if the geometry check in this task proves the app's positive bow bends the other way, and record why in a comment).
  - `core_wins_over_stale_residual`: convert, change wall `w-5` bulge to `0.5` and a space point, convert back; resulting `bowAngle` == `atan(0.5)·720/π`, `arcHeight` absent, point changed.
  - `unknown_sketchy_fields_survive_save`: add `"future_field"` to a converted space, `from_sketchy` then `to_sketchy` again keeps it (store Sketchy-side unknowns under residual key `"sketchy_other"` and restore them).
  - `subtract_always_becomes_within`: polygon with `subtractMode:"always", subtractTargetId: 6` → space `within "a-6"`, `subtract true`.
- [x] **Step 2: Run** `cargo test -p sketch-core --test sketchy_convert` → FAIL.
- [x] **Step 3: Implement** `to_sketchy`/`from_sketchy` for document, pages, lines, polygons (other page arrays stay in the page residual for now).
- [x] **Step 4: Run** → PASS.
- [x] **Step 5:** Leave changes uncommitted (no `git add`/`git commit`). *(Nothing committed; the owner stages files themselves.)*

### Task 4: Convert annotations, layers and area codes

**Files:**
- Modify: `crates/sketch-core/src/sketchy/convert.rs`
- Test: `crates/sketch-core/tests/sketchy_convert.rs`

**Interfaces:**
- Consumes/produces: same functions as Task 3 (no new public API).

| Save model | Sketchy |
|---|---|
| `pages[i].labels[]` `{id,x,y,text,rotation}` | `presentation.annotations[]` `{id: "t-{id}", kind: "text", level: "page-{i+1}", position, text, rotation}` |
| `pages[i].symbols[]` `{id,symbolType,x,y,rotation,scale,flipH,wallId}` | `{id: "y-{id}", kind: "symbol", level, ref: {scheme: "{NS}.symbols", name: symbolType}, position, rotation, scale: [s,s], mirror: flipH ? "x" : "none", host: {wall: "w-{wallId}"} when wallId}` |
| `pages[i].dimensionChains[]` `{id,seedLineId,offset}` | `{id: "d-{id}", kind: "dimension", level, target: {wall: "w-{seedLineId}"}, offset}` (Sketchy `points` = the seed wall's start/end) |
| root `layers[]` `{id,name,visible,locked}` | `presentation.layers[]` `{id: "layer:{id}", name, visible, locked}`; element `layerId` → `presentation.element_layers` |
| root `areaCodes[]` `{code,name,description,includeInGLA,group,multiplier}` | `classification_schemes[0]` = `{scheme: "{NS}.area-codes", name: "Sketch Rust area codes", codes: [{code, label: name, use, finished, area_factor: multiplier (omit 1)}]}` |

Area code → `use`/`finished`: `includeInGLA` true → `use "living"`, `finished true`; else by `group` (case-insensitive): `GAR`→`garage`, `CARPORT`/`CP`→`carport`, `POR`/`PORCH`→`porch`, `DECK`/`DCK`→`deck`, `PATIO`→`patio`, `BAL`→`balcony`, `BSMT`/`BAS`→`storage` + `finished false`, otherwise `other`. Spaces whose code is in the table copy that `use`/`finished`.

- [x] **Step 1: Write failing tests:** `labels_become_text_annotations`, `symbols_become_symbol_annotations` (rich symbol 8 → `ref.name "door"`, `mirror "x"`, `host.wall "w-5"`), `chains_become_dimensions`, `layers_map_and_assign`, `area_codes_become_scheme` (rich has 32 codes; `GLA1` → `use "living"`, `finished true`), and keep `rich_model_round_trips_exactly` green.
- [x] **Step 2: Run** → FAIL.
- [x] **Step 3: Implement.**
- [x] **Step 4: Run** `cargo test -p sketch-core` → PASS. Also validate `to_sketchy(rich)` output with the repo validator: write it to a temp file and run `/usr/bin/python3 -I <scratchpad>/check_sketchy.py <file>` and `check_enums.py ~/Development/ph/sketchy/DICTIONARY.md <file>` → OK.
- [x] **Step 5:** Leave changes uncommitted (no `git add`/`git commit`). *(Nothing committed; the owner stages files themselves.)*

### Tasks 5–8 moved

The SQLite store, app Save/Open, the store section of the spec and wrap-up moved to
`docs/superpowers/plans/2026-11-12-sketchy-sqlite-store.md` (owner decision, 2026-11-12).
This plan ended with Tasks 4b (open `.sketchy.json`, optional Appraise-It integration build) and 4c (y-up axis
fix, display walls), added during execution; their briefs are in
`.superpowers/sdd/2026-11-12-sketchy-storage/`.

### Added during execution — verified complete

- [x] **Task 4b:** the app opens `*.sketchy.json` by content (never overwritten: Save → Save As), and the Appraise-It integration component is optional at build time (a build cfg plus an opt-in strict environment switch) so the app cross-compiles from macOS. Reviewed; fix round 1 closed.
- [x] **Task 4c:** removed the wrong y-flip (the app world is +y up like Sketchy); bulge, rotation and dimension-offset signs re-derived from the app's own functions; display walls created for wall-less spaces and not written back. Reviewed and approved. Verified by the owner opening the examples in a Windows build.
- [x] **review-work fix wave** (5 reviewers): id safety (duplicates rejected, range-checked, no overflow), document identity kept, array order preserved, dangling references, hostile-input limits (`sketchy::read`, 64 MiB, depth 64, non-finite), unshown-content summary, Save As refusal for Sketchy files, Import accepts Sketchy, Appraise-It integration required for release packaging, integration panel keeps Restore, build.rs rerun hygiene, new guard tests. Scoped re-review: all 14 addressed.

**Final verification (2026-11-12):** `cargo test -p sketch-core` → 179 + 52 + 9 + 9 passed; `cargo test -p sketch-app --bin sketch-app sketchy` → 8 passed; full `sketch-app` suite → 624 passed, 6 failed (the pre-existing macOS print/layout baseline); `to_sketchy(rich)` passes `check_sketchy.py` and `check_enums.py`; HEAD unchanged (no commits).
