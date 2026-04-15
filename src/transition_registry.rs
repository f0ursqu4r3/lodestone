use std::collections::HashMap;
use std::sync::Arc;

/// Which color uniforms a transition shader exposes to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionParam {
    Color,
    FromColor,
    ToColor,
}

/// Definition of a single numeric parameter exposed by a transition shader.
#[derive(Debug, Clone)]
pub struct TransitionParamDef {
    pub name: String,
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

/// Parsed definition of a single transition shader.
#[derive(Debug, Clone)]
pub struct TransitionDef {
    /// Unique ID derived from the file stem (e.g. "fade", "dip_to_color").
    pub id: String,
    /// Display name from `@name` header, or title-cased file stem.
    pub name: String,
    /// Author from `@author` header, or empty.
    #[allow(dead_code)]
    pub author: String,
    /// Description from `@description` header, or empty.
    #[allow(dead_code)]
    pub description: String,
    /// Which color uniforms to expose in the UI, from `@params` header.
    pub params: Vec<TransitionParam>,
    /// Numeric parameters exposed by the shader, from `@param` headers.
    pub shader_params: Vec<TransitionParamDef>,
    /// Raw WGSL shader source.
    pub shader_source: String,
}

/// Registry of available transition shaders.
#[derive(Debug, Clone)]
pub struct TransitionRegistry {
    transitions: Arc<[TransitionDef]>,
    transition_index: Arc<HashMap<String, usize>>,
    /// Simple fingerprint: concatenation of (id, shader_source length) for change detection.
    fingerprint: u64,
}

impl TransitionRegistry {
    /// Create an empty registry with just the synthetic "cut" entry.
    pub fn empty() -> Self {
        let transitions = vec![TransitionDef {
            id: crate::transition::TRANSITION_CUT.to_string(),
            name: "Cut".to_string(),
            author: String::new(),
            description: "Instant scene switch".to_string(),
            params: Vec::new(),
            shader_params: Vec::new(),
            shader_source: String::new(),
        }];
        Self::from_transitions(transitions)
    }

    /// Scan a directory for `.wgsl` files and build the registry.
    /// Always includes a synthetic "cut" entry first.
    pub fn scan(dir: &std::path::Path) -> Self {
        let mut transitions = Vec::new();

        // Synthetic "Cut" entry — always present, always first.
        transitions.push(TransitionDef {
            id: crate::transition::TRANSITION_CUT.to_string(),
            name: "Cut".to_string(),
            author: String::new(),
            description: "Instant scene switch".to_string(),
            params: Vec::new(),
            shader_params: Vec::new(),
            shader_source: String::new(),
        });

        // Scan directory for .wgsl files.
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                log::warn!(
                    "Failed to read transitions directory {}: {e}",
                    dir.display()
                );
                return Self::from_transitions(transitions);
            }
        };

        let mut shader_defs: Vec<TransitionDef> = Vec::new();

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
                    log::warn!("Failed to read transition shader {}: {e}", path.display());
                    continue;
                }
            };

            let (header_name, author, description, params, shader_params) = parse_header(&source);
            let name = if header_name.is_empty() {
                title_case_stem(&stem)
            } else {
                header_name
            };

            shader_defs.push(TransitionDef {
                id: stem,
                name,
                author,
                description,
                params,
                shader_params,
                shader_source: source,
            });
        }

        // Sort shader defs alphabetically by name.
        shader_defs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        transitions.extend(shader_defs);

        Self::from_transitions(transitions)
    }

    /// Re-scan the transitions directory. Returns true if the registry changed.
    pub fn rescan(&mut self, dir: &std::path::Path) -> bool {
        let new = Self::scan(dir);
        if new.fingerprint != self.fingerprint {
            *self = new;
            true
        } else {
            false
        }
    }

    /// Simple fingerprint based on file count, IDs, and source lengths.
    fn compute_fingerprint(transitions: &[TransitionDef]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        for t in transitions {
            t.id.hash(&mut hasher);
            t.shader_source.len().hash(&mut hasher);
            t.shader_source.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn build_index(transitions: &[TransitionDef]) -> HashMap<String, usize> {
        transitions
            .iter()
            .enumerate()
            .map(|(idx, transition)| (transition.id.clone(), idx))
            .collect()
    }

    fn from_transitions(transitions: Vec<TransitionDef>) -> Self {
        let fingerprint = Self::compute_fingerprint(&transitions);
        let transition_index = Self::build_index(&transitions);
        Self {
            transitions: Arc::<[TransitionDef]>::from(transitions),
            transition_index: Arc::new(transition_index),
            fingerprint,
        }
    }

    /// Look up a transition by ID.
    pub fn get(&self, id: &str) -> Option<&TransitionDef> {
        self.transition_index
            .get(id)
            .map(|&idx| &self.transitions[idx])
    }

    /// All available transitions, in order (Cut first, then alphabetical).
    pub fn all(&self) -> &[TransitionDef] {
        &self.transitions
    }
}

/// Parse `// @key: value` metadata from the top of a WGSL source string.
/// Parsing stops at the first non-comment, non-blank line.
///
/// Supports both `@params:` (color uniforms, comma-separated) and
/// `@param:` (numeric parameters, `name default min max` per line).
fn parse_header(
    source: &str,
) -> (
    String,
    String,
    String,
    Vec<TransitionParam>,
    Vec<TransitionParamDef>,
) {
    let mut name = String::new();
    let mut author = String::new();
    let mut description = String::new();
    let mut params = Vec::new();
    let mut shader_params = Vec::new();

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
        } else if let Some(value) = comment_body.strip_prefix("@params:") {
            params = value
                .split(',')
                .filter_map(|p| match p.trim() {
                    "color" => Some(TransitionParam::Color),
                    "from_color" => Some(TransitionParam::FromColor),
                    "to_color" => Some(TransitionParam::ToColor),
                    _ => None,
                })
                .collect();
        } else if let Some(value) = comment_body.strip_prefix("@param:") {
            let parts: Vec<&str> = value.trim().split_whitespace().collect();
            if parts.len() >= 4 {
                if let (Ok(default), Ok(min), Ok(max)) = (
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    shader_params.push(TransitionParamDef {
                        name: parts[0].to_string(),
                        default,
                        min,
                        max,
                    });
                }
            }
        }
    }

    (name, author, description, params, shader_params)
}

