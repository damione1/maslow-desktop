//! Small markdown/file-writing helpers shared by the grpc/http/mcp
//! generators below. No domain knowledge lives here.

use std::path::Path;

pub fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("failed to create {}: {e}", parent.display()));
    }
    std::fs::write(path, content).unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
    println!("wrote {}", path.display());
}

pub fn frontmatter(id: &str, title: &str) -> String {
    format!("---\nid: {id}\ntitle: \"{title}\"\n---\n\n")
}

/// Provenance note placed right after the frontmatter on every generated
/// page, so a reader lands here and knows not to hand-edit it, and knows
/// which real source the content was read from.
pub fn generated_banner(source: &str) -> String {
    format!(
        "> **Generated content.** This page is produced by `cargo run --bin gen_docs` \
        (`src-tauri/src/bin/gen_docs/`) reading {source}, re-run on every docs-site build. \
        Do not hand-edit it: fix the generator or the source it reads instead.\n\n"
    )
}

pub fn md_escape(s: &str) -> String {
    let s = s.replace('|', "\\|").replace('\n', " ");
    s.trim().to_string()
}

/// `send_realtime` -> `SendRealtime`. Matches the convention this repo
/// already uses across all three transports: an MCP tool / HTTP handler
/// function name is the exact snake_case form of the proto RPC's PascalCase
/// name.
pub fn snake_to_pascal(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
