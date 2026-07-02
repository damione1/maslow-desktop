---
id: building-and-running
title: Building and Running
sidebar_position: 2
---

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/)
  for your OS

## Develop

```bash
npm install
npm run tauri dev
```

`npm run tauri dev` runs the full app (Rust core + frontend); it's the primary dev loop.

## Build

```bash
npm run tauri build
```

The packaged app lands in `src-tauri/target/release/`.

## Other commands

```bash
npm run dev      # frontend only in a browser (no Tauri APIs; limited use)
npm run check    # svelte-kit sync + svelte-check (TS/Svelte type checking)
```

There is no JS test suite or linter configured; run `npm run check` before considering frontend work done. For
Rust, use `cargo build` / `cargo clippy` / `cargo test` from `src-tauri/`.

## Releasing

Trunk-based development: `master` is the single long-lived branch. Releasing is just tagging a commit on
`master`:

```bash
git tag vX.Y.Z && git push origin vX.Y.Z
```

The release workflow builds installers and creates a draft GitHub Release from the tag; the tag is the single
source of truth for the version number (it overwrites `src-tauri/tauri.conf.json` at build time).

## This docs site

`docs-site/` is a separate Node project (its own `package.json`, not merged with the root SvelteKit app). Its API
reference pages are generated, not hand-written; see [Using the API](../api/using-the-api.md) and the reference
pages themselves for how.

```bash
cd docs-site
npm install
npm run build   # regenerates the API reference, then builds the static site
npm start       # regenerates the API reference, then runs the dev server
```

Regenerating the API reference runs a small Rust binary (`cargo run --bin gen_docs` in `src-tauri/`), so building
this docs site also requires the Rust toolchain used by the main app.
