# Sketchy SQLite Store Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Store Sketch Rust sketches as versioned Sketchy revisions in a SQLite `.sketchy` file, make it the app's default Save/Open format, and specify the store in the Sketchy spec.

**Architecture:** Builds on the completed JSON plan (`2026-11-12-sketchy-storage.md`): `sketch_core::sketchy::{model, canonical, convert}` already convert the app save model to and from Sketchy and compute RFC 8785 content hashes, and the app already opens `*.sketchy.json`. This plan adds `sketchy::store` (rusqlite) and wires Save/Open to it.

**Tech Stack:** Rust 2021, `rusqlite =0.40.2` (feature `bundled`), `uuid` (feature `v7`), existing `sketchy` modules.

**Spec:** `https://github.com/property-hackers/sketchy` — `SPEC.md`, `DICTIONARY.md`.

## Global Constraints

Same as the JSON plan's Global Constraints (no commits/pushes; toolchain `ASDF_RUST_VERSION=1.97.0`; extras namespace `io.github.garmonbz.sketchrust`; generator name/version; Rust practice), plus:
- SQLite conventions (ideas from the owner's draft-store, no shared code): pin `rusqlite` exactly with `bundled`; `PRAGMA quick_check` on open, reject a corrupt file; `foreign_keys = ON`; every write is `BEGIN IMMEDIATE`; identify the file with `application_id 0x534B4359` + `user_version 1` (GeoPackage-style), not a migrations table.
- The app's world frame is +y up; Sketchy coordinates equal app world coordinates (no flip).

## Carry-over requirements from the JSON plan's reviews

- Task 6 must make Sketchy-only data survive editing: add `#[serde(flatten)]` unknown-key maps to `SketchLine`, `Polygon`, `PlacedText`, `PlacedSymbol`, `ChainRecord` and `Page`, keep root-level unknown keys (`sketchy_other`, `parked_annotations`, …) on `App` and re-emit them in `build_save_model`; strip `sketchy_id` from copied/pasted elements (or let the converter re-issue duplicates).
- Task 6 must add an app-level test: open `single-family.sketchy.json`, save, reopen the store head → level `l1` still has 0 walls (display walls are not written back) and the spaces are unchanged.
- Save As from a Sketchy file must not write save-model JSON into a `*.sketchy.json` name.

- Document identity: the app's `build_save_model` does not carry the root `sketchy_other`, so the `document_id`/`parent_revision` parked by `from_sketchy` are lost after an app round trip. The store must keep the document id and head revision itself (it does via `meta`), and the app must pass the store's id to `to_sketchy(model, Some(id))`.
- Decide who re-emits `history` when exporting a single `*.sketchy.json` (SPEC §15.3): the store, from its revisions table.
- Volatile save-model keys (`savedAt`, `settings.sidebar`) change the content hash on every save; the store must hash excluding them (or `to_sketchy` must move them out of the hashed content) so unchanged saves don't create revisions.

## Review Focus

(See the JSON plan's Review Focus items 3–5: failed saves keep the previous revision, final revisions never change in place, non-Sketchy/corrupt files give clear errors.)

---

### Task 5: SQLite revision store

**Files:**
- Modify: `crates/sketch-core/Cargo.toml` (add `rusqlite = { version = "=0.40.2", features = ["bundled"] }`, `uuid = { version = "1", features = ["v7"] }`)
- Create: `crates/sketch-core/src/sketchy/store.rs`
- Test: `crates/sketch-core/tests/sketchy_store.rs` (temp dirs via `std::env::temp_dir()` + unique names; crate has no `tempfile`)

**Interfaces:**
- Consumes: Task 1 `Document`, Task 2 `content_hash`.
- Produces:
  - `pub struct Store` with `Store::create(path: &Path, doc: Document, info: &CommitInfo) -> Result<Store, SketchyError>` (fails if path exists), `Store::open(path: &Path) -> Result<Store, SketchyError>`, `store.document_id(&self) -> &str`, `store.head(&self) -> Result<Document, SketchyError>`, `store.commit(&mut self, doc: Document, info: &CommitInfo) -> Result<CommitOutcome, SketchyError>`, `store.history(&self) -> Result<Vec<RevisionSummary>, SketchyError>`, `store.copy_to(&self, path: &Path) -> Result<Store, SketchyError>` (via `VACUUM INTO`).
  - `pub struct CommitInfo { pub message: Option<String>, pub author: Option<String>, pub status: RevisionStatus }`, `pub enum RevisionStatus { Draft, Final }`, `pub enum CommitOutcome { Created(String), Unchanged(String) }` (revision id), `pub struct RevisionSummary { pub id: String, pub parents: Vec<String>, pub sequence: u64, pub created_at: String, pub message: Option<String>, pub status: RevisionStatus, pub content_hash: String }`.
  - `pub fn new_id() -> String` (UUIDv7 string).
  - `pub fn is_store_bytes(first_bytes: &[u8]) -> bool` (starts with `b"SQLite format 3\0"`).

Schema (create inside one transaction):

```sql
PRAGMA application_id = 0x534B4359;  -- 'SKCY'
PRAGMA user_version = 1;
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
  -- rows: format='sketchy', format_version='0.1', document_id, head
CREATE TABLE revisions (
  id TEXT PRIMARY KEY, sequence INTEGER NOT NULL, parents TEXT NOT NULL,  -- JSON array
  created_at TEXT NOT NULL, author TEXT, message TEXT,
  status TEXT NOT NULL CHECK (status IN ('draft','final')),
  content_hash TEXT NOT NULL, document TEXT NOT NULL                       -- full sketchy.json
);
```

Attachments are deferred (YAGNI, owner decision 2026-11-12): v1 stores revisions only. A later
minor schema version may add an `attachments` table if images ever need to live in the file.

`commit`: compute `content_hash` of the new doc; if equal to head's hash and head is `draft` → `Unchanged(head)`; otherwise insert a revision with `parents = [head]`, `sequence = head.sequence + 1`, the doc's `id` forced to the store's `document_id`, its `revision` block filled (id `new_id()`, parents, sequence, `created_at` UTC RFC 3339 via `skx::iso8601_now()`, author, message, status, content_hash, generator from the doc), then update `meta.head` — all in one transaction. `open` checks `application_id`, `meta.format == "sketchy"`, `user_version == 1`; anything else → `SketchyError::NotSketchy`. Never panics on bad input.

- [ ] **Step 1: Write failing tests:**
  - `create_then_head_returns_document` (head has `revision.sequence 1`, `parents []`, matching `content_hash`).
  - `unchanged_commit_creates_no_revision` → `Unchanged`, `history().len() == 1`.
  - `changed_commit_links_parent` → sequence 2, parents `[r1]`.
  - `final_revision_is_never_rewritten`: commit status Final, then commit identical content as Draft → `Created`, parent is the final id, final row's `document` unchanged.
  - `failed_commit_leaves_head_intact`: make the file read-only (`std::fs::set_permissions`), commit → `Err`, reopen → head is still r1.
  - `open_rejects_foreign_sqlite_and_garbage`: a plain SQLite DB with a random table → `NotSketchy`; a 10-byte random file → `Err`, no panic; a truncated copy of a real store → `Err`.
  - `copy_to_keeps_history`.
- [ ] **Step 2: Run** `cargo test -p sketch-core --test sketchy_store` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS; full `cargo test -p sketch-core` and `cargo clippy -p sketch-core -- -D warnings` (new code) clean.
- [ ] **Step 5:** Leave changes uncommitted (no `git add`/`git commit`).

### Task 6: App Save and Open use the Sketchy store

**Files:**
- Modify: `crates/sketch-app/src/main.rs` (`SKETCH_OPEN_EXTENSIONS` ~L487, `SketchFileFormat` ~L510, `CurrentFile`, `open_sketch_path` ~L8064, `save_to_file` ~L8106), `crates/sketch-app/src/file_dialogs.rs`
- Test: alongside the existing file tests in `crates/sketch-app/src/main.rs` (see `save_model_round_trips_a_rich_sketch` and the tests near L9700 that write real files) — follow their pattern for temp paths and for bypassing native dialogs.

**Interfaces:**
- Consumes: `sketchy::convert::{to_sketchy, from_sketchy}`, `sketchy::store::{Store, CommitInfo, RevisionStatus, CommitOutcome, is_store_bytes, new_id}`.
- Produces: `SketchFileFormat::Sketchy`; Save writes revisions; Open reads head.

Behaviour:
- `SKETCH_OPEN_EXTENSIONS` gains `"sketchy"` first. Open dialog filter "Sketch" lists it; Save dialog filter becomes `("Sketchy sketch", ["sketchy"])` first and `("Legacy sketch", ["skx", "json"])` second. Default Save As name `"{file_name}.sketchy"`.
- Open: if `is_store_bytes(&bytes)` → `Store::open` → `head()` → `from_sketchy` → `apply_model`; `CurrentFile.format = Sketchy`. Errors → existing toast `"Could not open sketch: {e}"`.
- Save on a `Sketchy` file: `to_sketchy(&model, Some(store.document_id()))` → `commit` with `CommitInfo { message: None, author: None, status: Draft }`. On a new file: `Store::create(path, to_sketchy(&model, Some(&new_id())), …)`. Save As from a `Sketchy` file to a new path: `copy_to(path)` then commit.
- Save on a legacy `Skx`/`Json` file: route to Save As with the `.sketchy` default (never overwrite the legacy file silently). Keep explicit Save As to `.skx`/`.json` working for compatibility.
- Keep the store handle out of `App` (open per save; files are small). Keep `atomic_write` for legacy formats only.

- [ ] **Step 1: Write failing tests:**
  - `sketchy_save_then_open_round_trips` (rich sketch → save to temp `.sketchy` → fresh app opens it → same lines/polygons/labels/symbols as the existing rich test asserts).
  - `second_save_adds_a_revision_and_unchanged_save_does_not` (inspect with `Store::open(..).history()`).
  - `legacy_skx_save_goes_to_save_as` (open a `.skx`; `save_to_file(false)` must not rewrite it — assert bytes unchanged; use the existing dialog-bypass seam or factor `save_to_path(&mut self, target: CurrentFile)` out of `save_to_file` so tests call it directly).
  - `opening_garbage_sketchy_shows_error` (random bytes in `x.sketchy` → toast, no panic, `current_file` unchanged).
- [ ] **Step 2: Run** `cargo test -p sketch-app --bin sketch-app sketchy` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the full `cargo test -p sketch-app` → PASS (existing e2e and skx tests still green).
- [ ] **Step 5:** Leave changes uncommitted (no `git add`/`git commit`).

### Task 7: Spec — define the `.sketchy` SQLite store; README note

**Files:**
- Modify: `~/Development/ph/sketchy/SPEC.md` (new section "Store files" after §15; renumber later sections), `~/Development/ph/sketchy/README.md` (one line), `~/Development/af/Sketch-Rust/README.md` ("Using it": files save as `.sketchy`; `.skx` still opens).

Spec content: a `.sketchy` file is a SQLite 3 database (precedent: GeoPackage, MBTiles) with `application_id` `0x534B4359`, `user_version` 1, the `meta` and `revisions` tables exactly as in Task 5, rules: every `revisions.document` is a complete `sketchy.json` snapshot valid on its own; `meta.head` names the current revision; writers change the database only inside transactions; readers MUST check `application_id` and `meta.format`. A single `.sketchy.json` remains the interchange form (export = head snapshot plus `history` summaries).

- [ ] **Step 1:** Edit both READMEs and SPEC.
- [ ] **Step 2:** Check: SPEC.md and README.md in the sketchy repo name no vendors, products or people (read them; no output from a grep of the project's vendor list).
- [ ] **Step 3:** Leave both repositories uncommitted.

### Task 8: Branch wrap-up

- [ ] **Step 1:** `cargo test --workspace` → all pass; `cargo clippy -p sketch-core -- -D warnings` → clean.
- [ ] **Step 2:** `git status` shows only intended files changed; nothing committed, nothing pushed.

### Task 9: Version history UI (after Tasks 5–8)

Owner decision 2026-11-12: build this after all other tasks, on the same branch and PR. It
follows the common pattern of authoring apps (Word, Google Docs, Figma): a File menu entry next
to Save, a side panel listing versions, a read-only preview with a banner, and non-destructive
restore.

**Files:**
- Modify: `crates/sketch-core/src/sketchy/store.rs` (add `revision`), `crates/sketch-app/src/backstage.rs`
  (menu item), `crates/sketch-app/src/dock.rs` (Versions panel), `crates/sketch-app/src/titlebar.rs`
  and `main.rs` `save_state` (version in the pill), `main.rs` (messages, read-only view state, banner).
- Test: `crates/sketch-core/tests/sketchy_store.rs`, app tests in `main.rs`, plus e2e coverage
  following `src/e2e.rs` patterns where practical.

**Interfaces:**
- Produces: `Store::revision(&self, id: &str) -> Result<Document, SketchyError>` (any stored
  revision, with its `revision` block).

**Behaviour:**
- **File menu:** "Version History..." directly under "Save As...", icon `stack` (not the
  clock icon Recent Files uses). Disabled unless the current file is a `.sketchy` store. Also in
  command search.
- **Versions dock panel** (right-hand dock, alongside Areas/Text/Symbols/Calcs/Layers): newest
  first; each row shows `v<sequence>`, local date and time, author, a FINAL badge for final
  revisions, and the message if any; the current head is marked "(current)". Buttons at the top:
  **Mark as Final...** (optional note → `commit` with `status: Final`, message = note). Row
  actions: **Open**, **Save a copy...**.
- **Title-bar pill:** `Saved · v4`, `Unsaved · v4`, `Final · v5` (green) for `.sketchy` files;
  unchanged wording for other files.
- **Viewing an old version:** loads `revision(id)` read-only (editing tools disabled) with a
  banner across the top of the canvas:
  `Viewing v5 (Final, <date time>) — read-only   [Restore this version] [Save a copy...] [Back to latest]`.
  - **Restore this version:** commits that snapshot as a new draft revision on top of head with
    message `Restored from v5`; then returns to normal editing on the new head.
  - **Save a copy...:** Save As dialog (`.sketchy`), creating a new store whose first revision is
    that snapshot (new document id, `derived_from` = this document and revision).
  - **Back to latest:** reloads head.
- Ordinary Save asks nothing (no message prompt).
- Out of scope (YAGNI): side-by-side compare, branching, deleting versions, thumbnails.

- [ ] **Step 1: Write failing tests:** `revision_returns_any_stored_snapshot` (store);
  app tests: `version_history_lists_revisions_newest_first`, `opening_old_version_is_read_only`,
  `restore_creates_new_revision_on_top` (history grows by one, parent = previous head, message
  `Restored from vN`, old revision untouched), `mark_as_final_records_final_revision`,
  `save_a_copy_of_old_version_creates_new_document`, `pill_shows_version_and_final`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p sketch-core` and `cargo test -p sketch-app` (only the 6 known
  macOS baseline failures) → PASS; then repeat Task 8's checks (Windows cross build for the VM).
- [ ] **Step 5:** Leave changes uncommitted.

### Task 10: Update to Sketchy draft 0.2 (after Task 9)

Owner decision 2026-11-12: the spec moved to draft 0.2 (`property-hackers/sketchy` commit
`3b2e3d1`), which allows breaking changes within 0.x. Sketch Rust writes 0.2 and still reads
0.1 files.

**Files:** `crates/sketch-core/src/sketchy/{mod.rs, model.rs, convert.rs, store.rs}`, tests in
`crates/sketch-core/tests/`, fixtures in `crates/sketch-core/tests/data/sketchy/`, and any app
code that hard-codes `0.1`. Spec: `~/Development/ph/sketchy/SPEC.md` (0.2) and `DICTIONARY.md`;
diff the change with `git -C ~/Development/ph/sketchy diff 4941b6a 3b2e3d1`.

**Required:**
- **Version:** `FORMAT_VERSION = "0.2"`; writers write `"0.2"` (and the 0.2 `$schema` URL where a
  schema URL is written). Readers accept `0.1` and `0.2`; any other `0.x` → `UnsupportedVersion`
  (0.x minors may now break).
- **Bulge sign (breaking):** 0.2 uses DXF: positive bulge = counter-clockwise arc from start to
  end = bows to the RIGHT of travel (edge `[0,0]→[10,0]`, bulge `1` passes through `[5,-5]`).
  0.1 was positive = LEFT. The app's positive bow bows LEFT (+y) in world space (pinned earlier
  against `flatten_ring`). So for 0.2 `bulge = -tan(bow·π/720)`; when READING a 0.1 file, negate
  0.1 bulges into the 0.2 convention first. Update the pinned sign tests to cite SPEC 0.2 §4.2's
  `[5,-5]` example. Store: revisions written by this build are 0.2; reading an older 0.1 revision
  still works.
- **Curves mapped to the app's own curve model (owner: fix now, not later):**
  - Walls and area edges: app `bowAngle` (sweep in degrees, ±359°, sign = side) ⇄ Sketchy 0.2
    `bulge` exactly (`|bulge| = tan(|bow|·π/720)`, sign per DXF above); the app's Height curve
    mode is the same arc (`arcHeight = |bulge|·chord/2`) — keep `arcHeight` consistent.
  - **Circles exact:** a Sketchy `circle` space opens as a 2-point polygon with two 180° bowed
    edges (bulge ±1 in 0.2), not a 48-gon; display walls get the same bows. On save, if the
    two points and bows are unchanged, write the original `circle` back; otherwise write the
    polygon with bulges. Replace the existing 48-gon circle tests.
  - Ellipses: the app has no ellipse geometry — keep the 48-gon approximation with the original
    kept and restored when unchanged (as today).
  - Open `arc`/`polyline` spaces and annotations: preserved, not displayed (unchanged).
  - A bulge whose sweep would exceed the app's 359° limit (|bulge| > tan(359°/4)) must not be
    clamped silently: keep the original shape in the residual and approximate the edge for
    display, restoring the original on save if unchanged. Test.
- **Ceilings:** `ceiling_height`/`ceiling_sloped` → `ceiling {height, zones}` on spaces and
  `ceiling {height}` on levels. The app doesn't model ceilings: keep them preserved (residual),
  and when reading 0.1 convert `ceiling_height` to `ceiling.height` (drop `ceiling_sloped` into
  the residual) so written files are valid 0.2.
- **Stairs:** `counted_on` is removed — never write it; preserve new stair fields verbatim.
- **Edges:** polygon `edges` (`physical`/`virtual`) and `edge_measured_to` per edge, also on
  holes — preserve verbatim and keep them aligned when points are rebuilt (if a space's points
  change in the app so the per-edge arrays no longer line up, drop them rather than misalign).
  Display walls are still created for wall-less edges (virtual edges included) and never
  written back.
- **Openings:** `swing` may be `unknown`; `swing_into`, `hole`, skylight `shape` preserved.
- **Rings rule:** a writer that reverses a ring's point order must reverse its per-edge arrays
  and negate its bulges — check the converter never reverses rings; add a test if it can.
- **Fixtures:** replace `crates/sketch-core/tests/data/sketchy/*.sketchy.json` with the 0.2
  examples from `~/Development/ph/sketchy/examples/`; keep one 0.1 fixture (rename to
  `*-0.1.sketchy.json`) for the read-0.1 test. Re-run `example_hashes_match_recorded` — if a 0.2
  example's recorded `content_hash` doesn't match, report it (it's the spec repo's bug), don't
  "fix" the test.
- Validators: the scratchpad `check_sketchy.py`/`check_enums.py` target 0.1; update their
  dictionary checks only if needed to validate 0.2 output, and say what changed.

- [ ] **Step 1: Write failing tests:** `writes_format_version_0_2`, `reads_0_1_and_0_2`,
  `rejects_other_0x_versions`, `bulge_sign_follows_dxf_in_0_2` (app bow +90 on `[0,0]→[10,0]`
  → negative bulge; arc midpoint at +y), `reads_0_1_bulges_with_flipped_sign`,
  `ceiling_height_0_1_becomes_ceiling_object`, `edge_kinds_survive_round_trip`,
  `circle_space_opens_as_two_exact_half_circle_edges`, `circle_written_back_when_unchanged`,
  `oversized_bulge_is_not_silently_clamped`, `height_mode_arc_height_matches_bulge`,
  `per_edge_arrays_dropped_when_points_change`, plus all existing tests green on the 0.2
  fixtures.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** full `sketch-core` + `sketch-app` suites (6 macOS baseline failures only),
  clippy, Windows cross build.
- [ ] **Step 5:** Leave changes uncommitted.
