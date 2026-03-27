//! macOS window picker overlay.
//!
//! Shows a fullscreen transparent overlay that highlights windows under the
//! cursor. Clicking selects the window; pressing Escape cancels.
//!
//! Uses native AppKit via `objc2` — no winit/wgpu involvement. Runs a modal
//! session on the main thread and returns the result synchronously.

/// Result returned when the user clicks a window in the picker.
#[derive(Debug, Clone)]
pub struct PickerResult {
    pub bundle_id: String,
    pub app_name: String,
    pub window_title: String,
}

// ─── Native implementation (macOS) ───────────────────────────────────────────

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionaryRef;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::color::CGColor;
    use core_graphics::window::{
        CGWindowListCopyWindowInfo, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly,
    };
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSApplication, NSColor, NSCursor, NSEvent, NSEventType, NSPanel, NSScreen, NSView,
        NSWindowStyleMask,
    };
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
    use std::cell::RefCell;

    type CGFloat = f64;

    /// Highlight state for the window currently under the cursor.
    #[derive(Debug, Clone)]
    struct PickerHighlight {
        /// Screen-space bounds in AppKit coordinates (origin bottom-left).
        bounds: NSRect,
        app_name: String,
        window_title: String,
        bundle_id: String,
    }

    /// Minimum dimension to consider a window a valid pick target.
    const MIN_WINDOW_DIM: f64 = 50.0;

    thread_local! {
        /// Stores the picker result (or None if cancelled).
        static PICKER_RESULT: RefCell<Option<Option<PickerResult>>> = const { RefCell::new(None) };
        /// Current highlight for drawing.
        static PICKER_HIGHLIGHT: RefCell<Option<PickerHighlight>> = const { RefCell::new(None) };
        /// The overlay panel, so we can close it from event handlers.
        static PICKER_PANEL: RefCell<Option<Retained<NSPanel>>> = const { RefCell::new(None) };
        /// The highlight subview, for updating its frame on mouse move.
        static HIGHLIGHT_VIEW: RefCell<Option<Retained<NSView>>> = const { RefCell::new(None) };
    }

    /// Find the topmost window under `screen_point` (in macOS screen coordinates,
    /// origin bottom-left). Skips our own PID and tiny windows.
    fn window_under_cursor(screen_point: NSPoint) -> Option<PickerHighlight> {
        let own_pid = std::process::id() as i64;

        // CGWindowList uses top-left origin. We need to convert.
        let main_screen_height = {
            let screens = NSScreen::screens(MainThreadMarker::new().expect("main thread"));
            if screens.count() == 0 {
                return None;
            }
            let main = screens.objectAtIndex(0);
            main.frame().size.height
        };

        let cg_point_y = main_screen_height - screen_point.y;

        let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
        let window_list = unsafe { CGWindowListCopyWindowInfo(options as u32, kCGNullWindowID) };
        if window_list.is_null() {
            return None;
        }

        let list = unsafe {
            core_foundation::array::CFArray::<CFType>::wrap_under_get_rule(window_list as *const _)
        };

        for i in 0..list.len() {
            let Some(item) = list.get(i as _) else {
                continue;
            };
            let dict_ref = item.as_CFTypeRef() as CFDictionaryRef;
            let dict = unsafe {
                core_foundation::dictionary::CFDictionary::<CFString, CFType>::wrap_under_get_rule(
                    dict_ref as *const _,
                )
            };

            // Skip our own windows.
            let pid_key = CFString::new("kCGWindowOwnerPID");
            if let Some(pid_val) = dict.find(&pid_key) {
                let pid_ptr = pid_val.as_CFTypeRef();
                let pid_num = unsafe { CFNumber::wrap_under_get_rule(pid_ptr as *const _) };
                if let Some(pid) = pid_num.to_i64() {
                    if pid == own_pid {
                        continue;
                    }
                }
            }

            // Get window layer — skip windows not on layer 0 (normal windows).
            let layer_key = CFString::new("kCGWindowLayer");
            if let Some(layer_val) = dict.find(&layer_key) {
                let layer_ptr = layer_val.as_CFTypeRef();
                let layer_num = unsafe { CFNumber::wrap_under_get_rule(layer_ptr as *const _) };
                if let Some(layer) = layer_num.to_i32() {
                    if layer != 0 {
                        continue;
                    }
                }
            }

            // Get bounds.
            let bounds_key = CFString::new("kCGWindowBounds");
            let bounds_val = match dict.find(&bounds_key) {
                Some(v) => v,
                None => continue,
            };
            let bounds_dict_ref = bounds_val.as_CFTypeRef() as CFDictionaryRef;
            let bounds_dict = unsafe {
                core_foundation::dictionary::CFDictionary::<CFString, CFType>::wrap_under_get_rule(
                    bounds_dict_ref as *const _,
                )
            };

            let get_f64 = |key: &str| -> Option<f64> {
                let k = CFString::new(key);
                let v = bounds_dict.find(&k)?;
                let n = unsafe { CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as *const _) };
                n.to_f64()
            };

            let x = match get_f64("X") {
                Some(v) => v,
                None => continue,
            };
            let y = match get_f64("Y") {
                Some(v) => v,
                None => continue,
            };
            let w = match get_f64("Width") {
                Some(v) => v,
                None => continue,
            };
            let h = match get_f64("Height") {
                Some(v) => v,
                None => continue,
            };

            if w < MIN_WINDOW_DIM || h < MIN_WINDOW_DIM {
                continue;
            }

            // Hit test (CG coords: origin top-left).
            if screen_point.x >= x
                && screen_point.x <= x + w
                && cg_point_y >= y
                && cg_point_y <= y + h
            {
                // Get window info.
                let owner_name = dict
                    .find(&CFString::new("kCGWindowOwnerName"))
                    .map(|v| {
                        let s =
                            unsafe { CFString::wrap_under_get_rule(v.as_CFTypeRef() as *const _) };
                        s.to_string()
                    })
                    .unwrap_or_default();

                let window_name = dict
                    .find(&CFString::new("kCGWindowName"))
                    .map(|v| {
                        let s =
                            unsafe { CFString::wrap_under_get_rule(v.as_CFTypeRef() as *const _) };
                        s.to_string()
                    })
                    .unwrap_or_default();

                // Get bundle ID from PID via NSRunningApplication.
                let bundle_id = get_bundle_id_from_pid(
                    dict.find(&pid_key)
                        .map(|v| {
                            let n = unsafe {
                                CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as *const _)
                            };
                            n.to_i32().unwrap_or(0)
                        })
                        .unwrap_or(0),
                );

                // Convert CG bounds (top-left origin) to NSRect (bottom-left origin).
                let ns_y = main_screen_height - y - h;

                return Some(PickerHighlight {
                    bounds: NSRect::new(NSPoint::new(x, ns_y), NSSize::new(w, h)),
                    app_name: owner_name,
                    window_title: window_name,
                    bundle_id,
                });
            }
        }

        None
    }

    /// Get bundle identifier from a PID using NSRunningApplication.
    fn get_bundle_id_from_pid(pid: i32) -> String {
        let app = objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
        match app {
            Some(a) => a
                .bundleIdentifier()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            None => String::new(),
        }
    }

    /// Show the window picker overlay and block until the user selects or cancels.
    ///
    /// MUST be called from the main thread.
    pub fn run_window_picker() -> Option<PickerResult> {
        let mtm = MainThreadMarker::new().expect("window picker must run on main thread");

        // Reset state.
        PICKER_RESULT.with(|r| *r.borrow_mut() = None);
        PICKER_HIGHLIGHT.with(|h| *h.borrow_mut() = None);

        // Get the full screen bounds (union of all screens).
        let screen_frame = full_screen_frame(mtm);

        // Create the overlay panel.
        let panel = create_overlay_panel(mtm, screen_frame);
        PICKER_PANEL.with(|p| *p.borrow_mut() = Some(panel.clone()));

        // Create the highlight view (colored rectangle).
        let highlight_view = create_highlight_view(mtm);
        highlight_view.setHidden(true);
        panel
            .contentView()
            .expect("content view")
            .addSubview(&highlight_view);
        HIGHLIGHT_VIEW.with(|h| *h.borrow_mut() = Some(highlight_view));

        // Set crosshair cursor.
        NSCursor::crosshairCursor().push();

        // Make key and order front.
        panel.makeKeyAndOrderFront(None);

        // Run modal — blocks until stopModal() is called.
        let app = NSApplication::sharedApplication(mtm);
        app.runModalForWindow(&panel);

        // Restore cursor.
        NSCursor::crosshairCursor().pop();

        // Clean up.
        panel.orderOut(None);
        PICKER_PANEL.with(|p| *p.borrow_mut() = None);
        HIGHLIGHT_VIEW.with(|h| *h.borrow_mut() = None);

        // Return result.
        PICKER_RESULT.with(|r| r.borrow_mut().take().flatten())
    }

    /// Get the bounding rect of all screens combined.
    fn full_screen_frame(mtm: MainThreadMarker) -> NSRect {
        let screens = NSScreen::screens(mtm);
        let count = screens.count();
        if count == 0 {
            return NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1920.0, 1080.0));
        }

        let first = screens.objectAtIndex(0);
        let mut frame = first.frame();
        for i in 1..count {
            let s = screens.objectAtIndex(i);
            let sf = s.frame();
            let min_x = frame.origin.x.min(sf.origin.x);
            let min_y = frame.origin.y.min(sf.origin.y);
            let max_x = (frame.origin.x + frame.size.width).max(sf.origin.x + sf.size.width);
            let max_y = (frame.origin.y + frame.size.height).max(sf.origin.y + sf.size.height);
            frame = NSRect::new(
                NSPoint::new(min_x, min_y),
                NSSize::new(max_x - min_x, max_y - min_y),
            );
        }
        frame
    }

    /// Create a borderless, transparent NSPanel at the given frame.
    fn create_overlay_panel(mtm: MainThreadMarker, frame: NSRect) -> Retained<NSPanel> {
        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            frame,
            style,
            objc2_app_kit::NSBackingStoreType::Buffered,
            false,
        );

        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(false);
        // Place above all other windows — use screen saver level to be above everything.
        panel.setLevel(objc2_app_kit::NSScreenSaverWindowLevel + 1);
        panel.setAcceptsMouseMovedEvents(true);
        panel.setIgnoresMouseEvents(false);
        // Make sure the panel collects all events.
        panel.setFloatingPanel(true);
        // Accept key events.
        panel.setBecomesKeyOnlyIfNeeded(false);

        // Install an event monitor for mouse and keyboard events.
        install_event_monitor(mtm, frame);

        panel
    }

    /// Install a local event monitor to handle mouse-moved, mouse-down, and escape.
    fn install_event_monitor(_mtm: MainThreadMarker, screen_frame: NSRect) {
        use objc2_app_kit::NSEventMask;
        use std::ptr::NonNull;

        let mask = NSEventMask::MouseMoved
            | NSEventMask::LeftMouseDown
            | NSEventMask::LeftMouseUp
            | NSEventMask::KeyDown;

        let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let event_ref = unsafe { event.as_ref() };
            let event_type = event_ref.r#type();

            match event_type {
                NSEventType::MouseMoved => {
                    handle_mouse_moved(event_ref, screen_frame);
                    event.as_ptr() // Pass through
                }
                NSEventType::LeftMouseDown | NSEventType::LeftMouseUp => {
                    // On click, capture the current highlight.
                    if event_type == NSEventType::LeftMouseUp {
                        let result = PICKER_HIGHLIGHT.with(|h| {
                            h.borrow().as_ref().map(|hl| PickerResult {
                                bundle_id: hl.bundle_id.clone(),
                                app_name: hl.app_name.clone(),
                                window_title: hl.window_title.clone(),
                            })
                        });
                        PICKER_RESULT.with(|r| *r.borrow_mut() = Some(result));
                        stop_modal();
                    }
                    std::ptr::null_mut() // Consume event
                }
                NSEventType::KeyDown => {
                    let keycode = event_ref.keyCode();
                    if keycode == 53 {
                        // Escape
                        PICKER_RESULT.with(|r| *r.borrow_mut() = Some(None));
                        stop_modal();
                    }
                    std::ptr::null_mut() // Consume event
                }
                _ => event.as_ptr(),
            }
        });

        unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block);
        }
    }

    /// Handle mouse movement — find the window under cursor and update highlight.
    fn handle_mouse_moved(_event: &NSEvent, _screen_frame: NSRect) {
        let mouse_loc = NSEvent::mouseLocation();

        let highlight = window_under_cursor(mouse_loc);

        PICKER_HIGHLIGHT.with(|h| *h.borrow_mut() = highlight.clone());

        // Update the highlight view frame.
        HIGHLIGHT_VIEW.with(|hv| {
            if let Some(view) = hv.borrow().as_ref() {
                if let Some(hl) = &highlight {
                    // Convert screen coords to panel-local coords.
                    let panel_frame =
                        PICKER_PANEL.with(|p| p.borrow().as_ref().map(|panel| panel.frame()));
                    if let Some(pf) = panel_frame {
                        let local_rect = NSRect::new(
                            NSPoint::new(
                                hl.bounds.origin.x - pf.origin.x,
                                hl.bounds.origin.y - pf.origin.y,
                            ),
                            NSSize::new(hl.bounds.size.width, hl.bounds.size.height),
                        );
                        view.setFrame(local_rect);
                        view.setHidden(false);
                        view.setNeedsDisplay(true);
                    }
                } else {
                    view.setHidden(true);
                }
            }
        });
    }

    /// Stop the modal session.
    fn stop_modal() {
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            app.stopModal();
        }
    }

    /// Create a simple colored NSView for the highlight rectangle.
    fn create_highlight_view(mtm: MainThreadMarker) -> Retained<NSView> {
        let view = NSView::initWithFrame(
            mtm.alloc(),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(100.0, 100.0)),
        );
        view.setWantsLayer(true);

        if let Some(layer) = view.layer() {
            // Use Core Graphics to create colors, cast to raw pointers for CALayer msg_send.
            let bg_color = CGColor::rgb(0.0, 0.47, 1.0, 0.2);
            let border_color = CGColor::rgb(0.0, 0.47, 1.0, 0.8);
            let bg_ptr = bg_color.as_concrete_TypeRef() as *const std::ffi::c_void;
            let border_ptr = border_color.as_concrete_TypeRef() as *const std::ffi::c_void;

            unsafe {
                let _: () = objc2::msg_send![&layer, setBackgroundColor: bg_ptr];
                let _: () = objc2::msg_send![&layer, setBorderColor: border_ptr];
                let _: () = objc2::msg_send![&layer, setBorderWidth: 2.0 as CGFloat];
                let _: () = objc2::msg_send![&layer, setCornerRadius: 4.0 as CGFloat];
            }
        }

        view
    }
}

#[cfg(target_os = "macos")]
pub use native::run_window_picker;

#[cfg(not(target_os = "macos"))]
pub fn run_window_picker() -> Option<PickerResult> {
    log::warn!("Window picker is only supported on macOS");
    None
}
