//! Game capture DLL injection, shared memory management, and frame reading.
//!
//! This module handles the host side of game capture: creating shared memory
//! for frame transfer, injecting the hook DLL into the target game process,
//! reading captured frames, and cleanup.

#![cfg(target_os = "windows")]

use anyhow::{Context, Result, bail};
use std::ffi::c_void;
use std::path::Path;

use super::types::RgbaFrame;

// ---------------------------------------------------------------------------
// Win32 FFI (inline, following the pattern in devices.rs)
// ---------------------------------------------------------------------------

mod win32 {
    use std::ffi::c_void;

    pub type HANDLE = *mut c_void;
    pub type DWORD = u32;

    pub const PROCESS_ALL_ACCESS: DWORD = 0x001F_0FFF;
    pub const PROCESS_QUERY_LIMITED_INFORMATION: DWORD = 0x1000;
    pub const MEM_COMMIT: DWORD = 0x1000;
    pub const MEM_RESERVE: DWORD = 0x2000;
    pub const MEM_RELEASE: DWORD = 0x8000;
    pub const PAGE_READWRITE: DWORD = 0x04;
    pub const FILE_MAP_ALL_ACCESS: DWORD = 0x000F_001F;
    pub const EVENT_ALL_ACCESS: DWORD = 0x001F_0003;
    pub const WAIT_OBJECT_0: DWORD = 0;
    pub const STILL_ACTIVE: DWORD = 259;
    pub const INVALID_HANDLE_VALUE: HANDLE = -1_isize as HANDLE;
    pub const INFINITE: DWORD = 0xFFFF_FFFF;

    unsafe extern "system" {
        pub fn OpenProcess(access: DWORD, inherit: i32, pid: DWORD) -> HANDLE;
        pub fn CloseHandle(handle: HANDLE) -> i32;
        pub fn GetCurrentProcessId() -> DWORD;
        pub fn GetExitCodeProcess(process: HANDLE, exit_code: *mut DWORD) -> i32;

        // Shared memory
        pub fn CreateFileMappingW(
            file: HANDLE,
            security: *const u8,
            protect: DWORD,
            high: DWORD,
            low: DWORD,
            name: *const u16,
        ) -> HANDLE;
        pub fn MapViewOfFile(
            mapping: HANDLE,
            access: DWORD,
            high: DWORD,
            low: DWORD,
            bytes: usize,
        ) -> *mut u8;
        pub fn UnmapViewOfFile(addr: *const u8) -> i32;

        // Events
        pub fn CreateEventW(
            security: *const u8,
            manual_reset: i32,
            initial: i32,
            name: *const u16,
        ) -> HANDLE;
        pub fn SetEvent(event: HANDLE) -> i32;
        pub fn WaitForSingleObject(handle: HANDLE, millis: DWORD) -> DWORD;

        // DLL injection
        pub fn VirtualAllocEx(
            process: HANDLE,
            addr: *const c_void,
            size: usize,
            alloc_type: DWORD,
            protect: DWORD,
        ) -> *mut c_void;
        pub fn VirtualFreeEx(
            process: HANDLE,
            addr: *mut c_void,
            size: usize,
            free_type: DWORD,
        ) -> i32;
        pub fn WriteProcessMemory(
            process: HANDLE,
            base: *mut c_void,
            buffer: *const c_void,
            size: usize,
            written: *mut usize,
        ) -> i32;
        pub fn CreateRemoteThread(
            process: HANDLE,
            security: *const u8,
            stack: usize,
            start: *const c_void,
            param: *mut c_void,
            flags: DWORD,
            id: *mut DWORD,
        ) -> HANDLE;
        pub fn GetModuleHandleW(name: *const u16) -> HANDLE;
        pub fn GetProcAddress(module: HANDLE, name: *const u8) -> *const c_void;
        pub fn GetLastError() -> DWORD;
    }
}

use win32::*;

