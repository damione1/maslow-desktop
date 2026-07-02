//! HTTP/JSON reference generator.
//!
//! There is no `google.api.http` proto annotation driving these routes (see
//! this generator's module doc in `main.rs` for why), so unlike the gRPC
//! generator this one cannot read a compiled descriptor. Instead it parses
//! the real route registrations directly out of `src/http/*.rs`
//! (`.route("/v1/...", get(handler))`-style calls) plus each handler
//! function's doc comment and `pb::` request/response types from its
//! signature. Reading the actual axum router source is what keeps this page
//! from drifting: there is no separate annotation to fall out of sync with
//! the handler that really runs.

use std::path::Path;

use crate::util;

/// (docs directory slug, `src/http/<slug>.rs`), the same five domains the
/// gRPC and MCP generators use.
pub const DOMAINS: [&str; 5] = ["machine", "job", "config", "files", "calibration"];

#[derive(Clone)]
pub struct HttpRoute {
    pub method: String,
    pub path: String,
    pub handler: String,
    pub doc: String,
    pub request_type: Option<String>,
    pub response_type: Option<String>,
    pub streaming: bool,
}

pub struct HttpModel {
    pub domains: Vec<(&'static str, Vec<HttpRoute>)>,
}

impl HttpModel {
    /// Find a route by its handler function name, across every domain. Used
    /// by the MCP generator to cross-reference a tool against the HTTP
    /// surface: handler function names and MCP tool names are both the
    /// snake_case operation name (`jog`, `get_snapshot`, `e_stop`, ...) by
    /// convention in this codebase, so an exact string match is enough.
    pub fn route_for_handler(&self, handler: &str) -> Option<&HttpRoute> {
        self.domains.iter().flat_map(|(_, routes)| routes).find(|r| r.handler == handler)
    }
}

pub fn load() -> HttpModel {
    let domains = DOMAINS
        .iter()
        .map(|domain| {
            let path = format!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/http/{}.rs"), domain);
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
            (*domain, parse_routes(&text))
        })
        .collect();
    HttpModel { domains }
}

pub fn write(model: &HttpModel, out_dir: &Path) {
    for (domain, routes) in &model.domains {
        let md = render_domain(domain, routes);
        util::write_file(&out_dir.join(format!("{domain}.md")), &md);
    }
}

/// Byte-offset balanced-paren scan: `s.as_bytes()[open_idx] == b'('`. Returns
/// the text strictly between the matching parens and the index just past the
/// closing paren. Slicing only ever happens at the byte offsets of ASCII `(`
/// and `)`, which are always valid UTF-8 char boundaries regardless of any
/// multi-byte content elsewhere in `s` (e.g. in a comment).
fn balanced(s: &str, open_idx: usize) -> (&str, usize) {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = open_idx;
    loop {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return (&s[open_idx + 1..i], i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
}

const VERBS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// Find every `verb(handler)` call at the top level of a `.route(...)`
/// call's argument list, e.g. both `get` and `patch` out of
/// `get(get_config_entry).patch(update_config_entry)`.
fn find_verb_handler_pairs(content: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for verb in VERBS {
        let needle = format!("{verb}(");
        let mut search_from = 0;
        while let Some(rel) = content[search_from..].find(&needle) {
            let idx = search_from + rel;
            let boundary_ok = idx == 0 || {
                let b = content.as_bytes()[idx - 1];
                !(b.is_ascii_alphanumeric() || b == b'_')
            };
            let open = idx + verb.len();
            if boundary_ok {
                let (handler, after) = balanced(content, open);
                let handler = handler.trim();
                if !handler.is_empty() && handler.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    pairs.push((verb.to_uppercase(), handler.to_string()));
                }
                search_from = after;
            } else {
                search_from = idx + needle.len();
            }
        }
    }
    pairs
}

fn extract_path(content: &str) -> Option<String> {
    let start = content.find('"')? + 1;
    let end = start + content[start..].find('"')?;
    Some(content[start..end].to_string())
}

fn parse_routes(text: &str) -> Vec<HttpRoute> {
    let mut routes = Vec::new();
    let needle = ".route(";
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find(needle) {
        let idx = search_from + rel;
        let open = idx + needle.len() - 1;
        let (content, after) = balanced(text, open);
        search_from = after;
        let Some(path) = extract_path(content) else { continue };
        for (method, handler) in find_verb_handler_pairs(content) {
            let (doc, request_type, response_type, streaming) = handler_details(text, &handler);
            routes.push(HttpRoute { method, path: path.clone(), handler, doc, request_type, response_type, streaming });
        }
    }
    routes
}

/// Pull `pb::TypeName` identifiers out of a signature fragment (either the
/// parameter list or the return type), in order of appearance.
fn find_pb_types(s: &str) -> Vec<String> {
    let mut found = Vec::new();
    let needle = "pb::";
    let mut search_from = 0;
    while let Some(rel) = s[search_from..].find(needle) {
        let idx = search_from + rel;
        let start = idx + needle.len();
        let end = s[start..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).map(|o| start + o).unwrap_or(s.len());
        if end > start {
            found.push(s[start..end].to_string());
        }
        search_from = end;
    }
    found
}

/// Doc comment, request/response `pb::` type, and streaming-ness for a
/// handler function, found by scanning the domain file's lines directly
/// (not the AST: this repo has no Rust parser dependency, and a line scan is
/// enough for this codebase's consistent handler shape).
fn handler_details(text: &str, handler: &str) -> (String, Option<String>, Option<String>, bool) {
    let lines: Vec<&str> = text.lines().collect();
    let marker_async = format!("async fn {handler}(");
    let marker_sync = format!("fn {handler}(");
    let Some(fn_line) = lines.iter().position(|l| {
        let t = l.trim_start();
        t.starts_with(&marker_async) || t.starts_with(&marker_sync)
    }) else {
        return (String::new(), None, None, false);
    };

    let mut doc_lines = Vec::new();
    let mut j = fn_line;
    while j > 0 {
        j -= 1;
        let t = lines[j].trim_start();
        if let Some(rest) = t.strip_prefix("///") {
            doc_lines.push(rest.trim().to_string());
        } else {
            break;
        }
    }
    doc_lines.reverse();
    let doc = doc_lines.join(" ");

    // Reconstruct enough of the signature to find the matching ')' after the
    // params and the '{' that opens the body, by joining a handful of
    // following lines (every handler in this codebase fits well within this
    // window) and running the same balanced-paren scan used for routes.
    let snippet_end = (fn_line + 20).min(lines.len());
    let snippet = lines[fn_line..snippet_end].join("\n");
    let open_paren = snippet.find('(').unwrap_or(0);
    let (params, after_params) = balanced(&snippet, open_paren);
    let body_start = snippet[after_params..].find('{').map(|o| after_params + o).unwrap_or(snippet.len());
    let return_ty = &snippet[after_params..body_start];

    let request_type = find_pb_types(params).into_iter().next();
    let response_type = find_pb_types(return_ty).into_iter().next();
    let streaming = return_ty.contains("Sse<");

    (doc, request_type, response_type, streaming)
}

fn render_domain(domain: &str, routes: &[HttpRoute]) -> String {
    let mut out = String::new();
    out += &util::frontmatter(domain, &format!("{} (HTTP)", title_case(domain)));
    out += &util::generated_banner(&format!("`src-tauri/src/http/{domain}.rs`'s real axum route registrations"));

    if routes.is_empty() {
        out += "_No routes found._\n";
        return out;
    }

    out += "| Method | Path | Description | Request | Response |\n|---|---|---|---|---|\n";
    for r in routes {
        let desc = util::md_escape(&r.doc);
        let req = r.request_type.as_deref().map(|t| format!("`{t}`")).unwrap_or_else(|| "_none_".to_string());
        let resp = match (&r.response_type, r.streaming) {
            (_, true) => "server-sent event stream".to_string(),
            (Some(t), false) => format!("`{t}`"),
            (None, false) => "_none_".to_string(),
        };
        out += &format!("| `{}` | `{}` | {} | {} | {} |\n", r.method, r.path, desc, req, resp);
    }
    out += "\n";
    out += "Request/response bodies are the JSON (pbjson) mapping of the identically named proto message: see the [gRPC reference](../grpc/index.md) for field-level detail. All routes require the `Authorization: Bearer <key>` header described in [Using the API](../using-the-api.md).\n";
    out
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
