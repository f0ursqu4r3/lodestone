//! macOS ScreenCaptureKit FFI module for display capture.
//!
//! Provides display capture using Apple's ScreenCaptureKit framework with
//! optional PID-based window exclusion (to hide our own app from the capture).
//!
//! This module is only compiled on macOS (`#[cfg(target_os = "macos")]`).

use anyhow::{Result, anyhow};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread, DeclaredClass, Message, define_class, msg_send};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCShareableContent, SCStream, SCStreamConfiguration,
    SCStreamDelegate, SCStreamOutput, SCStreamOutputType, SCWindow,
};
use std::sync::mpsc as std_mpsc;

use super::types::RgbaFrame;

/// Handle to a running ScreenCaptureKit capture session.
///
/// Dropping this handle does **not** automatically stop the capture -- call
/// [`stop_display_capture`] explicitly.
pub struct SCStreamHandle {
    stream: Retained<SCStream>,
    _delegate: Retained<StreamOutputDelegate>,
    screen_index: usize,
}

// SAFETY: SCStream and the delegate are thread-safe ObjC objects managed by ARC.
// We only interact with them through message sends which are themselves thread-safe.
unsafe impl Send for SCStreamHandle {}

/// Ivars for our ObjC delegate that receives captured frames.
struct StreamOutputDelegateIvars {
    frame_tx: std_mpsc::Sender<RgbaFrame>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "LodestoneStreamOutputDelegate"]
    #[ivars = StreamOutputDelegateIvars]
    struct StreamOutputDelegate;

    unsafe impl NSObjectProtocol for StreamOutputDelegate {}

    unsafe impl SCStreamOutput for StreamOutputDelegate {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn stream_didOutputSampleBuffer_ofType(
            &self,
            _stream: &SCStream,
            sample_buffer: &CMSampleBuffer,
            _of_type: SCStreamOutputType,
        ) {
            if let Some(frame) = extract_rgba_frame(sample_buffer) {
                let _ = self.ivars().frame_tx.send(frame);
            }
        }
    }

    unsafe impl SCStreamDelegate for StreamOutputDelegate {
        #[unsafe(method(stream:didStopWithError:))]
        unsafe fn stream_didStopWithError(&self, _stream: &SCStream, error: &NSError) {
            log::warn!("SCStream stopped with error: {}", error);
        }
    }
);

impl StreamOutputDelegate {
    fn new(frame_tx: std_mpsc::Sender<RgbaFrame>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(StreamOutputDelegateIvars { frame_tx });
        unsafe { msg_send![super(this), init] }
    }
}

/// Start capturing a display via ScreenCaptureKit.
///
/// # Arguments
/// * `screen_index` -- index into the list of available displays
/// * `width` -- output pixel width
/// * `height` -- output pixel height
/// * `fps` -- target frames per second
/// * `exclude_own_pid` -- if `true`, windows belonging to this process are excluded
///
/// # Returns
/// A tuple of `(handle, frame_receiver)`. The receiver yields `RgbaFrame`s as
/// they are captured. Call [`stop_display_capture`] with the handle to stop.
pub fn start_display_capture(
    screen_index: usize,
    width: u32,
    height: u32,
    fps: u32,
    exclude_own_pid: bool,
) -> Result<(SCStreamHandle, std_mpsc::Receiver<RgbaFrame>)> {
    // 1. Enumerate shareable content (blocking via channel)
    let content = get_shareable_content()?;

    // 2. Pick the display
    let displays: Retained<NSArray<SCDisplay>> = unsafe { content.displays() };
    let display_count = displays.count();
    if screen_index >= display_count {
        return Err(anyhow!(
            "Screen index {} out of range (only {} displays available)",
            screen_index,
            display_count
        ));
    }
    let display = unsafe { displays.objectAtIndex_unchecked(screen_index) };

    // 3. Build content filter
    let filter = build_content_filter(display, &content, exclude_own_pid)?;

    // 4. Build stream configuration
    let config = build_stream_config(width, height, fps)?;

    // 5. Create frame channel
    let (frame_tx, frame_rx) = std_mpsc::channel();

    // 6. Create delegate (implements both SCStreamOutput and SCStreamDelegate)
    let delegate = StreamOutputDelegate::new(frame_tx);

    // 7. Create SCStream with delegate
    let delegate_for_stream: Retained<ProtocolObject<dyn SCStreamDelegate>> =
        ProtocolObject::from_retained(delegate.clone());
    let stream = unsafe {
        SCStream::initWithFilter_configuration_delegate(
            SCStream::alloc(),
            &filter,
            &config,
            Some(&*delegate_for_stream),
        )
    };

    // 8. Add stream output
    let output_proto: Retained<ProtocolObject<dyn SCStreamOutput>> =
        ProtocolObject::from_retained(delegate.clone());
    unsafe {
        stream
            .addStreamOutput_type_sampleHandlerQueue_error(
                &output_proto,
                SCStreamOutputType::Screen,
                None,
            )
            .map_err(|e| anyhow!("Failed to add stream output: {}", e))?;
    }

    // 9. Start capture (blocking via channel)
    let (start_tx, start_rx) = std_mpsc::channel();
    let start_block = RcBlock::new(move |error: *mut NSError| {
        if error.is_null() {
            let _ = start_tx.send(Ok(()));
        } else {
            let desc = unsafe { (*error).localizedDescription().to_string() };
            let _ = start_tx.send(Err(anyhow!("Failed to start SCStream: {}", desc)));
        }
    });
    unsafe {
        stream.startCaptureWithCompletionHandler(Some(&start_block));
    }
    start_rx
        .recv()
        .map_err(|_| anyhow!("Start capture channel closed"))??;

    let handle = SCStreamHandle {
        stream,
        _delegate: delegate,
        screen_index,
    };

    Ok((handle, frame_rx))
}

