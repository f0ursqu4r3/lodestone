use crate::state::{AppState, AudioLevel, StreamStatus};
use rand::RngExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time;

pub fn random_walk_db(current: f32) -> f32 {
    let mut rng = rand::rng();
    let delta: f32 = rng.random_range(-3.0..3.0);
    (current + delta).clamp(-60.0, 0.0)
}

pub fn decay_peak(peak: f32, dt: f32) -> f32 {
    peak - (20.0 * dt) // 20 dB/s decay
}

pub fn spawn_mock_driver(state: Arc<Mutex<AppState>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(33));
        let mut levels: Vec<f32> = Vec::new();
        let mut peaks: Vec<f32> = Vec::new();
        let mut uptime: f64 = 0.0;

        loop {
            interval.tick().await;
            let dt = 1.0 / 30.0;
            let mut state = state.lock().expect("lock state");

            while levels.len() < state.sources.len() {
                levels.push(-30.0);
                peaks.push(-60.0);
            }

            // Collect source ids to avoid simultaneous borrow of state.sources and state.audio_levels
            let source_ids: Vec<_> = state.sources.iter().map(|s| s.id).collect();

            state.audio_levels.clear();
            for (i, source_id) in source_ids.iter().enumerate() {
                if i < levels.len() {
                    levels[i] = random_walk_db(levels[i]);
                    if levels[i] > peaks[i] {
                        peaks[i] = levels[i];
                    } else {
                        peaks[i] = decay_peak(peaks[i], dt).max(levels[i]);
                    }
                    state
                        .audio_levels
                        .push(AudioLevel::new(*source_id, levels[i], peaks[i]));
                }
            }

            if let StreamStatus::Live {
                ref mut uptime_secs,
                ref mut bitrate_kbps,
                ref mut dropped_frames,
            } = state.stream_status
            {
                uptime += dt as f64;
                *uptime_secs = uptime;
                let mut rng = rand::rng();
                *bitrate_kbps = (4500.0 + rng.random_range(-100.0..100.0_f64)).max(0.0);
                if rng.random_range(0..300_u32) == 0 {
                    *dropped_frames += 1;
                }
            } else {
                uptime = 0.0;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_level_random_walk_stays_in_range() {
        let mut level = -30.0_f32;
        for _ in 0..1000 {
            level = random_walk_db(level);
            assert!(level >= -60.0);
            assert!(level <= 0.0);
        }
    }

    #[test]
    fn peak_decay_reduces_over_time() {
        let peak = decay_peak(-5.0, 1.0 / 30.0);
        assert!(peak < -5.0);
    }
}