// Shared memory header — must match lodestone-hook/src/shared.rs exactly.
const CAPTURE_MAGIC: u32 = 0x45_44_4F_4C;
const CAPTURE_VERSION: u32 = 1;
const MAX_CAPTURE_WIDTH: u32 = 3840;
const MAX_CAPTURE_HEIGHT: u32 = 2160;
const BYTES_PER_PIXEL: u32 = 4;
const BUFFER_SIZE: usize =
    (MAX_CAPTURE_WIDTH as usize) * (MAX_CAPTURE_HEIGHT as usize) * (BYTES_PER_PIXEL as usize);
const HEADER_SIZE: usize = std::mem::size_of::<SharedCaptureHeader>();
const SHARED_MEM_SIZE: usize = HEADER_SIZE + BUFFER_SIZE * 2;

/// Must be kept in sync with `lodestone_hook::shared::SharedCaptureHeader`.
#[repr(C)]
struct SharedCaptureHeader {
    magic: u32,
    version: u32,
    width: u32,
    height: u32,
    pitch: u32,
    format: u32,
    frame_index: u64,
    active_buffer: u32,
    shutdown: u32,
    parent_pid: u32,
    _padding: [u32; 3],
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Handles to shared memory and events for a game capture session.
pub struct SharedCaptureHandles {
    pub file_mapping: HANDLE,
    pub mapped_ptr: *mut u8,
    pub ready_event: HANDLE,
    pub shutdown_event: HANDLE,
    pub process_handle: HANDLE,
    pub process_id: u32,
}

// Safety: the handles are opaque kernel objects, valid to send across threads.
unsafe impl Send for SharedCaptureHandles {}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn encode_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0u16)).collect()
}

// ---------------------------------------------------------------------------
// Shared memory + events
// ---------------------------------------------------------------------------

/// Create the shared memory region and signalling events for a game capture.
///
/// The shared memory is named `Local\LodestoneCapture_{pid}` and the events
/// are `Local\LodestoneCaptureReady_{pid}` and
/// `Local\LodestoneCaptureShutdown_{pid}`.
pub fn create_shared_capture(target_pid: u32) -> Result<SharedCaptureHandles> {
    let shm_name = encode_wide(&format!("Local\\LodestoneCapture_{target_pid}"));
    let ready_name = encode_wide(&format!("Local\\LodestoneCaptureReady_{target_pid}"));
    let shutdown_name = encode_wide(&format!("Local\\LodestoneCaptureShutdown_{target_pid}"));

    // Create shared memory.
    let file_mapping = unsafe {
        CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            std::ptr::null(),
            PAGE_READWRITE,
            0,
            SHARED_MEM_SIZE as u32,
            shm_name.as_ptr(),
        )
    };
    if file_mapping.is_null() {
        bail!(
            "CreateFileMappingW failed (error {})",
            unsafe { GetLastError() }
        );
    }

    let mapped = unsafe { MapViewOfFile(file_mapping, FILE_MAP_ALL_ACCESS, 0, 0, SHARED_MEM_SIZE) };
    if mapped.is_null() {
        unsafe { CloseHandle(file_mapping) };
        bail!("MapViewOfFile failed");
    }

    // Initialise the header.
    let header = mapped as *mut SharedCaptureHeader;
    unsafe {
        (*header).magic = CAPTURE_MAGIC;
        (*header).version = CAPTURE_VERSION;
        (*header).width = 0;
        (*header).height = 0;
        (*header).pitch = 0;
        (*header).format = 0;
        (*header).frame_index = 0;
        (*header).active_buffer = 0;
        (*header).shutdown = 0;
        (*header).parent_pid = GetCurrentProcessId();
        (*header)._padding = [0; 3];
    }

    // Create events (auto-reset for ready, manual-reset for shutdown).
    let ready_event =
        unsafe { CreateEventW(std::ptr::null(), 0, 0, ready_name.as_ptr()) }; // auto-reset
    let shutdown_event =
        unsafe { CreateEventW(std::ptr::null(), 1, 0, shutdown_name.as_ptr()) }; // manual-reset

    if ready_event.is_null() || shutdown_event.is_null() {
        unsafe {
            UnmapViewOfFile(mapped);
            CloseHandle(file_mapping);
            if !ready_event.is_null() {
                CloseHandle(ready_event);
            }
            if !shutdown_event.is_null() {
                CloseHandle(shutdown_event);
            }
        }
        bail!("CreateEventW failed");
    }

    // Open a handle to the target process for liveness checking.
    let process_handle =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, target_pid) };

    Ok(SharedCaptureHandles {
        file_mapping,
        mapped_ptr: mapped,
        ready_event,
        shutdown_event,
        process_handle,
        process_id: target_pid,
    })
}

