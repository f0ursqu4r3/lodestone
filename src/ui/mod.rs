pub mod audio_mixer;
pub mod scene_editor;
pub mod settings_modal;
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
            scene_editor::draw(ctx, state);
            audio_mixer::draw(ctx, state);
            stream_controls::draw(ctx, state);
            settings_modal::draw(ctx, state);
        })
    }
}
