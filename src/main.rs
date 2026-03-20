mod mock_driver;
mod obs;
mod renderer;
mod settings;
mod state;
mod ui;
mod window;

use anyhow::Result;
use obs::ObsEngine;
use obs::mock::MockObsEngine;
use renderer::SharedGpuState;
use state::AppState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use ui::layout::{LayoutTree, SplitDirection};
use window::{DetachRequest, WindowState};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::EventLoop,
    window::{Window, WindowAttributes, WindowId},
};

struct AppManager {
    gpu: Option<SharedGpuState>,
    windows: HashMap<WindowId, WindowState>,
    main_window_id: Option<WindowId>,
    state: Arc<Mutex<AppState>>,
    runtime: tokio::runtime::Runtime,
    #[allow(dead_code)]
    engine: MockObsEngine,
    pending_detaches: Vec<DetachRequest>,
}

impl AppManager {
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
            gpu: None,
            windows: HashMap::new(),
            main_window_id: None,
            state: Arc::new(Mutex::new(initial_state)),
            runtime,
            engine,
            pending_detaches: Vec::new(),
        }
    }
}

impl ApplicationHandler for AppManager {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.main_window_id.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("Lodestone")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_min_inner_size(LogicalSize::new(960.0, 540.0));
        let window = event_loop.create_window(attrs).expect("create window");
        let window: &'static Window = Box::leak(Box::new(window));
        let window_id = window.id();

        // Create shared GPU state from the main window
        let gpu =
            pollster::block_on(SharedGpuState::new(window)).expect("initialize shared GPU state");

        // Create main WindowState with default layout
        let layout = LayoutTree::default_layout();
        let win_state =
            WindowState::new(window, &gpu, layout, true).expect("create main window state");

        self.gpu = Some(gpu);
        self.main_window_id = Some(window_id);
        self.windows.insert(window_id, win_state);

        // Spawn mock data driver on the tokio runtime
        self.runtime
            .spawn(mock_driver::run_mock_driver(self.state.clone()));

        log::info!("Window and renderer initialized");
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Route event to the correct window
        if let Some(win) = self.windows.get_mut(&window_id) {
            let _ = win.egui_state.on_window_event(win.window, &event);
        }

        match event {
            WindowEvent::CloseRequested => {
                if Some(window_id) == self.main_window_id {
                    event_loop.exit();
                } else {
                    // Reattach panels from the detached window back to the main window.
                    if let Some(detached_win) = self.windows.remove(&window_id) {
                        if let Some(main_id) = self.main_window_id {
                            if let Some(main_win) = self.windows.get_mut(&main_id) {
                                let leaves = detached_win.layout.collect_leaves();
                                for (panel_id, panel_type, _node_id) in leaves {
                                    main_win.layout.insert_at_root(
                                        panel_type,
                                        panel_id,
                                        SplitDirection::Vertical,
                                        0.5,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                if let (Some(gpu), Some(win)) =
                    (&self.gpu, self.windows.get_mut(&window_id))
                {
                    win.resize(gpu, width, height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(gpu) = &self.gpu {
                    if let Some(win) = self.windows.get_mut(&window_id) {
                        let mut app_state = self.state.lock().unwrap();
                        match win.render(gpu, &mut app_state) {
                            Ok(detach_requests) => {
                                self.pending_detaches.extend(detach_requests);
                            }
                            Err(e) => {
                                log::error!("Render error: {e}");
                            }
                        }
                        drop(app_state);
                    }
                }
                if let Some(win) = self.windows.get(&window_id) {
                    win.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // Create windows for any pending detach requests.
        if let Some(gpu) = &self.gpu {
            for detach in self.pending_detaches.drain(..) {
                let attrs = WindowAttributes::default()
                    .with_title(detach.panel_type.display_name())
                    .with_inner_size(LogicalSize::new(400.0, 300.0));
                let window = event_loop.create_window(attrs).expect("create detached window");
                let window: &'static Window = Box::leak(Box::new(window));

                let layout = LayoutTree::new_with_id(detach.panel_type, detach.panel_id);
                let win_state =
                    WindowState::new(window, gpu, layout, false).expect("init detached window");
                self.windows.insert(window.id(), win_state);
            }
        }

        // Request redraw for all windows so detached windows also animate.
        for win in self.windows.values() {
            win.window.request_redraw();
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");
    let event_loop = EventLoop::new()?;
    let mut app = AppManager::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
