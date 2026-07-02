# Maslow Desktop docs site

Documentation for the [Maslow Desktop](https://github.com/damione1/maslow-desktop) app and its HTTP/gRPC/MCP
control API, built with [Docusaurus](https://docusaurus.io/).

The API reference pages (`docs/api/http`, `docs/api/grpc`, `docs/api/mcp`) are generated, not hand-written: `npm
run build` and `npm start` both regenerate them first (see `scripts/gen-docs.mjs`), by running a small Rust binary
(`cargo run --bin gen_docs`, in `../src-tauri/`) that reads the real proto descriptor, the real HTTP route
registrations, and the live MCP tool registry. This requires the Rust toolchain the main app already needs.

## Installation

```bash
npm install
```

## Local development

```bash
npm start
```

Regenerates the API reference, then starts a local dev server with live reload.

## Build

```bash
npm run build
```

Regenerates the API reference, then builds the static site into `build/`, servable by any static host.

```bash
npm run serve
```

serves that `build/` directory locally, to check the production build before deploying.
