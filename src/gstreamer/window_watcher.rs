//! Periodic window watcher that resolves the best capture target for each
//! active window source. Runs on the GStreamer thread.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::devices::{AppInfo, enumerate_applications};
use super::screencapturekit;
use crate::scene::{SourceId, WindowCaptureMode};

/// How often to poll for window changes.
const POLL_INTERVAL: Duration = Duration::from_millis(1500);

/// Tracks the current state of a window capture source.
pub struct WatchedSource {
    pub mode: WindowCaptureMode,
    pub current_window_id: Option<u32>,
    #[allow(dead_code)]
    pub current_window_size: Option<(u32, u32)>,
}

/// Resolves the best capture target for window sources.
pub struct WindowWatcher {
    last_poll: Instant,
    cached_apps: Vec<AppInfo>,
    cached_display_bounds: Vec<(f64, f64, f64, f64)>,
}

impl WindowWatcher {
    pub fn new() -> Self {
        Self {
            last_poll: Instant::now() - POLL_INTERVAL,
            cached_apps: Vec::new(),
            cached_display_bounds: Vec::new(),
        }
    }

    /// Called each iteration of the GStreamer thread poll loop.
    /// Returns (source_id, new_window_id) pairs for changed targets.
    pub fn poll(
        &mut self,
        watched: &HashMap<SourceId, WatchedSource>,
    ) -> Vec<(SourceId, Option<u32>)> {
        if self.last_poll.elapsed() < POLL_INTERVAL || watched.is_empty() {
            return Vec::new();
        }
        self.last_poll = Instant::now();
        self.cached_apps = enumerate_applications();
        self.refresh_display_bounds();

        let mut changes = Vec::new();
        for (source_id, source) in watched {
            let resolved = self.resolve_target(&source.mode);
            if resolved != source.current_window_id {
                changes.push((*source_id, resolved));
            }
        }
        changes
    }

    /// Force an immediate refresh of cached data.
    pub fn force_refresh(&mut self) {
        self.cached_apps = enumerate_applications();
        self.refresh_display_bounds();
        self.last_poll = Instant::now();
    }

    /// Resolve the best window ID for the given capture mode.
    pub fn resolve_target(&self, mode: &WindowCaptureMode) -> Option<u32> {
        match mode {
            WindowCaptureMode::AnyFullscreen => self.find_fullscreen_window(),
            WindowCaptureMode::Application {
                bundle_id,
                pinned_title,
                ..
            } => self.find_app_window(bundle_id, pinned_title.as_deref()),
        }
    }

    fn find_fullscreen_window(&self) -> Option<u32> {
        self.cached_apps
            .iter()
            .flat_map(|app| &app.windows)
            .find(|w| w.is_fullscreen && w.is_on_screen)
            .map(|w| w.window_id)
    }

    fn find_app_window(&self, bundle_id: &str, pinned_title: Option<&str>) -> Option<u32> {
        let app = self.cached_apps.iter().find(|a| a.bundle_id == bundle_id)?;
        if app.windows.is_empty() {
            return None;
        }
        if let Some(title) = pinned_title
            && let Some(win) = app.windows.iter().find(|w| w.title.contains(title))
        {
            return Some(win.window_id);
        }
        app.windows
            .iter()
            .find(|w| w.is_on_screen)
            .or(app.windows.first())
            .map(|w| w.window_id)
    }

    fn refresh_display_bounds(&mut self) {
        match screencapturekit::enumerate_displays() {
            Ok(displays) => {
                self.cached_display_bounds = displays
                    .iter()
                    .map(|d| (0.0, 0.0, d.width as f64, d.height as f64))
                    .collect();
            }
            Err(e) => {
                log::warn!("Failed to enumerate displays for fullscreen detection: {e}");
            }
        }
    }
}
