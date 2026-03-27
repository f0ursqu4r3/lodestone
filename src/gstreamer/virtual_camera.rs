//! macOS virtual camera via IOSurface shared memory.
//!
//! Creates an IOSurface that a Camera Extension (CMIO provider) can read from.
//! Frame data is written directly into the surface, and the extension detects
//! new frames by polling the surface seed value. Surface ID and dimensions are
//! published via App Group UserDefaults so the extension can locate the surface.
//!
//! This module is only compiled on macOS (`#[cfg(target_os = "macos")]`).

use std::ffi::c_uint;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow};
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSUserDefaults};
use objc2_io_surface::{
    IOSurface, IOSurfaceLockOptions, IOSurfacePropertyKey, IOSurfacePropertyKeyAllocSize,
    IOSurfacePropertyKeyBytesPerElement, IOSurfacePropertyKeyBytesPerRow,
    IOSurfacePropertyKeyHeight, IOSurfacePropertyKeyPixelFormat, IOSurfacePropertyKeyWidth,
};

use super::types::RgbaFrame;

/// App Group suite name shared between the main app and the camera extension.
const APP_GROUP_SUITE: &str = "group.com.lodestone.app";

/// UserDefaults key for the IOSurface ID.
const UD_KEY_SURFACE_ID: &str = "virtualCameraSurfaceID";
/// UserDefaults key for the surface width.
const UD_KEY_WIDTH: &str = "virtualCameraWidth";
/// UserDefaults key for the surface height.
const UD_KEY_HEIGHT: &str = "virtualCameraHeight";

/// BGRA pixel format as a 32-bit FourCC code (`'BGRA'`).
const PIXEL_FORMAT_BGRA: u32 = 0x42475241;

/// Handle to a running virtual camera session.
///
/// Owns the IOSurface and tracks frame writes. The surface is released when
/// this handle is dropped (via `Retained` reference counting).
pub struct VirtualCameraHandle {
    surface: Retained<IOSurface>,
    width: u32,
    height: u32,
    #[allow(dead_code)]
    fps: u32,
    frame_counter: AtomicU64,
}

// SAFETY: IOSurface is a thread-safe CoreFoundation/ObjC object backed by
// kernel-managed shared memory. All mutations go through IOSurfaceLock which
// provides its own synchronisation.
unsafe impl Send for VirtualCameraHandle {}

/// Start the virtual camera by creating a shared IOSurface and publishing its
/// ID to App Group UserDefaults.
///
/// The returned handle must be kept alive for the duration of the virtual
/// camera session. Call [`stop_virtual_camera`] to tear down cleanly.
pub fn start_virtual_camera(width: u32, height: u32, fps: u32) -> Result<VirtualCameraHandle> {
    let bytes_per_row = width as usize * 4;
    let alloc_size = bytes_per_row * height as usize;

    // Build the properties dictionary for IOSurface creation.
    let surface = unsafe {
        let keys: &[&IOSurfacePropertyKey] = &[
            IOSurfacePropertyKeyWidth,
            IOSurfacePropertyKeyHeight,
            IOSurfacePropertyKeyBytesPerElement,
            IOSurfacePropertyKeyBytesPerRow,
            IOSurfacePropertyKeyAllocSize,
            IOSurfacePropertyKeyPixelFormat,
        ];

        let v_width = NSNumber::numberWithUnsignedInt(width as c_uint);
        let v_height = NSNumber::numberWithUnsignedInt(height as c_uint);
        let v_bpe = NSNumber::numberWithUnsignedInt(4);
        let v_bpr = NSNumber::numberWithUnsignedInt(bytes_per_row as c_uint);
        let v_alloc = NSNumber::numberWithUnsignedInt(alloc_size as c_uint);
        let v_pixel_fmt = NSNumber::numberWithUnsignedInt(PIXEL_FORMAT_BGRA as c_uint);

        let values: &[&AnyObject] = &[&v_width, &v_height, &v_bpe, &v_bpr, &v_alloc, &v_pixel_fmt];

        let props: Retained<NSDictionary<IOSurfacePropertyKey, AnyObject>> =
            NSDictionary::from_slices(keys, values);

        IOSurface::initWithProperties(IOSurface::alloc(), &props)
            .ok_or_else(|| anyhow!("IOSurface::initWithProperties returned nil"))?
    };

    // Read back the surface ID (kernel-assigned global token).
    let surface_id = surface.surfaceID();
    log::info!(
        "Virtual camera IOSurface created: id={}, {}x{}, {} bytes",
        surface_id,
        width,
        height,
        alloc_size,
    );

    // Publish surface info to App Group UserDefaults so the camera extension
    // can find the surface via IOSurfaceLookup.
    publish_to_user_defaults(surface_id, width, height)?;

    Ok(VirtualCameraHandle {
        surface,
        width,
        height,
        fps,
        frame_counter: AtomicU64::new(0),
    })
}

