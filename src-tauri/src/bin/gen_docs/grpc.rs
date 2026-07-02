//! gRPC reference generator.
//!
//! Reads the `FileDescriptorSet` `build.rs` already compiles from
//! `proto/maslow/v1/*.proto` (`src/generated/maslow_descriptor.bin`) via the
//! `prost-reflect` crate, and renders one markdown page per service. This is
//! the actual compiled descriptor, not a re-parse of the `.proto` text, so it
//! can never describe an RPC or field shape the server doesn't really have.
//!
//! `prost_build::Config` passes `--include_source_info` to `protoc` by
//! default (only `skip_source_info()` turns it off, which `build.rs` does not
//! call), so the descriptor set already retains the `.proto` files' leading
//! `//` doc comments; no `build.rs` change was needed to make those available
//! here.

use prost_reflect::{
    DescriptorPool, EnumDescriptor, EnumValueDescriptor, FieldDescriptor, Kind, MessageDescriptor, MethodDescriptor, ServiceDescriptor,
};
use std::collections::BTreeMap;
use std::path::Path;

use crate::util;

/// (docs directory slug, fully-qualified proto service name), one per
/// `proto/maslow/v1/*.proto` file that declares a service (`common.proto`
/// declares none: it is a shared-types file, pulled in transitively wherever
/// a message it defines, e.g. `MachineStatus` or `Anchors`, is referenced).
pub const DOMAINS: [(&str, &str); 5] = [
    ("machine", "maslow.v1.MachineService"),
    ("job", "maslow.v1.JobService"),
    ("config", "maslow.v1.ConfigService"),
    ("files", "maslow.v1.FilesService"),
    ("calibration", "maslow.v1.CalibrationService"),
];

pub struct GrpcModel {
    pool: DescriptorPool,
}

pub fn load() -> GrpcModel {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/generated/maslow_descriptor.bin");
    let bytes = std::fs::read(path).unwrap_or_else(|e| {
        panic!(
            "failed to read the compiled proto descriptor set at {path}: {e}\n\
             run `cargo build` (or `cargo test`) in src-tauri/ at least once first so build.rs \
             regenerates it: this generator reads the same build artifact the app itself compiles \
             from proto/maslow/v1/*.proto, not the .proto text directly."
        )
    });
    let pool = DescriptorPool::decode(bytes.as_slice()).expect("decode maslow_descriptor.bin as a FileDescriptorSet");
    GrpcModel { pool }
}

impl GrpcModel {
    /// Find the gRPC method matching an MCP tool's snake_case name, if any.
    /// Tool names are the exact snake_case form of the RPC's PascalCase name
    /// (`get_snapshot` <-> `GetSnapshot`), the same convention the HTTP
    /// handler functions use, so a plain case conversion is enough: no
    /// hand-maintained mapping table to drift out of sync.
    pub fn method_for_tool_name(&self, tool_name: &str) -> Option<(String, MethodDescriptor)> {
        let pascal = util::snake_to_pascal(tool_name);
        for (_, service_full_name) in DOMAINS {
            let service = self.pool.get_service_by_name(service_full_name)?;
            let found = service.methods().find(|m| m.name() == pascal).map(|m| (service.name().to_string(), m));
            if let Some(result) = found {
                return Some(result);
            }
        }
        None
    }
}

pub fn write(model: &GrpcModel, out_dir: &Path) {
    for (domain, service_full_name) in DOMAINS {
        let service = model
            .pool
            .get_service_by_name(service_full_name)
            .unwrap_or_else(|| panic!("service {service_full_name} not found in the compiled descriptor set"));
        let md = render_service(domain, &service);
        util::write_file(&out_dir.join(format!("{domain}.md")), &md);
    }
}

fn leading_comment(fdp: &prost_reflect::prost_types::FileDescriptorProto, path: &[i32]) -> Option<String> {
    let info = fdp.source_code_info.as_ref()?;
    info.location
        .iter()
        .find(|l| l.path == path)
        .and_then(|l| l.leading_comments.clone())
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
}

// `MessageDescriptor`, `EnumDescriptor` and `ServiceDescriptor` expose
// `parent_file_descriptor_proto()` directly; `MethodDescriptor`,
// `FieldDescriptor` and `EnumValueDescriptor` don't, so these three route
// through `.parent_file().file_descriptor_proto()` instead.
fn method_comment(m: &MethodDescriptor) -> Option<String> {
    let file = m.parent_file();
    leading_comment(file.file_descriptor_proto(), m.path())
}

fn field_comment(f: &FieldDescriptor) -> Option<String> {
    let file = f.parent_file();
    leading_comment(file.file_descriptor_proto(), f.path())
}

fn enum_value_comment(v: &EnumValueDescriptor) -> Option<String> {
    let file = v.parent_file();
    leading_comment(file.file_descriptor_proto(), v.path())
}

enum TypeDoc {
    Message(MessageDescriptor),
    Enum(EnumDescriptor),
}

fn collect_types(msg: &MessageDescriptor, seen: &mut BTreeMap<String, TypeDoc>) {
    if seen.contains_key(msg.full_name()) {
        return;
    }
    seen.insert(msg.full_name().to_string(), TypeDoc::Message(msg.clone()));
    for f in msg.fields() {
        if f.is_map() {
            let entry = match f.kind() {
                Kind::Message(m) => m,
                _ => unreachable!("a map field's kind is always its synthetic entry message"),
            };
            collect_from_kind(entry.map_entry_value_field().kind(), seen);
            continue;
        }
        collect_from_kind(f.kind(), seen);
    }
}

