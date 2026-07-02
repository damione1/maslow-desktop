---
id: architecture
title: Architecture
sidebar_position: 1
---

Tauri 2 (Rust core) + SvelteKit + Svelte 5 + TypeScript. The frontend talks to FluidNC over WebSocket and HTTP;
the Rust side owns the connection, job streaming and the calibration state model.

The split is deliberate: **the Rust side owns the live connection and all machine-state truth; the frontend is a
thin reactive view** that listens for Tauri events and mirrors them into Svelte stores. Frontend code does not
replicate state-machine or reconciliation logic.

## Rust core (`src-tauri/src/`)

- `lib.rs` - registers every `#[tauri::command]` in one `invoke_handler`.
- `connection.rs` - the heart. A `connection_loop` supervisor task owns the WebSocket (`ws://<host>:81/`, i.e. web
  port + 1, subprotocol `arduino`) and survives automatic reconnects. Binary frames carry the GRBL line stream;
  text frames carry `key:value` control messages. It polls `?` (status), `$MINFO` and `$GSTATE` on tunable
  intervals (fast while moving, slow at idle), and burst-polls `$GSTATE` after any user action so the UI reflects
  state changes within milliseconds instead of waiting for the 1.5 second tick. All polling is suspended while
  `upload_active` is set (an SD/flash write stalls both ESP32 cores). The active streaming `Job` is owned here so
  it outlives reconnects.
- `grbl.rs` - parses `<...>` realtime status reports into `MachineStatus`. FluidNC may append `[GC:...]` after the
  report; the parser extracts the `<...>` substring. Maslow reports 5 axes (X, Y, Z, A, B).
- `maslow.rs` - Maslow telemetry (`MINFO:` JSON blob) and the calibration state machine (states 0-9, mirrors
  firmware `MaslowEnums.h`). `policy_for(state)` is the single source of truth for which user actions are allowed
  in each state. State (`Current state: N`) arrives separately from MINFO, via `[MSG:INFO:...]` or `$GSTATE`.
- `streaming.rs` - G-code job streaming via the GRBL character-counting protocol (127-byte RX budget). Only the
  `acked` cursor matters for resume; it is persisted to disk (every 25 acked lines plus on state change) so a job
  survives an app restart, not just a reconnect.
- `calibration.rs` - client-side anchor solver behind the `AnchorSolver` trait (a Levenberg-Marquardt
  implementation, a faithful port of the firmware's own solver). Exposed as `solve_calibration` for what-if
  waypoint-exclusion analysis without touching the machine.
- `http_api.rs` - HTTP client for `/command`, `/files`, `/upload` (file ops, ping, firmware version). `$`/`$/`
  commands return an empty HTTP body: their output is routed to the WebSocket instead, so config reads (`$CD`)
  must go over the socket, not HTTP.
- `toolpath.rs` - parses G-code into a polyline for the preview canvas.
- `grpc/`, `http/`, `mcp/` - the three control API transports. See [Using the API](../api/using-the-api.md).

## Frontend (`src/`)

- SPA mode: `+layout.ts` sets `ssr = false` (no Node server under Tauri); `adapter-static`.
- `src/lib/stores/` - one store module per domain (`connection`, `machine`, `maslow`, `job`, `ui`, `viewport`).
  Each exposes an `init*Listeners()` registered once in `+page.svelte`'s `onMount`. These listeners are the only
  place Tauri events become Svelte state.
- `src/routes/+page.svelte` - desktop shell (CSS-grid: topbar + persistent right control rail + 3-tab workspace +
  bottom console dock). `MobileShell.svelte` is the touch layout; `$layout` from `viewport.ts` switches between
  them.
- Tabs are kept mounted and hidden with `display:none` (not conditionally rendered) so the waypoint canvas and
  config panels keep their state and don't refetch on tab switch.

## Event contract (Rust `app.emit` to frontend `listen`)

Key events: `ws-state`, `grbl-line`, `machine-status`, `maslow-info`, `maslow-state`, `maslow-waypoint`,
`action-policy`, `stream-progress`, `config-dump` / `maslow-config` / `maslow-anchors` (all filled by one `$CD`
dump), `maslow-discord` (logged when firmware state reports disagree with the app's reconciliation).
`action-policy` is the reconciled allow-list (FluidNC state + calibration state + job state) computed in Rust and
emitted only on change; the UI gates buttons on it rather than re-deriving it.

## Conventions

- E-Stop is `0x18` (Ctrl-X soft reset), sent as a realtime byte; it stays reachable from app chrome at all times,
  never gated behind a tab.
- HTTP vs WebSocket for a machine interaction: `$`/`$/` commands go over WebSocket; `[ESP...]`/`/files`/`/upload`
  go over HTTP.
- Firmware-version-gated features (e.g. apply-tension limiting, v1.22+) hide or no-op on older firmware rather than
  error.