/// Convert a file stem like "radial_wipe" to "Radial Wipe".
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
        assert_eq!(title_case_stem("radial_wipe"), "Radial Wipe");
    }

    #[test]
    fn title_case_single_word() {
        assert_eq!(title_case_stem("fade"), "Fade");
    }

    #[test]
    fn title_case_already_capitalized() {
        assert_eq!(title_case_stem("Dip_To_Color"), "Dip To Color");
    }

    #[test]
    fn parse_header_full() {
        let src = "// @name: Dip to Color\n// @author: Lodestone\n// @description: Fades through a solid color\n// @params: color, from_color\n\nstruct TransitionUniforms { progress: f32 };\n";
        let (name, author, desc, params, shader_params) = parse_header(src);
        assert_eq!(name, "Dip to Color");
        assert_eq!(author, "Lodestone");
        assert_eq!(desc, "Fades through a solid color");
        assert_eq!(
            params,
            vec![TransitionParam::Color, TransitionParam::FromColor]
        );
        assert!(shader_params.is_empty());
    }

    #[test]
    fn parse_header_partial_no_params() {
        let src = "// @name: Fade\n\nstruct Foo {};";
        let (name, author, desc, params, _) = parse_header(src);
        assert_eq!(name, "Fade");
        assert_eq!(author, "");
        assert_eq!(desc, "");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_header_empty_source() {
        let (name, author, desc, params, shader_params) = parse_header("");
        assert_eq!(name, "");
        assert_eq!(author, "");
        assert_eq!(desc, "");
        assert!(params.is_empty());
        assert!(shader_params.is_empty());
    }

    #[test]
    fn parse_header_no_comment_lines() {
        let src = "struct TransitionUniforms { progress: f32 };";
        let (name, _, _, _, _) = parse_header(src);
        assert_eq!(name, "");
    }

    #[test]
    fn parse_header_all_params() {
        let src = "// @params: color, from_color, to_color\n";
        let (_, _, _, params, _) = parse_header(src);
        assert_eq!(
            params,
            vec![
                TransitionParam::Color,
                TransitionParam::FromColor,
                TransitionParam::ToColor,
            ]
        );
    }

    #[test]
    fn parse_header_shader_params() {
        let src = "// @name: Wipe\n// @param: softness 0.02 0.0 0.5\n// @param: angle 0.0 0.0 360.0\n\n@fragment fn fs_main() {}";
        let (name, _, _, _, shader_params) = parse_header(src);
        assert_eq!(name, "Wipe");
        assert_eq!(shader_params.len(), 2);
        assert_eq!(shader_params[0].name, "softness");
        assert!((shader_params[0].default - 0.02).abs() < f32::EPSILON);
        assert!((shader_params[0].min - 0.0).abs() < f32::EPSILON);
        assert!((shader_params[0].max - 0.5).abs() < f32::EPSILON);
        assert_eq!(shader_params[1].name, "angle");
        assert!((shader_params[1].default - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn registry_scan_empty_dir_has_cut() {
        let dir = TempDir::new().unwrap();
        let reg = TransitionRegistry::scan(dir.path());
        assert_eq!(reg.all().len(), 1);
        assert_eq!(reg.all()[0].id, "cut");
    }

    #[test]
    fn registry_scan_finds_wgsl_files() {
        let dir = TempDir::new().unwrap();
        let mut f = std::fs::File::create(dir.path().join("fade.wgsl")).unwrap();
        writeln!(f, "// @name: Fade\n@fragment fn fs_main() {{}}").unwrap();
        let mut f2 = std::fs::File::create(dir.path().join("wipe.wgsl")).unwrap();
        writeln!(f2, "@fragment fn fs_main() {{}}").unwrap();

        let reg = TransitionRegistry::scan(dir.path());
        // cut + fade + wipe
        assert_eq!(reg.all().len(), 3);
        assert!(reg.get("fade").is_some());
        assert!(reg.get("wipe").is_some());
        assert_eq!(reg.get("fade").unwrap().name, "Fade");
        assert_eq!(reg.get("wipe").unwrap().name, "Wipe");
    }

    #[test]
    fn registry_scan_ignores_non_wgsl() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("readme.txt"), "not a shader").unwrap();
        let reg = TransitionRegistry::scan(dir.path());
        assert_eq!(reg.all().len(), 1);
    }

    #[test]
    fn registry_cut_always_first() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("aaa.wgsl"), "@fragment fn fs_main() {}").unwrap();
        let reg = TransitionRegistry::scan(dir.path());
        assert_eq!(reg.all()[0].id, "cut");
    }

    #[test]
    fn registry_get_missing_returns_none() {
        let dir = TempDir::new().unwrap();
        let reg = TransitionRegistry::scan(dir.path());
        assert!(reg.get("nonexistent").is_none());
    }
}