/// Stop a running ScreenCaptureKit capture session.
pub fn stop_display_capture(handle: SCStreamHandle) -> Result<()> {
    let (stop_tx, stop_rx) = std_mpsc::channel();
    let stop_block = RcBlock::new(move |error: *mut NSError| {
        if error.is_null() {
            let _ = stop_tx.send(Ok(()));
        } else {
            let desc = unsafe { (*error).localizedDescription().to_string() };
            let _ = stop_tx.send(Err(anyhow!("Failed to stop SCStream: {}", desc)));
        }
    });
    unsafe {
        handle
            .stream
            .stopCaptureWithCompletionHandler(Some(&stop_block));
    }
    stop_rx
        .recv()
        .map_err(|_| anyhow!("Stop capture channel closed"))??;
    Ok(())
}

/// Update the content filter on a running capture to change window exclusion.
///
/// Re-enumerates shareable content to pick up any new/closed windows, rebuilds
/// the filter with the current exclusion setting, and applies it live.
pub fn update_exclusion(handle: &SCStreamHandle, exclude_own_pid: bool) -> Result<()> {
    let content = get_shareable_content()?;
    let displays: Retained<NSArray<SCDisplay>> = unsafe { content.displays() };
    if handle.screen_index >= displays.count() {
        return Err(anyhow!("Display no longer available"));
    }
    let display = unsafe { displays.objectAtIndex_unchecked(handle.screen_index) };
    let filter = build_content_filter(display, &content, exclude_own_pid)?;

    let (tx, rx) = std_mpsc::channel();
    let block = RcBlock::new(move |error: *mut NSError| {
        if error.is_null() {
            let _ = tx.send(Ok(()));
        } else {
            let desc = unsafe { (*error).localizedDescription().to_string() };
            let _ = tx.send(Err(anyhow!("Failed to update content filter: {}", desc)));
        }
    });
    unsafe {
        handle
            .stream
            .updateContentFilter_completionHandler(&filter, Some(&block));
    }
    rx.recv()
        .map_err(|_| anyhow!("Update filter channel closed"))??;
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Fetch shareable content synchronously by bridging the async ObjC callback.
fn get_shareable_content() -> Result<Retained<SCShareableContent>> {
    let (tx, rx) = std_mpsc::channel();
    let block = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            if !error.is_null() {
                let desc = unsafe { (*error).localizedDescription().to_string() };
                let _ = tx.send(Err(anyhow!("SCShareableContent error: {}", desc)));
            } else if content.is_null() {
                let _ = tx.send(Err(anyhow!("SCShareableContent returned null")));
            } else {
                // SAFETY: content is non-null and valid; retain it for ownership.
                let retained = unsafe { Retained::retain(content).expect("non-null content") };
                let _ = tx.send(Ok(retained));
            }
        },
    );
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            false,
            true,
            &block,
        );
    }
    rx.recv()
        .map_err(|_| anyhow!("Shareable content channel closed"))?
}

