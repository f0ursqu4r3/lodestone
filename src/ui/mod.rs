pub mod audio_mixer;
pub mod scene_editor;
pub mod stream_controls;

use crate::state::AppState;
use egui::Context;

pub struct UiRoot {
    pub ctx: Context,
}

impl UiRoot {
    pub fn new() -> Self {
        Self {
            ctx: Context::default(),
        }
    }

    pub fn run(&self, state: &mut AppState, raw_input: egui::RawInput) -> egui::FullOutput {
        self.ctx.run(raw_input, |ctx| {
            egui::Window::new("Lodestone")
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("egui integration working");
                    let status = match &state.stream_status {
                        crate::state::StreamStatus::Offline => "Offline",
                        crate::state::StreamStatus::Connecting => "Connecting...",
                        crate::state::StreamStatus::Live { .. } => "Live",
                    };
                    ui.label(format!("Status: {status}"));
                });
        })
    }
}
