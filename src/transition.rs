use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use crate::scene::SceneId;

/// Well-known transition ID: instant scene switch, no animation.
pub const TRANSITION_CUT: &str = "cut";
/// Well-known transition ID: linear crossfade.
pub const TRANSITION_FADE: &str = "fade";

/// User-configurable color parameters passed to transition shaders.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionColors {
    /// Accent color for the transition effect (e.g. wipe edge glow).
    pub color: [f32; 4],
    /// Color to transition FROM (e.g. dip-to-black: this is black).
    pub from_color: [f32; 4],
    /// Color to transition TO.
    pub to_color: [f32; 4],
}

impl Default for TransitionColors {
    fn default() -> Self {
        Self {
            color: [0.0, 0.0, 0.0, 1.0],
            from_color: [0.0, 0.0, 0.0, 1.0],
            to_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// Global transition defaults, persisted in settings TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionSettings {
    /// Transition ID string (file stem, e.g. "fade", "dip_to_color").
    pub default_transition: String,
    pub default_duration_ms: u32,
    pub default_colors: TransitionColors,
}

impl Default for TransitionSettings {
    fn default() -> Self {
        Self {
            default_transition: TRANSITION_FADE.to_string(),
            default_duration_ms: 300,
            default_colors: TransitionColors::default(),
        }
    }
}

/// Per-scene transition override. Controls the transition used when
/// transitioning *into* this scene. `None` fields inherit from global defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneTransitionOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<TransitionColors>,
}

/// Fully resolved transition parameters (global defaults merged with per-scene overrides).
pub struct ResolvedTransition {
    pub transition: String,
    pub duration: Duration,
    pub colors: TransitionColors,
}

/// Runtime state for an in-progress transition. Not persisted.
#[derive(Debug, Clone)]
pub struct TransitionState {
    pub from_scene: SceneId,
    pub to_scene: SceneId,
    /// Transition ID string (e.g. "fade", "dip_to_color").
    pub transition: String,
    pub started_at: Instant,
    pub duration: Duration,
    pub colors: TransitionColors,
}

impl TransitionState {
    /// Returns the transition progress in 0.0..=1.0.
    pub fn progress(&self) -> f32 {
        let elapsed = self.started_at.elapsed().as_secs_f32();
        let total = self.duration.as_secs_f32();
        if total <= 0.0 {
            1.0
        } else {
            (elapsed / total).clamp(0.0, 1.0)
        }
    }

    /// Returns true when the transition has completed.
    pub fn is_complete(&self) -> bool {
        self.started_at.elapsed() >= self.duration
    }
}

/// Resolve which transition, duration, and colors to use for a scene switch.
/// Per-scene override takes priority over global default.
pub fn resolve_transition(
    global: &TransitionSettings,
    scene_override: &SceneTransitionOverride,
) -> ResolvedTransition {
    let transition = scene_override
        .transition
        .clone()
        .unwrap_or_else(|| global.default_transition.clone());
    let duration_ms = scene_override
        .duration_ms
        .unwrap_or(global.default_duration_ms);
    let colors = scene_override.colors.unwrap_or(global.default_colors);
    ResolvedTransition {
        transition,
        duration: Duration::from_millis(duration_ms as u64),
        colors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_colors_default_is_black() {
        let c = TransitionColors::default();
        assert_eq!(c.color, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(c.from_color, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(c.to_color, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn transition_settings_default_uses_fade() {
        let s = TransitionSettings::default();
        assert_eq!(s.default_transition, TRANSITION_FADE);
        assert_eq!(s.default_duration_ms, 300);
    }

    #[test]
    fn resolve_uses_global_defaults() {
        let global = TransitionSettings::default();
        let override_ = SceneTransitionOverride::default();
        let resolved = resolve_transition(&global, &override_);
        assert_eq!(resolved.transition, TRANSITION_FADE);
        assert_eq!(resolved.duration, Duration::from_millis(300));
        assert_eq!(resolved.colors.color, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn resolve_per_scene_overrides_global() {
        let global = TransitionSettings::default();
        let override_ = SceneTransitionOverride {
            transition: Some(TRANSITION_CUT.to_string()),
            duration_ms: Some(0),
            colors: Some(TransitionColors {
                color: [1.0, 0.0, 0.0, 1.0],
                ..Default::default()
            }),
        };
        let resolved = resolve_transition(&global, &override_);
        assert_eq!(resolved.transition, TRANSITION_CUT);
        assert_eq!(resolved.duration, Duration::ZERO);
        assert_eq!(resolved.colors.color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn resolve_partial_override_inherits_unset_fields() {
        let global = TransitionSettings {
            default_transition: TRANSITION_FADE.to_string(),
            default_duration_ms: 300,
            default_colors: TransitionColors::default(),
        };
        let override_ = SceneTransitionOverride {
            transition: None,
            duration_ms: Some(1000),
            colors: None,
        };
        let resolved = resolve_transition(&global, &override_);
        assert_eq!(resolved.transition, TRANSITION_FADE);
        assert_eq!(resolved.duration, Duration::from_millis(1000));
    }

    #[test]
    fn transition_state_progress_at_start() {
        let state = TransitionState {
            from_scene: SceneId(1),
            to_scene: SceneId(2),
            transition: TRANSITION_FADE.to_string(),
            started_at: Instant::now(),
            duration: Duration::from_millis(300),
            colors: TransitionColors::default(),
        };
        assert!(state.progress() < 0.1);
        assert!(!state.is_complete());
    }

    #[test]
    fn transition_state_progress_when_complete() {
        let state = TransitionState {
            from_scene: SceneId(1),
            to_scene: SceneId(2),
            transition: TRANSITION_FADE.to_string(),
            started_at: Instant::now() - Duration::from_millis(500),
            duration: Duration::from_millis(300),
            colors: TransitionColors::default(),
        };
        assert_eq!(state.progress(), 1.0);
        assert!(state.is_complete());
    }

    #[test]
    fn transition_state_zero_duration_is_immediately_complete() {
        let state = TransitionState {
            from_scene: SceneId(1),
            to_scene: SceneId(2),
            transition: TRANSITION_CUT.to_string(),
            started_at: Instant::now(),
            duration: Duration::ZERO,
            colors: TransitionColors::default(),
        };
        assert_eq!(state.progress(), 1.0);
        assert!(state.is_complete());
    }

    #[test]
    fn scene_transition_override_default_is_none() {
        let o = SceneTransitionOverride::default();
        assert!(o.transition.is_none());
        assert!(o.duration_ms.is_none());
        assert!(o.colors.is_none());
    }
}
