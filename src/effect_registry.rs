//! Registry of available shader effects.
//!
//! Scans a directory of `.wgsl` files, parses `@name`, `@param` headers,
//! and provides lookup by effect ID. Same pattern as `TransitionRegistry`.

use std::path::Path;

/// Definition of a single parameter exposed by an effect shader.
#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

/// A registered effect shader with metadata parsed from its header.
#[derive(Debug, Clone)]
pub struct EffectDef {
    /// Unique ID derived from the file stem (e.g. "bloom", "vignette").
    pub id: String,
    /// Display name from `@name` header, or title-cased file stem.
    pub name: String,
    /// Author from `@author` header, or empty.
    #[allow(dead_code)]
    pub author: String,
    /// Description from `@description` header, or empty.
    #[allow(dead_code)]
    pub description: String,
    /// Float parameters exposed by the shader, from `@param` headers.
    #[allow(dead_code)]
    pub params: Vec<ParamDef>,
    /// Raw WGSL shader source.
    pub shader_source: String,
    /// Whether this is a built-in effect (from application resources).
    #[allow(dead_code)]
    pub is_builtin: bool,
}

/// Registry of available effect shaders, scanned from a directory.
pub struct EffectRegistry {
    effects: Vec<EffectDef>,
    /// Simple fingerprint: concatenation of (id, shader_source) for change detection.
    fingerprint: u64,
}

impl EffectRegistry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self {
            effects: Vec::new(),
            fingerprint: 0,
        }
    }

    /// Scan a directory for `.wgsl` files and build the registry.
    pub fn scan(dir: &Path) -> Self {
        let mut effects = Vec::new();

        // Scan directory for .wgsl files.
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                log::warn!(
                    "Failed to read effects directory {}: {e}",
                    dir.display()
                );
                let fingerprint = Self::compute_fingerprint(&effects);
                return Self { effects, fingerprint };
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("wgsl") {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("Failed to read effect shader {}: {e}", path.display());
                    continue;
                }
            };

            let (header_name, author, description, params) = parse_header(&source);
            let name = if header_name.is_empty() {
                title_case_stem(&stem)
            } else {
                header_name
            };

            effects.push(EffectDef {
                id: stem,
                name,
                author,
                description,
                params,
                shader_source: source,
                is_builtin: false,
            });
        }

        // Sort effects alphabetically by name.
        effects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let fingerprint = Self::compute_fingerprint(&effects);
        Self { effects, fingerprint }
    }

    /// Re-scan the effects directory. Returns true if the registry changed.
    pub fn rescan(&mut self, dir: &Path) -> bool {
        let new = Self::scan(dir);
        if new.fingerprint != self.fingerprint {
            *self = new;
            true
        } else {
            false
        }
    }

    /// Simple fingerprint based on file count, IDs, and source lengths.
    fn compute_fingerprint(effects: &[EffectDef]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        for e in effects {
            e.id.hash(&mut hasher);
            e.shader_source.len().hash(&mut hasher);
            e.shader_source.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Look up an effect by ID.
    pub fn get(&self, id: &str) -> Option<&EffectDef> {
        self.effects.iter().find(|e| e.id == id)
    }

    /// All available effects, in alphabetical order.
    pub fn all(&self) -> &[EffectDef] {
        &self.effects
    }
}

/// Parse `// @key: value` metadata from the top of a WGSL source string.
/// Parsing stops at the first non-comment, non-blank line.
fn parse_header(source: &str) -> (String, String, String, Vec<ParamDef>) {
    let mut name = String::new();
    let mut author = String::new();
    let mut description = String::new();
    let mut params = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with("//") {
            break;
        }
        let comment_body = trimmed.trim_start_matches("//").trim();
        if let Some(value) = comment_body.strip_prefix("@name:") {
            name = value.trim().to_string();
        } else if let Some(value) = comment_body.strip_prefix("@author:") {
            author = value.trim().to_string();
        } else if let Some(value) = comment_body.strip_prefix("@description:") {
            description = value.trim().to_string();
        } else if let Some(value) = comment_body.strip_prefix("@param:") {
            let parts: Vec<&str> = value.trim().split_whitespace().collect();
            if parts.len() >= 4 {
                if let (Ok(default), Ok(min), Ok(max)) = (
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    params.push(ParamDef {
                        name: parts[0].to_string(),
                        default,
                        min,
                        max,
                    });
                }
            }
        }
    }

    (name, author, description, params)
}