/// Write an RGBA frame into the shared IOSurface, converting to BGRA in-place.
///
/// If the frame dimensions are smaller than the surface, the data is written
/// to the top-left region; extra surface area is left unchanged.
pub fn write_frame(handle: &VirtualCameraHandle, frame: &RgbaFrame) -> Result<()> {
    let surface = &handle.surface;

    // Lock the surface for CPU writing.
    let lock_result = surface.lockWithOptions_seed(IOSurfaceLockOptions(0), ptr::null_mut());
    if lock_result != 0 {
        return Err(anyhow!(
            "IOSurfaceLock failed with kern_return_t {}",
            lock_result
        ));
    }

    // Write pixel data with RGBA -> BGRA swizzle using u32 operations.
    // Processing whole pixels at once is ~4x faster than per-byte swaps,
    // especially in debug builds where the inner loop is not vectorized.
    unsafe {
        let base = surface.baseAddress().as_ptr() as *mut u8;
        let surface_bpr = surface.bytesPerRow() as usize;

        let copy_width = frame.width.min(handle.width) as usize;
        let copy_height = frame.height.min(handle.height) as usize;
        let src_bpr = frame.width as usize * 4;

        for row in 0..copy_height {
            let src_row = frame.data.as_ptr().add(row * src_bpr) as *const u32;
            let dst_row = base.add(row * surface_bpr) as *mut u32;

            for col in 0..copy_width {
                // RGBA (0xAABBGGRR in little-endian) -> BGRA (0xAARRGGBB)
                let rgba = *src_row.add(col);
                let r = rgba & 0xFF;
                let b = (rgba >> 16) & 0xFF;
                let bgra = (rgba & 0xFF00FF00) | (r << 16) | b;
                *dst_row.add(col) = bgra;
            }
        }
    }

    // Unlock the surface — this also increments the seed, signalling the
    // extension that a new frame is available.
    let unlock_result = surface.unlockWithOptions_seed(IOSurfaceLockOptions(0), ptr::null_mut());
    if unlock_result != 0 {
        return Err(anyhow!(
            "IOSurfaceUnlock failed with kern_return_t {}",
            unlock_result
        ));
    }

    handle.frame_counter.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Stop the virtual camera and clean up shared state.
///
/// Clears the UserDefaults entries so the camera extension stops reading.
/// The IOSurface itself is released when the `Retained<IOSurface>` inside
/// the handle is dropped.
pub fn stop_virtual_camera(handle: VirtualCameraHandle) -> Result<()> {
    log::info!(
        "Stopping virtual camera (wrote {} frames)",
        handle.frame_counter.load(Ordering::Relaxed),
    );

    // Clear UserDefaults so the extension knows we're done.
    clear_user_defaults()?;

    // The IOSurface is dropped here when `handle` goes out of scope.
    drop(handle);
    Ok(())
}

/// Publish the IOSurface ID and dimensions to App Group UserDefaults.
fn publish_to_user_defaults(surface_id: u32, width: u32, height: u32) -> Result<()> {
    let defaults = user_defaults()?;
    let key_id = NSString::from_str(UD_KEY_SURFACE_ID);
    let key_w = NSString::from_str(UD_KEY_WIDTH);
    let key_h = NSString::from_str(UD_KEY_HEIGHT);

    defaults.setInteger_forKey(surface_id as isize, &key_id);
    defaults.setInteger_forKey(width as isize, &key_w);
    defaults.setInteger_forKey(height as isize, &key_h);

    log::debug!(
        "Published virtual camera to UserDefaults: surfaceID={}, {}x{}",
        surface_id,
        width,
        height,
    );
    Ok(())
}

/// Clear all virtual camera keys from App Group UserDefaults.
fn clear_user_defaults() -> Result<()> {
    let defaults = user_defaults()?;
    let key_id = NSString::from_str(UD_KEY_SURFACE_ID);
    let key_w = NSString::from_str(UD_KEY_WIDTH);
    let key_h = NSString::from_str(UD_KEY_HEIGHT);

    defaults.setInteger_forKey(0, &key_id);
    defaults.setInteger_forKey(0, &key_w);
    defaults.setInteger_forKey(0, &key_h);

    log::debug!("Cleared virtual camera UserDefaults entries");
    Ok(())
}

/// Get a handle to the App Group UserDefaults suite.
fn user_defaults() -> Result<Retained<NSUserDefaults>> {
    let suite = NSString::from_str(APP_GROUP_SUITE);
    NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&suite))
        .ok_or_else(|| anyhow!("Failed to open UserDefaults suite '{}'", APP_GROUP_SUITE))
}
