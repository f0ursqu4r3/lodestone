mod mock_driver;
mod obs;
mod renderer;
mod settings;
mod state;
mod ui;

use anyhow::Result;
use obs::ObsEngine;
use obs::mock::MockObsEngine;
use renderer::Renderer;
use state::AppState;
use std::sync::{Arc, Mutex};
use ui::layout::PanelId;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::EventLoop,
    window::{Window, WindowAttributes},
};

struct App {
    window: Option<&'static Window>,
    renderer: Option<Renderer>,
    state: Arc<Mutex<AppState>>,
    egui_ctx: Option<egui::Context>,
    egui_state: Option<egui_winit::State>,
    runtime: tokio::runtime::Runtime,
    #[allow(dead_code)]
    engine: MockObsEngine,
}

impl App {
    fn new() -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
        let engine = MockObsEngine::new();

        // Populate initial AppState from the engine's default scenes/sources.
        let scenes = engine.scenes();
        let active_scene_id = engine.active_scene_id();
        let initial_state = AppState {
            scenes,
            active_scene_id,
            ..AppState::default()
        };

        Self {
            window: None,
            renderer: None,
            state: Arc::new(Mutex::new(initial_state)),
            egui_ctx: None,
            egui_state: None,
            runtime,
            engine,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = WindowAttributes::default()
            .with_title("Lodestone")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_min_inner_size(LogicalSize::new(960.0, 540.0));
        let window = event_loop.create_window(attrs).expect("create window");
        let window: &'static Window = Box::leak(Box::new(window));
        self.window = Some(window);
        let renderer = pollster::block_on(Renderer::new(window)).expect("initialize renderer");
        self.renderer = Some(renderer);

        // Initialize egui
        let ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(renderer_max_texture_side(&self.renderer)),
        );
        self.egui_ctx = Some(ctx);
        self.egui_state = Some(egui_state);

        // Spawn mock data driver on the tokio runtime
        self.runtime
            .spawn(mock_driver::run_mock_driver(self.state.clone()));

        log::info!("Window and renderer initialized");
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        // Feed events to egui first
        if let Some(egui_state) = &mut self.egui_state
            && let Some(window) = self.window
        {
            let _ = egui_state.on_window_event(window, &event);
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(width, height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(renderer), Some(ctx), Some(egui_state), Some(window)) = (
                    &mut self.renderer,
                    &self.egui_ctx,
                    &mut self.egui_state,
                    self.window,
                ) {
                    let raw_input = egui_state.take_egui_input(window);
                    let mut app_state = self.state.lock().unwrap();

                    let full_output = ctx.run(raw_input, |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui::draw_panel(
                                ui::layout::PanelType::SceneEditor,
                                ui,
                                &mut app_state,
                                PanelId(0),
                            );
                        });
                    });
                    drop(app_state);

                    let pixels_per_point = full_output.pixels_per_point;
                    let paint_jobs = ctx.tessellate(full_output.shapes, pixels_per_point);

                    if let Err(e) = renderer.render_with_egui(
                        &paint_jobs,
                        &full_output.textures_delta,
                        pixels_per_point,
                    ) {
                        log::error!("Render error: {e}");
                    }

                    egui_state.handle_platform_output(window, full_output.platform_output);
                } else if let Some(renderer) = &mut self.renderer
                    && let Err(e) = renderer.render()
                {
                    log::error!("Render error: {e}");
                }
                if let Some(window) = self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn renderer_max_texture_side(renderer: &Option<Renderer>) -> usize {
    renderer
        .as_ref()
        .map(|r| r.device.limits().max_texture_dimension_2d as usize)
        .unwrap_or(2048)
}

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");
    let event_loop = EventLoop::new()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
