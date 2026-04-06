#![cfg(target_os = "windows")]
#![allow(non_snake_case)]

mod d3d11;
pub mod shared;
mod win32;

use core::ffi::c_void;
use core::ptr;

use shared::{CAPTURE_MAGIC, CAPTURE_VERSION, SHARED_MEM_SIZE, SharedCaptureHeader};
use win32::*;

// ---------------------------------------------------------------------------
// DllMain
// ---------------------------------------------------------------------------

const DLL_PROCESS_ATTACH: u32 = 1;
const DLL_PROCESS_DETACH: u32 = 0;

/// Module handle of this DLL, saved so the watchdog can call
/// `FreeLibraryAndExitThread` for a clean self-unload.
static mut DLL_MODULE: HANDLE = ptr::null_mut();

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(
    module: HANDLE,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    match reason {
        DLL_PROCESS_ATTACH => {
            unsafe { DLL_MODULE = module };
            // Spawn a setup thread — never do heavy work under the loader lock.
            unsafe {
                CreateThread(
                    ptr::null(),
                    0,
                    setup_thread,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                );
            }
            1 // TRUE
        }
        DLL_PROCESS_DETACH => 1,
        _ => 1,
    }
}

// ---------------------------------------------------------------------------
// Setup thread — runs after DllMain returns
// ---------------------------------------------------------------------------

/// Entry point for the setup thread spawned from `DllMain`.
///
/// 1. Opens the shared memory and events created by the Lodestone host.
/// 2. Installs the D3D11 Present hook.
/// 3. Enters a watchdog loop that monitors the shutdown event and parent
///    process liveness.
/// 4. On shutdown: unhooks, cleans up, and unloads the DLL from the game.
unsafe extern "system" fn setup_thread(_param: *mut c_void) -> u32 {
    // Give the game a moment to finish initialising its D3D11 device.
    unsafe { Sleep(1000) };

    // --- Open shared memory and events ------------------------------------
    let pid = unsafe { GetCurrentProcessId() };
    let shm_name = encode_wide(&format!("Local\\LodestoneCapture_{pid}"));
    let ready_name = encode_wide(&format!("Local\\LodestoneCaptureReady_{pid}"));
    let shutdown_name = encode_wide(&format!("Local\\LodestoneCaptureShutdown_{pid}"));

    let shm_handle = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, shm_name.as_ptr()) };
    if shm_handle.is_null() {
        return 1;
    }

    let mapped = unsafe { MapViewOfFile(shm_handle, FILE_MAP_ALL_ACCESS, 0, 0, SHARED_MEM_SIZE) };
    if mapped.is_null() {
        unsafe { CloseHandle(shm_handle) };
        return 1;
    }

    let ready_event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, ready_name.as_ptr()) };
    let shutdown_event = unsafe { OpenEventW(EVENT_ALL_ACCESS, 0, shutdown_name.as_ptr()) };

    if ready_event.is_null() || shutdown_event.is_null() {
        unsafe {
            UnmapViewOfFile(mapped);
            CloseHandle(shm_handle);
            if !ready_event.is_null() {
                CloseHandle(ready_event);
            }
            if !shutdown_event.is_null() {
                CloseHandle(shutdown_event);
            }
        }
        return 1;
    }

    // --- Validate header --------------------------------------------------
    let header = mapped as *mut SharedCaptureHeader;
    let h = unsafe { &*header };
    if h.magic != CAPTURE_MAGIC || h.version != CAPTURE_VERSION {
        unsafe {
            UnmapViewOfFile(mapped);
            CloseHandle(shm_handle);
            CloseHandle(ready_event);
            CloseHandle(shutdown_event);
        }
        return 1;
    }

    // Read the parent PID so we can detect if Lodestone crashes.
    let parent_pid = h.parent_pid;
    let parent_process = if parent_pid != 0 {
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, parent_pid) }
    } else {
        NULL_HANDLE
    };

    // --- Install the D3D11 hook -------------------------------------------
    let hooked = unsafe { d3d11::install_hook(mapped, ready_event) };
    if !hooked {
        // Could not hook — clean up and exit the thread (stay loaded to avoid
        // crashing the game with a premature FreeLibrary).
        unsafe {
            UnmapViewOfFile(mapped);
            CloseHandle(shm_handle);
            CloseHandle(ready_event);
            CloseHandle(shutdown_event);
            if !parent_process.is_null() {
                CloseHandle(parent_process);
            }
        }
        return 1;
    }

    // --- Watchdog loop ----------------------------------------------------
    loop {
        // Check shutdown event (200ms timeout).
        let wait = unsafe { WaitForSingleObject(shutdown_event, 200) };
        if wait == WAIT_OBJECT_0 {
            break; // Host requested shutdown.
        }

        // Check the shutdown flag in shared memory.
        let h = unsafe { &*header };
        if h.shutdown != 0 {
            break;
        }

        // Check if the parent (Lodestone) process is still alive.
        if !parent_process.is_null() {
            let mut exit_code: u32 = 0;
            let ok = unsafe { GetExitCodeProcess(parent_process, &mut exit_code) };
            if ok != 0 && exit_code != STILL_ACTIVE {
                break; // Parent died — self-unload.
            }
        }
    }

    // --- Unhook and clean up ----------------------------------------------
    unsafe { d3d11::uninstall_hook() };

    unsafe {
        UnmapViewOfFile(mapped);
        CloseHandle(shm_handle);
        CloseHandle(ready_event);
        CloseHandle(shutdown_event);
        if !parent_process.is_null() {
            CloseHandle(parent_process);
        }
    }

    // Unload ourselves from the game process.
    let module = unsafe { DLL_MODULE };
    if !module.is_null() {
        unsafe { FreeLibraryAndExitThread(module, 0) };
    }

    0
}

// std::format! requires an allocator — make sure we have one.
extern crate alloc;
use alloc::format;
