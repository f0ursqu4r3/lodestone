// Stub — will be replaced in a later task with DockLayout serialization.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::tree::{DockLayout, PanelType};

/// Placeholder for detached window serialization (kept for main.rs compat).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DetachedEntry {
    pub panel: PanelType,
    pub id: u64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Stub: serialize the layout to TOML. Returns an empty string for now.
pub fn serialize_full_layout(_layout: &DockLayout, _detached: &[DetachedEntry]) -> Result<String> {
    Ok(String::new())
}

/// Stub: deserialize a layout from TOML. Falls back to default_layout.
pub fn deserialize_full_layout(_toml_str: &str) -> Result<(DockLayout, Vec<DetachedEntry>)> {
    Ok((DockLayout::default_layout(), Vec::new()))
}
