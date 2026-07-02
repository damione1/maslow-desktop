#!/usr/bin/env node
// Regenerates docs/api/{http,grpc,mcp}/*.md from real source artifacts: the
// compiled proto descriptor, the real axum route registrations, and the live
// MCP tool registry (see src-tauri/src/bin/gen_docs/ for how). Wired as an
// npm "pre" script for both `build` and `start` (see package.json), so those
// commands always regenerate before Docusaurus reads the docs folder: the
// API reference can't silently go stale relative to the real app.

import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import path from 'node:path';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const srcTauriDir = path.resolve(__dirname, '..', '..', 'src-tauri');
const outDir = path.resolve(__dirname, '..', 'docs', 'api');

console.log(`[gen-docs] cargo run --bin gen_docs (in ${srcTauriDir}) -> ${outDir}`);

const result = spawnSync('cargo', ['run', '--quiet', '--bin', 'gen_docs', '--', outDir], {
  cwd: srcTauriDir,
  stdio: 'inherit',
});

if (result.error) {
  console.error(
    '[gen-docs] failed to run `cargo`. Is the Rust toolchain installed? ' +
      'See https://www.rust-lang.org/tools/install',
  );
  process.exit(1);
}
if (result.status !== 0) {
  console.error('[gen-docs] `cargo run --bin gen_docs` exited with a non-zero status; see output above.');
  process.exit(result.status ?? 1);
}

console.log('[gen-docs] done.');
