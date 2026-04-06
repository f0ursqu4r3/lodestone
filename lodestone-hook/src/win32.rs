//! Raw Win32, DXGI, and D3D11 FFI declarations.
//!
//! Everything here is hand-written — no `windows` or `winapi` crate.
//! Functions from kernel32/user32/ole32 are linked statically.
//! D3D11 symbols are loaded at runtime via `GetProcAddress`.

#![allow(
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    clippy::upper_case_acronyms
)]

use core::ffi::c_void;

// ---------------------------------------------------------------------------
// Handle / pointer types
// ---------------------------------------------------------------------------

/// Opaque Win32 handle.
pub type HANDLE = *mut c_void;

/// Window handle.
pub type HWND = *mut c_void;

/// Instance handle (HMODULE).
pub type HINSTANCE = *mut c_void;

/// COM result code.
pub type HRESULT = i32;

// ---------------------------------------------------------------------------
// Sentinel values
// ---------------------------------------------------------------------------

/// `INVALID_HANDLE_VALUE` (kernel32 convention for failure).
pub const INVALID_HANDLE_VALUE: HANDLE = -1_isize as HANDLE;

/// Null handle.
pub const NULL_HANDLE: HANDLE = core::ptr::null_mut();

// ---------------------------------------------------------------------------
// Process / memory constants
// ---------------------------------------------------------------------------

pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
pub const STILL_ACTIVE: u32 = 259;

pub const PAGE_READWRITE: u32 = 0x04;
pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;

pub const FILE_MAP_ALL_ACCESS: u32 = 0xF001F;

pub const EVENT_ALL_ACCESS: u32 = 0x1F_0003;
pub const EVENT_MODIFY_STATE: u32 = 0x0002;

pub const WAIT_OBJECT_0: u32 = 0;
pub const WAIT_TIMEOUT: u32 = 258;
pub const INFINITE: u32 = 0xFFFF_FFFF;

// ---------------------------------------------------------------------------
// COM
// ---------------------------------------------------------------------------

pub const COINIT_MULTITHREADED: u32 = 0x0;

// ---------------------------------------------------------------------------
// Window styles / creation
// ---------------------------------------------------------------------------

pub const WS_OVERLAPPED: u32 = 0x0000_0000;
pub const CW_USEDEFAULT: i32 = 0x8000_0000_u32 as i32;

// ---------------------------------------------------------------------------
// DXGI constants
// ---------------------------------------------------------------------------

pub const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
pub const DXGI_FORMAT_B8G8R8A8_UNORM: u32 = 87;
pub const DXGI_USAGE_RENDER_TARGET_OUTPUT: u32 = 0x20;
pub const DXGI_SWAP_EFFECT_DISCARD: u32 = 0;

// ---------------------------------------------------------------------------
// D3D11 constants
// ---------------------------------------------------------------------------

pub const D3D11_SDK_VERSION: u32 = 7;
pub const D3D11_USAGE_STAGING: u32 = 3;
pub const D3D11_CPU_ACCESS_READ: u32 = 0x0002_0000;
pub const D3D11_MAP_READ: u32 = 1;
pub const D3D_DRIVER_TYPE_HARDWARE: u32 = 1;
pub const D3D_FEATURE_LEVEL_11_0: u32 = 0xb000;

