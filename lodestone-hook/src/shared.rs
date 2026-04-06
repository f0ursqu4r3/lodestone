/// Shared memory layout for game capture frame transfer between the hook DLL
/// (running inside the game process) and the Lodestone host process.
///
/// Memory layout:
/// ```text
/// [ SharedCaptureHeader ]          — 64 bytes
/// [ buffer_0: MAX_W×MAX_H×4 ]     — pixel data (BGRA8)
/// [ buffer_1: MAX_W×MAX_H×4 ]     — pixel data (BGRA8)
/// ```
///
/// Double-buffered: the hook writes to `active_buffer`, the host reads from the
/// other buffer. The hook flips `active_buffer` after finishing a write, then
/// signals the ready event.

/// Maximum capture resolution. Preallocated in shared memory.
pub const MAX_CAPTURE_WIDTH: u32 = 3840;
pub const MAX_CAPTURE_HEIGHT: u32 = 2160;
pub const BYTES_PER_PIXEL: u32 = 4;
pub const BUFFER_SIZE: usize =
    (MAX_CAPTURE_WIDTH as usize) * (MAX_CAPTURE_HEIGHT as usize) * (BYTES_PER_PIXEL as usize);

/// Total shared memory size: header + two pixel buffers.
pub const SHARED_MEM_SIZE: usize = size_of::<SharedCaptureHeader>() + BUFFER_SIZE * 2;

/// Magic value for validation ("LODE" in little-endian).
pub const CAPTURE_MAGIC: u32 = 0x45_44_4F_4C;

/// Protocol version.
pub const CAPTURE_VERSION: u32 = 1;

/// Pixel format identifier.
pub const FORMAT_BGRA8: u32 = 0;

use core::mem::size_of;

#[repr(C)]
pub struct SharedCaptureHeader {
    /// Must be [`CAPTURE_MAGIC`].
    pub magic: u32,
    /// Must be [`CAPTURE_VERSION`].
    pub version: u32,
    /// Actual frame width in pixels (≤ [`MAX_CAPTURE_WIDTH`]).
    pub width: u32,
    /// Actual frame height in pixels (≤ [`MAX_CAPTURE_HEIGHT`]).
    pub height: u32,
    /// Row pitch in bytes (may include padding).
    pub pitch: u32,
    /// Pixel format — currently always [`FORMAT_BGRA8`].
    pub format: u32,
    /// Monotonically increasing frame counter. Bumped each time the hook
    /// finishes writing a frame.
    pub frame_index: u64,
    /// Which buffer (0 or 1) the hook is *currently writing to*.
    /// The host should read from `active_buffer ^ 1`.
    pub active_buffer: u32,
    /// Set to nonzero by the host to tell the hook to unhook and unload.
    pub shutdown: u32,
    /// PID of the Lodestone host process. The hook's watchdog thread monitors
    /// this — if the parent dies, the hook self-unloads.
    pub parent_pid: u32,
    /// Reserved for future use.
    pub _padding: [u32; 3],
}

impl SharedCaptureHeader {
    /// Byte offset of `buffer_0` from the start of shared memory.
    pub const fn buffer_offset(buffer_index: u32) -> usize {
        size_of::<Self>() + (buffer_index as usize) * BUFFER_SIZE
    }
}
