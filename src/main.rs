mod obs;
mod renderer;
mod settings;
mod state;

use std::sync::{Arc, Mutex};
use anyhow::Result;
use renderer::Renderer;
use state::AppState;
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
}

impl App {
    fn new() -> Self {
        Self { window: None, renderer: None, state: Arc::new(Mutex::new(AppState::default())) }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() { return; }
        let attrs = WindowAttributes::default()
            .with_title("Lodestone")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_min_inner_size(LogicalSize::new(960.0, 540.0));
        let window = event_loop.create_window(attrs).expect("create window");
        let window: &'static Window = Box::leak(Box::new(window));
        self.window = Some(window);
        let renderer = pollster::block_on(Renderer::new(window)).expect("initialize renderer");
        self.renderer = Some(renderer);
        log::info!("Window and renderer initialized");
    }

    fn window_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, _window_id: winit::window::WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => { event_loop.exit(); }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                if let Some(renderer) = &mut self.renderer { renderer.resize(width, height); }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &mut self.renderer {
                    if let Err(e) = renderer.render() { log::error!("Render error: {e}"); }
                }
                if let Some(window) = self.window { window.request_redraw(); }
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");
    let event_loop = EventLoop::new()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