// ---------------------------------------------------------------------------
// DXGI / D3D11 structures
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DXGI_RATIONAL {
    pub Numerator: u32,
    pub Denominator: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DXGI_MODE_DESC {
    pub Width: u32,
    pub Height: u32,
    pub RefreshRate: DXGI_RATIONAL,
    pub Format: u32,
    pub ScanlineOrdering: u32,
    pub Scaling: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DXGI_SAMPLE_DESC {
    pub Count: u32,
    pub Quality: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DXGI_SWAP_CHAIN_DESC {
    pub BufferDesc: DXGI_MODE_DESC,
    pub SampleDesc: DXGI_SAMPLE_DESC,
    pub BufferUsage: u32,
    pub BufferCount: u32,
    pub OutputWindow: HWND,
    pub Windowed: i32,
    pub SwapEffect: u32,
    pub Flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct D3D11_TEXTURE2D_DESC {
    pub Width: u32,
    pub Height: u32,
    pub MipLevels: u32,
    pub ArraySize: u32,
    pub Format: u32,
    pub SampleDesc: DXGI_SAMPLE_DESC,
    pub Usage: u32,
    pub BindFlags: u32,
    pub CPUAccessFlags: u32,
    pub MiscFlags: u32,
}

#[repr(C)]
pub struct D3D11_MAPPED_SUBRESOURCE {
    pub pData: *mut u8,
    pub RowPitch: u32,
    pub DepthPitch: u32,
}

// ---------------------------------------------------------------------------
// GUID (for COM QueryInterface / GetBuffer)
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

/// `IID_ID3D11Texture2D` — `{6f15aaf2-d208-4e89-9ab4-489535d34f9c}`
pub const IID_ID3D11Texture2D: GUID = GUID {
    data1: 0x6f15aaf2,
    data2: 0xd208,
    data3: 0x4e89,
    data4: [0x9a, 0xb4, 0x48, 0x95, 0x35, 0xd3, 0x4f, 0x9c],
};

// ---------------------------------------------------------------------------
// WNDCLASSEXW (for dummy window creation)
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: unsafe extern "system" fn(HWND, u32, usize, isize) -> isize,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HINSTANCE,
    pub hIcon: HANDLE,
    pub hCursor: HANDLE,
    pub hbrBackground: HANDLE,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: HANDLE,
}

// ---------------------------------------------------------------------------
// D3D11CreateDeviceAndSwapChain — loaded at runtime from d3d11.dll
// ---------------------------------------------------------------------------

pub type D3D11CreateDeviceAndSwapChainFn = unsafe extern "system" fn(
    adapter: *const c_void,
    driver_type: u32,
    software: HANDLE,
    flags: u32,
    feature_levels: *const u32,
    num_feature_levels: u32,
    sdk_version: u32,
    swap_chain_desc: *const DXGI_SWAP_CHAIN_DESC,
    swap_chain: *mut *mut c_void,
    device: *mut *mut c_void,
    feature_level: *mut u32,
    device_context: *mut *mut c_void,
) -> HRESULT;

// ---------------------------------------------------------------------------
// Present function pointer type (IDXGISwapChain::Present)
// ---------------------------------------------------------------------------

/// `HRESULT Present(IDXGISwapChain *this, UINT SyncInterval, UINT Flags)`
pub type PresentFn =
    unsafe extern "system" fn(this: *mut c_void, sync_interval: u32, flags: u32) -> HRESULT;

// ---------------------------------------------------------------------------
// Linked Win32 functions — kernel32
// ---------------------------------------------------------------------------

unsafe extern "system" {
    #[link_name = "GetCurrentProcessId"]
    pub fn GetCurrentProcessId() -> u32;

    #[link_name = "OpenProcess"]
    pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> HANDLE;

    #[link_name = "GetExitCodeProcess"]
    pub fn GetExitCodeProcess(process: HANDLE, exit_code: *mut u32) -> i32;

    #[link_name = "CloseHandle"]
    pub fn CloseHandle(handle: HANDLE) -> i32;

    #[link_name = "CreateFileMappingW"]
    pub fn CreateFileMappingW(
        file: HANDLE,
        security: *const u8,
        protect: u32,
        high: u32,
        low: u32,
        name: *const u16,
    ) -> HANDLE;

    #[link_name = "OpenFileMappingW"]
    pub fn OpenFileMappingW(access: u32, inherit: i32, name: *const u16) -> HANDLE;

    #[link_name = "MapViewOfFile"]
    pub fn MapViewOfFile(
        mapping: HANDLE,
        access: u32,
        high: u32,
        low: u32,
        bytes: usize,
    ) -> *mut u8;

    #[link_name = "UnmapViewOfFile"]
    pub fn UnmapViewOfFile(addr: *const u8) -> i32;

    #[link_name = "CreateEventW"]
    pub fn CreateEventW(
        security: *const u8,
        manual_reset: i32,
        initial: i32,
        name: *const u16,
    ) -> HANDLE;

    #[link_name = "OpenEventW"]
    pub fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> HANDLE;

    #[link_name = "SetEvent"]
    pub fn SetEvent(event: HANDLE) -> i32;

    #[link_name = "WaitForSingleObject"]
    pub fn WaitForSingleObject(handle: HANDLE, millis: u32) -> u32;

    #[link_name = "Sleep"]
    pub fn Sleep(millis: u32);

    #[link_name = "GetModuleHandleW"]
    pub fn GetModuleHandleW(name: *const u16) -> HANDLE;

    #[link_name = "GetProcAddress"]
    pub fn GetProcAddress(module: HANDLE, name: *const u8) -> *const c_void;

    #[link_name = "FreeLibraryAndExitThread"]
    pub fn FreeLibraryAndExitThread(module: HANDLE, exit_code: u32) -> !;

    #[link_name = "CreateThread"]
    pub fn CreateThread(
        security: *const u8,
        stack: usize,
        start: unsafe extern "system" fn(*mut c_void) -> u32,
        param: *mut c_void,
        flags: u32,
        id: *mut u32,
    ) -> HANDLE;

    #[link_name = "VirtualProtect"]
    pub fn VirtualProtect(
        addr: *mut c_void,
        size: usize,
        new_protect: u32,
        old_protect: *mut u32,
    ) -> i32;

    #[link_name = "GetLastError"]
    pub fn GetLastError() -> u32;
}

// ---------------------------------------------------------------------------
// Linked Win32 functions — user32
// ---------------------------------------------------------------------------

#[link(name = "user32")]
unsafe extern "system" {
    pub fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: HWND,
        menu: HANDLE,
        instance: HINSTANCE,
        param: *mut c_void,
    ) -> HWND;

    pub fn DestroyWindow(hwnd: HWND) -> i32;

    pub fn RegisterClassExW(wc: *const WNDCLASSEXW) -> u16;

    pub fn DefWindowProcW(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize;
}

// ---------------------------------------------------------------------------
// Linked Win32 functions — ole32
// ---------------------------------------------------------------------------

#[link(name = "ole32")]
unsafe extern "system" {
    pub fn CoInitializeEx(reserved: *const c_void, co_init: u32) -> HRESULT;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Encode a `&str` as a null-terminated UTF-16 `Vec<u16>` for Win32 W-suffix APIs.
pub fn encode_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0u16)).collect()
}

// ---------------------------------------------------------------------------
// COM helper: read a function pointer from a COM vtable.
// ---------------------------------------------------------------------------

/// Read the function pointer at `index` in a COM object's vtable.
///
/// # Safety
/// `obj` must be a valid COM interface pointer.
#[inline]
pub unsafe fn vtable_fn(obj: *mut c_void, index: usize) -> *const c_void {
    unsafe {
        let vtable = *(obj as *const *const *const c_void);
        *vtable.add(index)
    }
}

/// Call `IUnknown::Release` on a COM object (vtable index 2).
///
/// # Safety
/// `obj` must be a valid COM interface pointer.
#[inline]
pub unsafe fn com_release(obj: *mut c_void) -> u32 {
    unsafe {
        let release: unsafe extern "system" fn(*mut c_void) -> u32 =
            core::mem::transmute(vtable_fn(obj, 2));
        release(obj)
    }
}