/// Convert a file stem like "circle_crop" to "Circle Crop".
fn title_case_stem(stem: &str) -> String {
    stem.split('_')
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn title_case_basic() {
        assert_eq!(title_case_stem("circle_crop"), "Circle Crop");
    }

    #[test]
    fn title_case_single_word() {
        assert_eq!(title_case_stem("bloom"), "Bloom");
    }

    #[test]
    fn title_case_already_capitalized() {
        assert_eq!(title_case_stem("Vignette_Effect"), "Vignette Effect");
    }

    #[test]
    fn parse_header_full() {
        let src = "// @name: Circle Crop\n// @author: Lodestone\n// @description: Crops to a circle\n// @param: radius 0.4 0.0 1.0\n// @param: feather 0.02 0.0 0.2\n\n@fragment\nfn fs_main() {}";
        let (name, author, desc, params) = parse_header(src);
        assert_eq!(name, "Circle Crop");
        assert_eq!(author, "Lodestone");
        assert_eq!(desc, "Crops to a circle");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "radius");
        assert!((params[0].default - 0.4).abs() < f32::EPSILON);
        assert!((params[0].min - 0.0).abs() < f32::EPSILON);
        assert!((params[0].max - 1.0).abs() < f32::EPSILON);
        assert_eq!(params[1].name, "feather");
        assert!((params[1].default - 0.02).abs() < f32::EPSILON);
        assert!((params[1].min - 0.0).abs() < f32::EPSILON);
        assert!((params[1].max - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_header_no_params() {
        let src = "// @name: Simple\n@fragment\nfn fs_main() {}";
        let (name, _, _, params) = parse_header(src);
        assert_eq!(name, "Simple");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_header_empty_source() {
        let (name, author, desc, params) = parse_header("");
        assert_eq!(name, "");
        assert_eq!(author, "");
        assert_eq!(desc, "");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_header_no_comment_lines() {
        let src = "@fragment\nfn fs_main() {}";
        let (name, _, _, _) = parse_header(src);
        assert_eq!(name, "");
    }

    #[test]
    fn parse_header_malformed_params() {
        let src = "// @param: radius 0.4 not_a_number\n@fragment\nfn fs_main() {}";
        let (_, _, _, params) = parse_header(src);
        assert!(params.is_empty());
    }

    #[test]
    fn empty_registry() {
        let reg = EffectRegistry::empty();
        assert!(reg.all().is_empty());
        assert!(reg.get("anything").is_none());
    }

    #[test]
    fn registry_scan_empty_dir() {
        let dir = TempDir::new().unwrap();
        let reg = EffectRegistry::scan(dir.path());
        assert_eq!(reg.all().len(), 0);
    }

    #[test]
    fn registry_scan_finds_wgsl_files() {
        let dir = TempDir::new().unwrap();
        let mut f = std::fs::File::create(dir.path().join("bloom.wgsl")).unwrap();
        writeln!(f, "// @name: Bloom\n@fragment fn fs_main() {{}}").unwrap();
        let mut f2 = std::fs::File::create(dir.path().join("vignette.wgsl")).unwrap();
        writeln!(f2, "@fragment fn fs_main() {{}}").unwrap();

        let reg = EffectRegistry::scan(dir.path());
        assert_eq!(reg.all().len(), 2);
        assert!(reg.get("bloom").is_some());
        assert!(reg.get("vignette").is_some());
        assert_eq!(reg.get("bloom").unwrap().name, "Bloom");
        assert_eq!(reg.get("vignette").unwrap().name, "Vignette");
    }

    #[test]
    fn registry_scan_ignores_non_wgsl() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("readme.txt"), "not a shader").unwrap();
        let reg = EffectRegistry::scan(dir.path());
        assert_eq!(reg.all().len(), 0);
    }

    #[test]
    fn registry_get_missing_returns_none() {
        let dir = TempDir::new().unwrap();
        let reg = EffectRegistry::scan(dir.path());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_alphabetical_sort() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("zebra.wgsl"), "// @name: Zebra\n@fragment fn fs_main() {}").unwrap();
        std::fs::write(dir.path().join("alpha.wgsl"), "// @name: Alpha\n@fragment fn fs_main() {}").unwrap();
        std::fs::write(dir.path().join("gamma.wgsl"), "// @name: Gamma\n@fragment fn fs_main() {}").unwrap();

        let reg = EffectRegistry::scan(dir.path());
        assert_eq!(reg.all().len(), 3);
        assert_eq!(reg.all()[0].name, "Alpha");
        assert_eq!(reg.all()[1].name, "Gamma");
        assert_eq!(reg.all()[2].name, "Zebra");
    }

    #[test]
    fn registry_rescan_detects_changes() {
        let dir = TempDir::new().unwrap();
        let mut reg = EffectRegistry::scan(dir.path());
        assert!(reg.all().is_empty());

        std::fs::write(dir.path().join("new_effect.wgsl"), "// @name: New\n@fragment fn fs_main() {}").unwrap();
        assert!(reg.rescan(dir.path()));
        assert_eq!(reg.all().len(), 1);

        assert!(!reg.rescan(dir.path()));
        assert_eq!(reg.all().len(), 1);
    }
}
