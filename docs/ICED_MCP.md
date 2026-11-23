# Sketch Rust — agent MCP guide

How LLM coding tools (Cursor, Claude Code, Codex, etc.) drive the native
`sketch-app` iced UI through [iced-mcp](https://crates.io/crates/iced-mcp).

This is **dev tooling**, not a product feature. The bridge is gated behind the
`agent` Cargo feature and `ICED_MCP=1`. Leave both off for normal runs.

Companion skill (same content, agent-oriented): `.agents/skills/iced-mcp/`.

---

## Architecture (two processes)

```text
┌─────────────────────┐  stdio MCP   ┌──────────────┐  TCP 127.0.0.1:9339  ┌──────────────────┐
│ Cursor / Claude /   │◄────────────►│  iced-mcp    │◄────────────────────►│ sketch-app       │
│ Codex agent         │              │  (server)    │                      │ + iced-mcp bridge│
└─────────────────────┘              └──────────────┘                      └──────────────────┘
```

1. **App bridge** — `sketch-app` built with `--features agent`, run with
   `ICED_MCP=1`. Listens on loopback (default **9339**).
2. **MCP server** — the `iced-mcp` binary. Agents talk to it over stdio; it
   attaches with `connect` / `launch`.

Prefer widget `.id(...)` values over visible labels when driving the UI.

---

## One-time setup

### 1. Install `iced-mcp`

```bash
cargo install iced-mcp --locked
```

Resolve the **real binary**, not an asdf/mise shim (Cursor often cannot run
shims):

```bash
asdf which iced-mcp
# e.g. ~/.asdf/installs/rust/<version>/bin/iced-mcp

command -v iced-mcp
```

### 2. Register the MCP server

**Cursor Agent CLI** loads **`~/.cursor/mcp.json`** (global). Project
`.mcp.json` alone is **not** enough for `cursor-agent`.

**Cursor Desktop / other hosts** may also read the worktree `.mcp.json`.

Minimal entry (use your real path):

```json
{
  "mcpServers": {
    "iced": {
      "command": "/ABS/PATH/TO/iced-mcp"
    }
  }
}
```

This worktree’s `.mcp.json` includes an `iced` entry for Desktop.

After editing MCP config: restart the agent chat / reload MCP so tools appear.

### 3. Confirm tools are visible

The agent should see an `iced` namespace with tools like `connect`, `snapshot`,
`click`, `type_text`, `state`, `run_sequence`, `status`.

---

## Run the app with the bridge

From the repo root:

```bash
ICED_MCP=1 cargo run -p sketch-app --features agent
# optional: path to a sketch file as argv
```

Success line:

```text
iced-mcp: listening on 127.0.0.1:9339
```

Notes:

- Keep the process running while agents drive it. Quitting the window drops
  the bridge; start it again before `connect`.
- Without `ICED_MCP=1`, `--features agent` still builds the bridge code but
  does not listen (plain app).

**Auth:** set a strong `ICED_MCP_TOKEN` whenever the bridge may see real
customer sketches — any local process can otherwise connect to
`127.0.0.1:9339` and read snapshots/messages. Pass the same `token` to MCP
`connect`. Prefer sample/fixture sketches for unauthenticated automation.

Optional: `ICED_MCP_PORT` for a non-default port.

---

## Agent playbook

### Connect and inspect

```text
1. status          — is anything already connected?
2. connect         — port 9339 (or launch if you want MCP to start the app)
3. snapshot        — widget tree with #ids
4. state           — app-authored summary (file, mode, selection counts)
```

`launch` can start the app for you (`command`, `cwd`, `env` with `ICED_MCP=1`).
For local work, humans often start the app; agents then `connect`.

Example launch env:

```json
{
  "command": "cargo run -p sketch-app --features agent",
  "cwd": "/ABS/PATH/TO/Sketch-Rust",
  "env": { "ICED_MCP": "1" }
}
```

### Prefer ids over labels

| Id | Role |
| --- | --- |
| `menu` / `open` / `save` / `export` | Top bar chrome |
| `new-sketch` / `blank-canvas` | Start screen |
| `tool-select` / `tool-wall` / `tool-inner` / … | Tool tray (every `TrayTool`) |
| `drawing-tool-tray` / `drawing-compact-tray` | Tray scroll containers |
| `area-row-{id}` | Areas ledger row |
| `area-name` / `area-apply` / `area-type-chooser` | Area inspector |
| `area-chooser-name` / `area-chooser-apply` / `area-chooser-cancel` | Area type chooser |
| `area-definition-search` / `area-chooser-scope-list` | Chooser search + list |
| `menu-search` | Menu search field |
| `ledger-areas` | Areas ledger scroll |
| `layer-rename-input` | Layer rename field |
| `print-pages` / `breakdown-list` / `laser-list` | Dialog lists |
| `room-labels-list` / `undo-list` / `guide-article-list` | More list panes |
| `area-types-list` | Area types editor list |
| `symbol-categories-list` / `symbol-grid-list` | Symbol library |
| `north-angle-input` | North angle field |

The sketch **canvas** does not appear in accessibility `snapshot` — use
`screenshot` / `inspect_point` + x/y, or `run_app_action` below for geometry.

### App actions (skip flaky UI)

With the bridge up, `app_actions` / `run_app_action`:

| Action | Args | Effect |
| --- | --- | --- |
| `new_blank` | none | Blank untitled sketch |
| `draw_rect` | `{"width": 40, "height": 30}` | Closed exterior rectangle (feet) |
| `save_to` | `{"path": "/abs/house.skx"}` | Save without the native picker |

Example house smoke: `new_blank` → `draw_rect` → click `#area-row-*` / `#area-type-chooser` → `save_to`.

### Verify

- `snapshot` — chrome / dialogs show expected labels and `#ids`.
- `state` — e.g. `file=… mode=Select polys=… sel_polys=… dirty=…`.
- `messages` — recent TEA messages.
- `diagnose` / `audit` — layout / semantics checks after UI changes.

---

## Adding targets in code

When you add interactive widgets agents should drive:

1. Prefer iced widgets that support `.id(...)` (`text_input`, `scrollable`,
   `container`, …). iced 0.14 `Button` has no `.id()` — wrap in
   `container(...).id("…")`.
2. Keep ids stable and kebab-case.
3. Route mutations through `Message` / `update` so `messages` / `state` stay
   meaningful.
4. Extend the `state(|app| …)` dump in `crates/sketch-app/src/main.rs` if
   agents need a new one-line summary field.

Relevant paths:

| Path | Role |
| --- | --- |
| `crates/sketch-app/Cargo.toml` | `agent` feature |
| `crates/sketch-app/src/main.rs` | Bridge attach when `ICED_MCP=1` |
| `crates/sketch-app/src/ui/` | Most widget ids |

---

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| No `iced` tools in agent | Register real binary in the MCP config the host actually loads; restart chat |
| Cursor Agent: tools missing after project `.mcp.json` only | Add the same entry to `~/.cursor/mcp.json` |
| `iced` namespace `error` / discovery failed | Retry tool discovery; restart agent; confirm `iced-mcp --help` works |
| `connect` fails | App must print `listening on 127.0.0.1:9339`; restart with `ICED_MCP=1` |
| Shim path fails under Cursor | Point `command` at `asdf which iced-mcp` real path |
| Canvas missing from `snapshot` | Expected — use `screenshot` / `inspect_point` + x/y |

---

## Out of scope

- Wasm builds do not use this bridge (native `agent` feature only).
- Product MCP for Sketch Rust is a separate design; do not conflate with
  iced-mcp.