/// Build an `SCContentFilter` for a display, optionally excluding our own windows.
fn build_content_filter(
    display: &SCDisplay,
    content: &SCShareableContent,
    exclude_own_pid: bool,
) -> Result<Retained<SCContentFilter>> {
    if exclude_own_pid {
        let our_pid = std::process::id() as i32;
        let all_windows: Retained<NSArray<SCWindow>> = unsafe { content.windows() };

        let mut excluded: Vec<Retained<SCWindow>> = Vec::new();
        let count = all_windows.count();
        for i in 0..count {
            let window = unsafe { all_windows.objectAtIndex_unchecked(i) };
            if let Some(app) = unsafe { window.owningApplication() }
                && unsafe { app.processID() } == our_pid
            {
                excluded.push(window.retain());
            }
        }

        let excluded_array = NSArray::from_retained_slice(&excluded);
        let filter = unsafe {
            SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                display,
                &excluded_array,
            )
        };
        Ok(filter)
    } else {
        // Capture the full display with no exclusions
        let empty: Vec<Retained<SCWindow>> = Vec::new();
        let empty_array = NSArray::from_retained_slice(&empty);
        let filter = unsafe {
            SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                display,
                &empty_array,
            )
        };
        Ok(filter)
    }
}

/// Build an `SCStreamConfiguration` with the desired capture parameters.
fn build_stream_config(
    width: u32,
    height: u32,
    fps: u32,
) -> Result<Retained<SCStreamConfiguration>> {
    let config = unsafe { SCStreamConfiguration::new() };
    unsafe {
        config.setWidth(width as usize);
        config.setHeight(height as usize);
        // kCVPixelFormatType_32BGRA = 'BGRA' = 0x42475241
        config.setPixelFormat(0x42475241);
        let cm_time = CMTime {
            value: 1,
            timescale: fps as i32,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        };
        config.setMinimumFrameInterval(cm_time);
        config.setShowsCursor(true);
    }
    Ok(config)
}

/// Extract an `RgbaFrame` from a `CMSampleBuffer`.
///
/// Reads the pixel data from the backing `CVPixelBuffer`, copies it row-by-row
/// (to handle stride/padding), and converts BGRA to RGBA.
fn extract_rgba_frame(sample_buffer: &CMSampleBuffer) -> Option<RgbaFrame> {
    unsafe {
        // Get the CVImageBuffer (which is a CVPixelBuffer for video)
        let pixel_buffer = sample_buffer.image_buffer()?;

        // Lock pixel data for reading
        let lock_status =
            CVPixelBufferLockBaseAddress(&pixel_buffer, CVPixelBufferLockFlags::ReadOnly);
        if lock_status != 0 {
            log::warn!("CVPixelBufferLockBaseAddress failed: {}", lock_status);
            return None;
        }

        let base_addr = CVPixelBufferGetBaseAddress(&pixel_buffer);
        let width = CVPixelBufferGetWidth(&pixel_buffer);
        let height = CVPixelBufferGetHeight(&pixel_buffer);
        let bytes_per_row = CVPixelBufferGetBytesPerRow(&pixel_buffer);

        if base_addr.is_null() || width == 0 || height == 0 {
            CVPixelBufferUnlockBaseAddress(&pixel_buffer, CVPixelBufferLockFlags::ReadOnly);
            return None;
        }

        let stride = width * 4;
        let mut rgba = Vec::with_capacity(width * height * 4);
        let base = base_addr as *const u8;

        for row in 0..height {
            let row_start = row * bytes_per_row;
            let row_ptr = base.add(row_start);
            let row_slice = std::slice::from_raw_parts(row_ptr, stride);
            // BGRA -> RGBA: swap B and R channels
            for pixel in row_slice.chunks_exact(4) {
                rgba.push(pixel[2]); // R
                rgba.push(pixel[1]); // G
                rgba.push(pixel[0]); // B
                rgba.push(pixel[3]); // A
            }
        }

        CVPixelBufferUnlockBaseAddress(&pixel_buffer, CVPixelBufferLockFlags::ReadOnly);

        Some(RgbaFrame {
            data: rgba,
            width: width as u32,
            height: height as u32,
        })
    }
}
