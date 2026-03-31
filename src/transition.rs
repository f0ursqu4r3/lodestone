use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use crate::scene::SceneId;

/// The type of transition effect between scenes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransitionType {
    /// Instant scene switch, no animation.
    Cut,
    /// Linear crossfade between outgoing and incoming scene.
    #[default]
    Fade,
}

/// Global transition defaults, persisted in settings TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionSettings {
    pub default_type: TransitionType,
    pub default_duration_ms: u32,
}

impl Default for TransitionSettings {
    fn default() -> Self {
        Self {
            default_type: TransitionType::Fade,
            default_duration_ms: 300,
        }
    }
}

/// Per-scene transition override. Controls the transition used when
/// transitioning *into* this scene. `None` fields inherit from global defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneTransitionOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_type: Option<TransitionType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
}

/// Runtime state for an in-progress transition. Not persisted.
#[derive(Debug, Clone)]
pub struct TransitionState {
    pub from_scene: SceneId,
    pub to_scene: SceneId,
    pub transition_type: TransitionType,
    pub started_at: Instant,
    pub duration: Duration,
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

/// Resolve which transition type and duration to use for a scene switch.
/// Per-scene override takes priority over global default.
pub fn resolve_transition(
    global: &TransitionSettings,
    scene_override: &SceneTransitionOverride,
) -> (TransitionType, Duration) {
    let t = scene_override
        .transition_type
        .unwrap_or(global.default_type);
    let d = scene_override
        .duration_ms
        .unwrap_or(global.default_duration_ms);
    (t, Duration::from_millis(d as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_state_progress_at_start() {
        let state = TransitionState {
            from_scene: SceneId(1),
            to_scene: SceneId(2),
            transition_type: TransitionType::Fade,
            started_at: Instant::now(),
            duration: Duration::from_millis(300),
        };
        assert!(state.progress() < 0.1);
        assert!(!state.is_complete());
    }

    #[test]
    fn transition_state_progress_when_complete() {
        let state = TransitionState {
            from_scene: SceneId(1),
            to_scene: SceneId(2),
            transition_type: TransitionType::Fade,
            started_at: Instant::now() - Duration::from_millis(500),
            duration: Duration::from_millis(300),
        };
        assert_eq!(state.progress(), 1.0);
        assert!(state.is_complete());
    }

    #[test]
    fn transition_state_zero_duration_is_immediately_complete() {
        let state = TransitionState {
            from_scene: SceneId(1),
            to_scene: SceneId(2),
            transition_type: TransitionType::Cut,
            started_at: Instant::now(),
            duration: Duration::ZERO,
        };
        assert_eq!(state.progress(), 1.0);
        assert!(state.is_complete());
    }

    #[test]
    fn resolve_uses_global_defaults() {
        let global = TransitionSettings {
            default_type: TransitionType::Fade,
            default_duration_ms: 500,
        };
        let override_ = SceneTransitionOverride::default();
        let (t, d) = resolve_transition(&global, &override_);
        assert_eq!(t, TransitionType::Fade);
        assert_eq!(d, Duration::from_millis(500));
    }

    #[test]
    fn resolve_per_scene_overrides_global() {
        let global = TransitionSettings {
            default_type: TransitionType::Fade,
            default_duration_ms: 300,
        };
        let override_ = SceneTransitionOverride {
            transition_type: Some(TransitionType::Cut),
            duration_ms: Some(0),
        };
        let (t, d) = resolve_transition(&global, &override_);
        assert_eq!(t, TransitionType::Cut);
        assert_eq!(d, Duration::ZERO);
    }

    #[test]
    fn resolve_partial_override() {
        let global = TransitionSettings {
            default_type: TransitionType::Fade,
            default_duration_ms: 300,
        };
        let override_ = SceneTransitionOverride {
            transition_type: None,
            duration_ms: Some(1000),
        };
        let (t, d) = resolve_transition(&global, &override_);
        assert_eq!(t, TransitionType::Fade);
        assert_eq!(d, Duration::from_millis(1000));
    }

    #[test]
    fn transition_settings_default() {
        let s = TransitionSettings::default();
        assert_eq!(s.default_type, TransitionType::Fade);
        assert_eq!(s.default_duration_ms, 300);
    }

    #[test]
    fn scene_transition_override_default_is_none() {
        let o = SceneTransitionOverride::default();
        assert!(o.transition_type.is_none());
        assert!(o.duration_ms.is_none());
    }
}
