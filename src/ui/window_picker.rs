//! macOS window picker overlay.
//!
//! Shows a fullscreen transparent overlay that highlights windows under the
//! cursor. Clicking selects the window; pressing Escape cancels.
//!
//! Uses native AppKit via `objc2` — no winit/wgpu involvement.
//! Non-blocking: call `start_window_picker()` to open, `poll_window_picker()`
//! each frame to update highlight + check for result.

/// Result returned when the user clicks a window in the picker.
#[derive(Debug, Clone)]
pub struct PickerResult {
    pub bundle_id: String,
    pub app_name: String,
    pub window_title: String,
    pub width: u32,
    pub height: u32,
}

// ─── Native implementation (macOS) ───────────────────────────────────────────

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionaryRef;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        CGWindowListCopyWindowInfo, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly,
    };
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSColor, NSCursor, NSEvent, NSPanel, NSScreen, NSView, NSWindowStyleMask,
    };
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
    use std::cell::RefCell;

    /// Highlight state for the window currently under the cursor.
    #[derive(Debug, Clone)]
    struct PickerHighlight {
        bounds: NSRect,
        app_name: String,
        window_title: String,
        bundle_id: String,
    }

    const MIN_WINDOW_DIM: f64 = 50.0;

    thread_local! {
        /// `Some(Some(...))` = selected, `Some(None)` = cancelled, `None` = still picking.
        static PICKER_RESULT: RefCell<Option<Option<PickerResult>>> = const { RefCell::new(None) };
        static PICKER_HIGHLIGHT: RefCell<Option<PickerHighlight>> = const { RefCell::new(None) };
        static PICKER_PANEL: RefCell<Option<Retained<NSPanel>>> = const { RefCell::new(None) };
        static HIGHLIGHT_VIEW: RefCell<Option<Retained<NSView>>> = const { RefCell::new(None) };
        static EVENT_MONITORS: RefCell<Vec<Retained<objc2::runtime::AnyObject>>> = const { RefCell::new(Vec::new()) };
    }

    /// Open the window picker overlay. Non-blocking — returns immediately.
    /// Call `poll_window_picker()` each frame to update and check for result.
    pub fn start_window_picker() {
        let mtm = MainThreadMarker::new().expect("window picker must run on main thread");

        // Reset state.
        PICKER_RESULT.with(|r| *r.borrow_mut() = None);
        PICKER_HIGHLIGHT.with(|h| *h.borrow_mut() = None);

        let screen_frame = full_screen_frame(mtm);
        let panel = create_overlay_panel(mtm, screen_frame);
        PICKER_PANEL.with(|p| *p.borrow_mut() = Some(panel.clone()));

        let highlight_view = create_highlight_view(mtm);
        highlight_view.setHidden(true);
        panel
            .contentView()
            .expect("content view")
            .addSubview(&highlight_view);
        HIGHLIGHT_VIEW.with(|h| *h.borrow_mut() = Some(highlight_view));

        NSCursor::crosshairCursor().push();
        panel.makeKeyAndOrderFront(None);
        install_event_monitors();
    }

    /// Poll the picker state. Call each frame while `window_picker_active` is true.
    /// Updates the highlight rectangle under the cursor.
    /// Returns `Some(result)` when the user clicks or cancels, `None` if still picking.
    pub fn poll_window_picker() -> Option<Option<PickerResult>> {
        // Update highlight based on current mouse position.
        let mouse_loc = NSEvent::mouseLocation();
        let highlight = window_under_cursor(mouse_loc);
        update_highlight_view(&highlight);
        PICKER_HIGHLIGHT.with(|h| *h.borrow_mut() = highlight);

        // Check if a result was set by the event monitors.
        PICKER_RESULT.with(|r| {
            let result = r.borrow();
            if result.is_some() {
                // Clone the result and return it; cleanup happens in stop_window_picker.
                result.clone()
            } else {
                None
            }
        })
    }

    /// Close the picker overlay and clean up. Call after `poll_window_picker` returns `Some`.
    pub fn stop_window_picker() {
        NSCursor::crosshairCursor().pop();
        remove_event_monitors();

        PICKER_PANEL.with(|p| {
            if let Some(panel) = p.borrow().as_ref() {
                panel.orderOut(None);
            }
            *p.borrow_mut() = None;
        });
        HIGHLIGHT_VIEW.with(|h| *h.borrow_mut() = None);
        PICKER_RESULT.with(|r| *r.borrow_mut() = None);
        PICKER_HIGHLIGHT.with(|h| *h.borrow_mut() = None);
    }

    // ── Hit testing ──────────────────────────────────────────────────────────

    fn window_under_cursor(screen_point: NSPoint) -> Option<PickerHighlight> {
        let own_pid = std::process::id() as i64;

        let main_screen_height = {
            let screens = NSScreen::screens(MainThreadMarker::new().expect("main thread"));
            if screens.count() == 0 {
                return None;
            }
            screens.objectAtIndex(0).frame().size.height
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

            let pid_key = CFString::new("kCGWindowOwnerPID");
            if let Some(pid_val) = dict.find(&pid_key) {
                let pid_num =
                    unsafe { CFNumber::wrap_under_get_rule(pid_val.as_CFTypeRef() as *const _) };
                if pid_num.to_i64() == Some(own_pid) {
                    continue;
                }
            }

            let layer_key = CFString::new("kCGWindowLayer");
            if let Some(layer_val) = dict.find(&layer_key) {
                let layer_num =
                    unsafe { CFNumber::wrap_under_get_rule(layer_val.as_CFTypeRef() as *const _) };
                if layer_num.to_i32() != Some(0) {
                    continue;
                }
            }

            let bounds_key = CFString::new("kCGWindowBounds");
            let Some(bounds_val) = dict.find(&bounds_key) else {
                continue;
            };
            let bounds_dict = unsafe {
                core_foundation::dictionary::CFDictionary::<CFString, CFType>::wrap_under_get_rule(
                    bounds_val.as_CFTypeRef() as *const _,
                )
            };

            let get_f64 = |key: &str| -> Option<f64> {
                let v = bounds_dict.find(&CFString::new(key))?;
                unsafe { CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as *const _) }.to_f64()
            };

            let (x, y, w, h) = match (get_f64("X"), get_f64("Y"), get_f64("Width"), get_f64("Height")) {
                (Some(x), Some(y), Some(w), Some(h)) => (x, y, w, h),
                _ => continue,
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
                let owner_name = dict
                    .find(&CFString::new("kCGWindowOwnerName"))
                    .map(|v| unsafe { CFString::wrap_under_get_rule(v.as_CFTypeRef() as *const _) }.to_string())
                    .unwrap_or_default();

                let window_name = dict
                    .find(&CFString::new("kCGWindowName"))
                    .map(|v| unsafe { CFString::wrap_under_get_rule(v.as_CFTypeRef() as *const _) }.to_string())
                    .unwrap_or_default();

                let bundle_id = get_bundle_id_from_pid(
                    dict.find(&pid_key)
                        .and_then(|v| unsafe { CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as *const _) }.to_i32())
                        .unwrap_or(0),
                );

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

    fn get_bundle_id_from_pid(pid: i32) -> String {
        objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
            .and_then(|a| a.bundleIdentifier().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    // ── Overlay panel ────────────────────────────────────────────────────────

    fn full_screen_frame(mtm: MainThreadMarker) -> NSRect {
        let screens = NSScreen::screens(mtm);
        let count = screens.count();
        if count == 0 {
            return NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1920.0, 1080.0));
        }
        let mut frame = screens.objectAtIndex(0).frame();
        for i in 1..count {
            let sf = screens.objectAtIndex(i).frame();
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
        panel.setLevel(objc2_app_kit::NSScreenSaverWindowLevel + 1);
        panel.setAcceptsMouseMovedEvents(true);
        panel.setIgnoresMouseEvents(false);
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(false);
        panel
    }

    // ── Event monitors ───────────────────────────────────────────────────────

    fn install_event_monitors() {
        use objc2_app_kit::NSEventMask;
        use std::ptr::NonNull;

        // Global monitor: clicks on other apps / other monitors.
        let global_block = block2::RcBlock::new(move |event: NonNull<NSEvent>| {
            let event_ref = unsafe { event.as_ref() };
            if event_ref.r#type() == objc2_app_kit::NSEventType::LeftMouseUp {
                let result = PICKER_HIGHLIGHT.with(|h| {
                    h.borrow().as_ref().map(|hl| PickerResult {
                        bundle_id: hl.bundle_id.clone(),
                        app_name: hl.app_name.clone(),
                        window_title: hl.window_title.clone(),
                        width: hl.bounds.size.width as u32,
                        height: hl.bounds.size.height as u32,
                    })
                });
                PICKER_RESULT.with(|r| *r.borrow_mut() = Some(result));
            }
        });
        if let Some(m) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseUp,
            &global_block,
        ) {
            EVENT_MONITORS.with(|monitors| monitors.borrow_mut().push(m));
        }

        // Local monitor: clicks on our overlay panel.
        let local_click = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let _event_ref = unsafe { event.as_ref() };
            let result = PICKER_HIGHLIGHT.with(|h| {
                h.borrow().as_ref().map(|hl| PickerResult {
                    bundle_id: hl.bundle_id.clone(),
                    app_name: hl.app_name.clone(),
                    window_title: hl.window_title.clone(),
                    width: hl.bounds.size.width as u32,
                    height: hl.bounds.size.height as u32,
                })
            });
            PICKER_RESULT.with(|r| *r.borrow_mut() = Some(result));
            std::ptr::null_mut()
        });
        if let Some(m) = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::LeftMouseUp,
                &local_click,
            )
        } {
            EVENT_MONITORS.with(|monitors| monitors.borrow_mut().push(m));
        }

        // Local monitor: Escape key.
        let local_key = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let event_ref = unsafe { event.as_ref() };
            if event_ref.keyCode() == 53 {
                PICKER_RESULT.with(|r| *r.borrow_mut() = Some(None));
            }
            std::ptr::null_mut()
        });
        if let Some(m) = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &local_key)
        } {
            EVENT_MONITORS.with(|monitors| monitors.borrow_mut().push(m));
        }
    }

    fn remove_event_monitors() {
        EVENT_MONITORS.with(|monitors| {
            for monitor in monitors.borrow_mut().drain(..) {
                unsafe { NSEvent::removeMonitor(&monitor) };
            }
        });
    }

    // ── Highlight view ───────────────────────────────────────────────────────

    fn update_highlight_view(highlight: &Option<PickerHighlight>) {
        HIGHLIGHT_VIEW.with(|hv| {
            if let Some(view) = hv.borrow().as_ref() {
                if let Some(hl) = highlight {
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

    fn create_highlight_view(mtm: MainThreadMarker) -> Retained<NSView> {
        let view = NSView::initWithFrame(
            mtm.alloc(),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(100.0, 100.0)),
        );
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            let bg_color = objc2_core_graphics::CGColor::new_srgb(0.0, 0.47, 1.0, 0.2);
            let border_color = objc2_core_graphics::CGColor::new_srgb(0.0, 0.47, 1.0, 0.8);
            layer.setBackgroundColor(Some(&bg_color));
            layer.setBorderColor(Some(&border_color));
            layer.setBorderWidth(2.0);
            layer.setCornerRadius(4.0);
        }
        view
    }
}

#[cfg(target_os = "macos")]
pub use native::{poll_window_picker, start_window_picker, stop_window_picker};

#[cfg(not(target_os = "macos"))]
pub fn start_window_picker() {}
#[cfg(not(target_os = "macos"))]
pub fn poll_window_picker() -> Option<Option<PickerResult>> {
    Some(None)
}
#[cfg(not(target_os = "macos"))]
pub fn stop_window_picker() {}