fn collect_from_kind(kind: Kind, seen: &mut BTreeMap<String, TypeDoc>) {
    match kind {
        Kind::Message(m) => collect_types(&m, seen),
        Kind::Enum(e) => {
            seen.entry(e.full_name().to_string()).or_insert(TypeDoc::Enum(e));
        }
        _ => {}
    }
}

fn scalar_or_name(kind: &Kind) -> String {
    match kind {
        Kind::Double => "double".into(),
        Kind::Float => "float".into(),
        Kind::Int32 => "int32".into(),
        Kind::Int64 => "int64".into(),
        Kind::Uint32 => "uint32".into(),
        Kind::Uint64 => "uint64".into(),
        Kind::Sint32 => "sint32".into(),
        Kind::Sint64 => "sint64".into(),
        Kind::Fixed32 => "fixed32".into(),
        Kind::Fixed64 => "fixed64".into(),
        Kind::Sfixed32 => "sfixed32".into(),
        Kind::Sfixed64 => "sfixed64".into(),
        Kind::Bool => "bool".into(),
        Kind::String => "string".into(),
        Kind::Bytes => "bytes".into(),
        Kind::Message(m) => format!("[`{}`](#{})", m.name(), m.name().to_lowercase()),
        Kind::Enum(e) => format!("[`{}`](#{})", e.name(), e.name().to_lowercase()),
    }
}

fn field_type(f: &FieldDescriptor) -> String {
    if f.is_map() {
        let entry = match f.kind() {
            Kind::Message(m) => m,
            _ => unreachable!("a map field's kind is always its synthetic entry message"),
        };
        let k = scalar_or_name(&entry.map_entry_key_field().kind());
        let v = scalar_or_name(&entry.map_entry_value_field().kind());
        return format!("map<{k}, {v}>");
    }
    let base = scalar_or_name(&f.kind());
    if f.is_list() {
        format!("repeated {base}")
    } else if f.supports_presence() && f.containing_oneof().is_none() {
        format!("optional {base}")
    } else {
        base
    }
}

fn render_service(domain: &str, service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    out += &util::frontmatter(domain, &format!("{} (gRPC)", service.name()));
    out += &util::generated_banner("`proto/maslow/v1/*.proto`'s compiled `FileDescriptorSet`, via `prost-reflect`");

    if let Some(c) = leading_comment(service.parent_file_descriptor_proto(), service.path()) {
        out += &format!("{c}\n\n");
    }
    out += &format!("Full name: `{}`\n\n", service.full_name());

    let mut types: BTreeMap<String, TypeDoc> = BTreeMap::new();

    out += "## Methods\n\n";
    for method in service.methods() {
        out += &format!("### {}\n\n", method.name());
        if let Some(c) = method_comment(&method) {
            out += &format!("{c}\n\n");
        }
        let kind = match (method.is_client_streaming(), method.is_server_streaming()) {
            (false, false) => "unary",
            (false, true) => "server streaming",
            (true, false) => "client streaming",
            (true, true) => "bidirectional streaming",
        };
        let input = method.input();
        let output = method.output();
        out += &format!("- **Kind:** {kind}\n");
        out += &format!("- **Request:** [`{}`](#{})\n", input.name(), input.name().to_lowercase());
        out += &format!(
            "- **Response:** [`{}`](#{}){}\n\n",
            output.name(),
            output.name().to_lowercase(),
            if method.is_server_streaming() { " (a stream of these)" } else { "" }
        );
        collect_types(&input, &mut types);
        collect_types(&output, &mut types);
    }

    out += "## Types\n\n";
    out += "Every message and enum reachable from this service's requests and responses, including shared types defined in `common.proto`.\n\n";
    for doc in types.values() {
        match doc {
            TypeDoc::Message(m) => out += &render_message(m),
            TypeDoc::Enum(e) => out += &render_enum(e),
        }
    }
    out
}

fn render_message(m: &MessageDescriptor) -> String {
    let mut out = format!("### {}\n\n", m.name());
    if let Some(c) = leading_comment(m.parent_file_descriptor_proto(), m.path()) {
        out += &format!("{c}\n\n");
    }
    if m.fields().len() == 0 {
        out += "_No fields._\n\n";
        return out;
    }
    out += "| Field | Type | Description |\n|---|---|---|\n";
    for f in m.fields() {
        let ty = field_type(&f);
        let mut desc = field_comment(&f).unwrap_or_default();
        if let Some(oneof) = f.containing_oneof() {
            let note = format!("(part of oneof `{}`)", oneof.name());
            desc = if desc.is_empty() { note } else { format!("{desc} {note}") };
        }
        out += &format!("| `{}` | {} | {} |\n", f.name(), ty, util::md_escape(&desc));
    }
    out += "\n";
    out
}

fn render_enum(e: &EnumDescriptor) -> String {
    let mut out = format!("### {}\n\n", e.name());
    if let Some(c) = leading_comment(e.parent_file_descriptor_proto(), e.path()) {
        out += &format!("{c}\n\n");
    }
    out += "| Value | Number | Description |\n|---|---|---|\n";
    for v in e.values() {
        let desc = enum_value_comment(&v).unwrap_or_default();
        out += &format!("| `{}` | {} | {} |\n", v.name(), v.number(), util::md_escape(&desc));
    }
    out += "\n";
    out
}
