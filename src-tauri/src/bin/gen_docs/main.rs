//! Generates the API reference markdown consumed by `docs-site/` for the
//! HTTP, gRPC, and MCP surfaces, each from a real source artifact rather than
//! hand-written prose:
//!
//! - **gRPC** (`grpc.rs`): the compiled `FileDescriptorSet` `build.rs`
//!   already produces from `proto/maslow/v1/*.proto`
//!   (`src/generated/maslow_descriptor.bin`), read with `prost-reflect`
//!   (pure Rust, no new toolchain). `protoc-gen-doc` was the more standard
//!   choice here, but it is a Go binary and this repo has no Go toolchain;
//!   `prost-reflect` stays inside the Rust/Node stack this repo already
//!   requires everywhere.
//! - **HTTP** (`http.rs`): the real axum route registrations in
//!   `src/http/*.rs`. `protoc-gen-openapiv2` (grpc-gateway's OpenAPI
//!   generator) was considered and rejected: it only produces an accurate
//!   REST mapping from `google.api.http` proto annotations, and this repo's
//!   `.proto` files carry none, because the HTTP routes were hand-written
//!   directly in axum rather than derived from proto annotations via
//!   grpc-gateway's transcoding. Retrofitting annotations onto ~40 RPCs to
//!   match the hand-written routes exactly would be busywork with a real
//!   risk of silently drifting from the actual axum implementation over
//!   time. Reading the axum source directly cannot drift, because it *is*
//!   the real behavior.
//! - **MCP** (`mcp.rs`): the live `rmcp::ToolRouter`s
//!   `McpServer::new` combines, via the newly exported
//!   `mcp::tool_routers_by_domain()`. This is the best-available source of
//!   truth: the exact registry the running server queries.
//!
//! Run via `cargo run --bin gen_docs -- <output-dir>`; `docs-site`'s
//! `npm run build` (and `npm start`) invoke this automatically as an npm
//! `pre`-script (see `docs-site/package.json`), so generated content can
//! never silently go stale relative to the real `.proto`/route/tool-registry
//! state.

mod grpc;
mod http;
mod mcp;
mod util;

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: gen_docs <output-dir>");
        std::process::exit(1);
    });
    let out_dir = std::path::PathBuf::from(out_dir);

    let grpc_model = grpc::load();
    let http_model = http::load();
    let mcp_model = mcp::load();

    grpc::write(&grpc_model, &out_dir.join("grpc"));
    http::write(&http_model, &out_dir.join("http"));
    mcp::write(&mcp_model, &out_dir.join("mcp"), &http_model, &grpc_model);

    println!("gen_docs: wrote HTTP/gRPC/MCP reference markdown to {}", out_dir.display());
}