// ---------------------------------------------------------------------------
// DLL injection
// ---------------------------------------------------------------------------

/// Inject the hook DLL into the target process via `CreateRemoteThread` +
/// `LoadLibraryW`.
pub fn inject_hook_dll(target_pid: u32, dll_path: &Path) -> Result<()> {
    let process = unsafe { OpenProcess(PROCESS_ALL_ACCESS, 0, target_pid) };
    if process.is_null() {
        let err = unsafe { GetLastError() };
        if err == 5 {
            bail!(
                "Access denied — the game may be running as administrator or \
                 is protected by anti-cheat. Try running Lodestone as administrator."
            );
        }
        bail!("OpenProcess failed (error {err})");
    }

    let result = inject_into_process(process, dll_path);

    unsafe { CloseHandle(process) };
    result
}

fn inject_into_process(process: HANDLE, dll_path: &Path) -> Result<()> {
    // Encode the DLL path as UTF-16.
    let dll_path_str = dll_path
        .to_str()
        .context("DLL path contains invalid Unicode")?;
    let wide_path = encode_wide(dll_path_str);
    let path_bytes = wide_path.len() * 2; // size in bytes

    // Allocate memory in the target process for the path string.
    let remote_mem = unsafe {
        VirtualAllocEx(
            process,
            std::ptr::null(),
            path_bytes,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };
    if remote_mem.is_null() {
        bail!("VirtualAllocEx failed (error {})", unsafe { GetLastError() });
    }

    // Write the path into the allocated memory.
    let mut written: usize = 0;
    let ok = unsafe {
        WriteProcessMemory(
            process,
            remote_mem,
            wide_path.as_ptr() as *const c_void,
            path_bytes,
            &mut written,
        )
    };
    if ok == 0 {
        unsafe { VirtualFreeEx(process, remote_mem, 0, MEM_RELEASE) };
        bail!("WriteProcessMemory failed (error {})", unsafe {
            GetLastError()
        });
    }

    // Get the address of LoadLibraryW in kernel32.dll.
    let kernel32 = encode_wide("kernel32.dll");
    let k32 = unsafe { GetModuleHandleW(kernel32.as_ptr()) };
    if k32.is_null() {
        unsafe { VirtualFreeEx(process, remote_mem, 0, MEM_RELEASE) };
        bail!("Could not find kernel32.dll");
    }

    let load_library = unsafe { GetProcAddress(k32, b"LoadLibraryW\0".as_ptr()) };
    if load_library.is_null() {
        unsafe { VirtualFreeEx(process, remote_mem, 0, MEM_RELEASE) };
        bail!("Could not find LoadLibraryW");
    }

    // Create a remote thread that calls LoadLibraryW(dll_path).
    let thread = unsafe {
        CreateRemoteThread(
            process,
            std::ptr::null(),
            0,
            load_library,
            remote_mem,
            0,
            std::ptr::null_mut(),
        )
    };
    if thread.is_null() {
        unsafe { VirtualFreeEx(process, remote_mem, 0, MEM_RELEASE) };
        bail!(
            "CreateRemoteThread failed (error {}) — anti-cheat may be blocking injection",
            unsafe { GetLastError() }
        );
    }

    // Wait for LoadLibraryW to finish (10 second timeout).
    let wait = unsafe { WaitForSingleObject(thread, 10_000) };
    unsafe { CloseHandle(thread) };
    // Free the remote memory (the DLL path string).
    unsafe { VirtualFreeEx(process, remote_mem, 0, MEM_RELEASE) };

    if wait != WAIT_OBJECT_0 {
        bail!("LoadLibraryW timed out in target process");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Frame reading
// ---------------------------------------------------------------------------

/// Try to read the latest frame from shared memory. Returns `None` if no new
/// frame is available since `last_frame_index`.
///
/// On success, updates `*last_frame_index` to the new value.
pub fn try_read_frame(
    handles: &SharedCaptureHandles,
    last_frame_index: &mut u64,
) -> Option<RgbaFrame> {
    // Non-blocking check for the ready event.
    let wait = unsafe { WaitForSingleObject(handles.ready_event, 0) };
    if wait != WAIT_OBJECT_0 {
        return None;
    }

    let header = handles.mapped_ptr as *const SharedCaptureHeader;
    let h = unsafe { &*header };

    // Skip if no new frame.
    if h.frame_index <= *last_frame_index {
        return None;
    }

    let width = h.width;
    let height = h.height;

    if width == 0 || height == 0 || width > MAX_CAPTURE_WIDTH || height > MAX_CAPTURE_HEIGHT {
        return None;
    }

    // Read from the buffer the hook is NOT writing to.
    let read_buffer = h.active_buffer ^ 1;
    let buf_offset = HEADER_SIZE + (read_buffer as usize) * BUFFER_SIZE;
    let src_pitch = h.pitch as usize;
    let dst_pitch = (width as usize) * (BYTES_PER_PIXEL as usize);
    let frame_bytes = dst_pitch * (height as usize);

    let mut data = vec![0u8; frame_bytes];
    let src_base = unsafe { handles.mapped_ptr.add(buf_offset) };

    // Copy row-by-row (pitch may differ), converting BGRA → RGBA.
    for row in 0..(height as usize) {
        let src_row = unsafe { src_base.add(row * src_pitch) };
        let dst_start = row * dst_pitch;
        for px in 0..(width as usize) {
            let si = px * 4;
            let di = dst_start + px * 4;
            unsafe {
                data[di] = *src_row.add(si + 2); // R ← B
                data[di + 1] = *src_row.add(si + 1); // G ← G
                data[di + 2] = *src_row.add(si); // B ← R
                data[di + 3] = *src_row.add(si + 3); // A ← A
            }
        }
    }

    *last_frame_index = h.frame_index;

    Some(RgbaFrame {
        data,
        width,
        height,
    })
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Signal the hook DLL to unhook and unload.
pub fn signal_shutdown(handles: &SharedCaptureHandles) {
    // Set the shutdown flag in shared memory.
    let header = handles.mapped_ptr as *mut SharedCaptureHeader;
    unsafe {
        (*header).shutdown = 1;
    }
    // Also signal the shutdown event.
    unsafe { SetEvent(handles.shutdown_event) };
}

/// Check if the target process is still alive.
pub fn is_process_alive(handles: &SharedCaptureHandles) -> bool {
    if handles.process_handle.is_null() {
        return false;
    }
    let mut exit_code: u32 = 0;
    let ok = unsafe { GetExitCodeProcess(handles.process_handle, &mut exit_code) };
    ok != 0 && exit_code == STILL_ACTIVE
}

/// Clean up all handles. Call after `signal_shutdown` + a brief wait.
pub fn cleanup_shared_capture(handles: SharedCaptureHandles) {
    unsafe {
        if !handles.mapped_ptr.is_null() {
            UnmapViewOfFile(handles.mapped_ptr);
        }
        if !handles.file_mapping.is_null() {
            CloseHandle(handles.file_mapping);
        }
        if !handles.ready_event.is_null() {
            CloseHandle(handles.ready_event);
        }
        if !handles.shutdown_event.is_null() {
            CloseHandle(handles.shutdown_event);
        }
        if !handles.process_handle.is_null() {
            CloseHandle(handles.process_handle);
        }
    }
}
