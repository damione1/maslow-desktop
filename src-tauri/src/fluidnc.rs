// Generic FluidNC config tree handling.
//
// `$CD` (Config/Dump) returns the whole machine config as YAML. We flatten it
// into a list of leaf settings keyed by their `$/`-style path so the desktop can
// render every field and write changes back via `$/<path>=<value>` + `$CO`
// (the same write/persist path MaslowConfig already uses). The `$/` root does
// NOT dump everything (the firmware's RuntimeSetting matcher rejects an empty
// prefix), so `$CD` is the only way to enumerate the full tree generically.

use serde::Serialize;

/// Best-effort value type, used by the UI to pick an input widget.
#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigKind {
    Bool,
    Int,
    Float,
    Text,
}

/// One editable leaf of the config tree. `path` is the `/`-joined key path that
/// `$/<path>=<value>` expects (e.g. `axes/x/steps_per_mm`, `Maslow_Scale_X`).
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ConfigEntry {
    pub path: String,
    pub value: String,
    pub kind: ConfigKind,
}

fn classify(value: &str) -> ConfigKind {
    match value {
        "true" | "false" => ConfigKind::Bool,
        _ if value.parse::<i64>().is_ok() => ConfigKind::Int,
        _ if value.parse::<f64>().is_ok() => ConfigKind::Float,
        _ => ConfigKind::Text,
    }
}

/// Flatten a `$CD` YAML dump into editable leaf entries. Group headers (`key:`
/// with no value) build the path; `key: value` lines become leaves. Comments,
/// blank lines, sequence items and empty-valued keys are skipped. Indentation is
/// compared relatively, so the exact step size does not matter.
pub fn flatten_config(dump: &str) -> Vec<ConfigEntry> {
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut out = Vec::new();

    for raw in dump.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }
        let indent = line.len() - trimmed.len();

        // Leave any sections shallower-or-equal to this line.
        while matches!(stack.last(), Some(&(ind, _)) if ind >= indent) {
            stack.pop();
        }

        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let key = trimmed[..colon].trim();
        let value = trimmed[colon + 1..].trim();
        if key.is_empty() {
            continue;
        }

        if value.is_empty() {
            // Group header (or empty-valued leaf, which we don't surface).
            stack.push((indent, key.to_string()));
        } else {
            let mut path = String::new();
            for (_, k) in &stack {
                path.push_str(k);
                path.push('/');
            }
            path.push_str(key);
            out.push(ConfigEntry {
                kind: classify(value),
                path,
                value: value.to_string(),
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_values() {
        assert_eq!(classify("true"), ConfigKind::Bool);
        assert_eq!(classify("false"), ConfigKind::Bool);
        assert_eq!(classify("9"), ConfigKind::Int);
        assert_eq!(classify("1300"), ConfigKind::Int);
        assert_eq!(classify("0.000000"), ConfigKind::Float);
        assert_eq!(classify("-12.5"), ConfigKind::Float);
        assert_eq!(classify("NO_PIN"), ConfigKind::Text);
        assert_eq!(classify("gpio.36:low"), ConfigKind::Text);
        assert_eq!(classify("https://example.com"), ConfigKind::Text);
    }

    #[test]
    fn flattens_root_and_nested() {
        let dump = "\
board: Maslow
meta:
Maslow_calibration_grid_size: 9
Maslow_Scale_X: 1.000000
Maslow_Apply_Tension_Allow_Limiting: true
stepping:
  engine: Timed
  idle_ms: 240
kinematics:
  MaslowKinematics:
    tlX: 139.199997
    fixedZ: false
";
        let e = flatten_config(dump);
        let get = |p: &str| e.iter().find(|c| c.path == p);

        // Root scalar leaves keep their bare key as the path.
        assert_eq!(get("board").unwrap().value, "Maslow");
        assert_eq!(get("board").unwrap().kind, ConfigKind::Text);
        assert_eq!(get("Maslow_calibration_grid_size").unwrap().kind, ConfigKind::Int);
        assert_eq!(get("Maslow_Scale_X").unwrap().kind, ConfigKind::Float);
        assert_eq!(
            get("Maslow_Apply_Tension_Allow_Limiting").unwrap().kind,
            ConfigKind::Bool
        );

        // Nested leaves get the slash-joined path that `$/<path>=` expects.
        assert_eq!(get("stepping/engine").unwrap().value, "Timed");
        assert_eq!(get("stepping/idle_ms").unwrap().kind, ConfigKind::Int);
        assert_eq!(get("kinematics/MaslowKinematics/tlX").unwrap().value, "139.199997");
        assert_eq!(
            get("kinematics/MaslowKinematics/fixedZ").unwrap().kind,
            ConfigKind::Bool
        );

        // The empty-valued `meta:` group produces no leaf.
        assert!(get("meta").is_none());
    }

    #[test]
    fn dedents_correctly_between_sections() {
        // After a deep leaf, a new top-level key must not inherit the old path.
        let dump = "\
axes:
  x:
    steps_per_mm: 100
    motor0:
      step_pin: gpio.1
spi:
  miso_pin: gpio.13
";
        let e = flatten_config(dump);
        let paths: Vec<&str> = e.iter().map(|c| c.path.as_str()).collect();
        assert!(paths.contains(&"axes/x/steps_per_mm"));
        assert!(paths.contains(&"axes/x/motor0/step_pin"));
        assert!(paths.contains(&"spi/miso_pin"));
        // miso_pin must not be nested under axes/x/motor0.
        assert!(!paths.iter().any(|p| p.contains("motor0/miso_pin")));
    }

    #[test]
    fn skips_comments_and_blanks() {
        let dump = "# a comment\n\nname: Maslow S3\n  # indented comment\n";
        let e = flatten_config(dump);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].path, "name");
        assert_eq!(e[0].value, "Maslow S3");
    }
}
