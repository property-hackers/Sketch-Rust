---
name: iced-mcp
description: >
  Drive the Sketch Rust native iced app (sketch-app) through iced-mcp for
  interactive UI smoke, widget-id targeting, screenshots, and TEA message
  inspection. Use when setting up or using the iced MCP bridge, ICED_MCP=1,
  --features agent, connecting to port 9339, snapshot/click/type_text, or
  debugging Cursor/Claude/Codex MCP config for iced.
---

# Sketch Rust iced-mcp

Full guide: `docs/ICED_MCP.md`.

## When to use

- Smoke the Sketch Rust UI from an agent (menus, dialogs, trays, ledger).
- Verify widget ids after view changes.
- Capture screenshots of the canvas (not visible in accessibility snapshot).
- Diagnose “MCP not loading” / path issues for this app.

## Setup checklist

1. **Binary:** `cargo install iced-mcp --locked`. Point MCP `command` at the
   **real** path (`asdf which iced-mcp`), never an asdf/mise shim.
2. **MCP config:**
   - Cursor Agent CLI → `~/.cursor/mcp.json` (project `.mcp.json` alone is not enough).
   - Cursor Desktop / others → worktree `.mcp.json` and/or host MCP settings.
3. **Reload** the agent chat after config changes until the `iced` tool namespace
   is ready (tools: `connect`, `snapshot`, `click`, `type_text`, …).

## Start the app

From the repo root:

```bash
ICED_MCP=1 cargo run -p sketch-app --features agent
```

Expect: `iced-mcp: listening on 127.0.0.1:9339`.

Keep the process alive while driving it.

## Drive it

```text
status → connect { port: 9339 } → snapshot → state
```

Prefer **ids** over labels: `#menu`, `#save`, `#new-sketch`, `#tool-wall`,
`#area-chooser-apply`, `#area-row-{id}`, … Full table in `docs/ICED_MCP.md`.

For geometry / save without fighting the canvas or native dialogs:

- `run_app_action` `new_blank`
- `run_app_action` `draw_rect` with `{"width": 40, "height": 30}`
- `run_app_action` `save_to` with `{"path": "/abs/file.skx"}`

Canvas missing from `snapshot` is expected — use `screenshot` / x,y or
`draw_rect`.

`launch` can start the app from MCP; for local spikes, humans often run cargo
and agents only `connect`.

## Code rules when adding UI

- Stable kebab-case `.id(...)` on interactive widgets.
- iced 0.14 `Button` has no `.id()` — wrap with `container(...).id("…")`.
- Mutations go through `Message` / `update`.
- Paths: `crates/sketch-app/src/main.rs` (bridge), `crates/sketch-app/src/ui/` (ids).

## Quick troubleshooting

| Problem | Fix |
| --- | --- |
| No `iced` tools | Wrong MCP config file or shim path; restart chat |
| `connect` fails | App not listening — restart with `ICED_MCP=1` |
| Canvas missing from snapshot | Use `screenshot` / x,y |

Not for wasm builds. Not a product MCP surface.