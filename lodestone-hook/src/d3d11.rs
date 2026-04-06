//! D3D11 Present hook.
//!
//! Installs a vtable hook on `IDXGISwapChain::Present` to capture each frame
//! from the game's swap chain into shared memory for the Lodestone host process.

#![allow(non_snake_case, static_mut_refs, dead_code)]

use core::ffi::c_void;
use core::mem;
use core::ptr;

use crate::shared::{
    BUFFER_SIZE, BYTES_PER_PIXEL, FORMAT_BGRA8, MAX_CAPTURE_HEIGHT, MAX_CAPTURE_WIDTH,
    SharedCaptureHeader,
};
use crate::win32::*;

// ---------------------------------------------------------------------------
// COM vtable indices
// ---------------------------------------------------------------------------

/// `IUnknown::Release`
const VTABLE_RELEASE: usize = 2;
/// `IDXGISwapChain::Present`
const VTABLE_PRESENT: usize = 8;
/// `IDXGISwapChain::GetBuffer`
const VTABLE_GET_BUFFER: usize = 9;
/// `ID3D11Device::CreateTexture2D`
const VTABLE_CREATE_TEXTURE2D: usize = 5;
/// `ID3D11Device::GetImmediateContext`
const VTABLE_GET_IMMEDIATE_CONTEXT: usize = 40;
/// `ID3D11Texture2D::GetDesc`
const VTABLE_TEXTURE2D_GET_DESC: usize = 10;
/// `ID3D11DeviceContext::Map`
const VTABLE_CONTEXT_MAP: usize = 14;
/// `ID3D11DeviceContext::Unmap`
const VTABLE_CONTEXT_UNMAP: usize = 15;
/// `ID3D11DeviceContext::CopyResource`
const VTABLE_CONTEXT_COPY_RESOURCE: usize = 47;

// ---------------------------------------------------------------------------
// Hook state — accessed only from the game's render thread.
// ---------------------------------------------------------------------------

struct HookState {
    original_present: Option<PresentFn>,
    /// The vtable slot we patched so we can restore it on unhook.
    vtable_slot: *mut *const c_void,
    device: *mut c_void,
    context: *mut c_void,
    staging_texture: *mut c_void,
    staging_width: u32,
    staging_height: u32,
    shared_header: *mut SharedCaptureHeader,
    shared_base: *mut u8,
    ready_event: HANDLE,
    frame_index: u64,
    initialized: bool,
}

impl HookState {
    const fn new() -> Self {
        Self {
            original_present: None,
            vtable_slot: ptr::null_mut(),
            device: ptr::null_mut(),
            context: ptr::null_mut(),
            staging_texture: ptr::null_mut(),
            staging_width: 0,
            staging_height: 0,
            shared_header: ptr::null_mut(),
            shared_base: ptr::null_mut(),
            ready_event: NULL_HANDLE,
            frame_index: 0,
            initialized: false,
        }
    }
}

// Safety: Present is only ever called from the game's single render thread.
// No concurrent access occurs.
static mut HOOK_STATE: HookState = HookState::new();

// ---------------------------------------------------------------------------
// Dummy window + device creation to discover Present vtable address
// ---------------------------------------------------------------------------

/// Default window procedure used for the hidden dummy window.
unsafe extern "system" fn dummy_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Creates a hidden window, a dummy D3D11 device + swap chain, reads the
/// Present function pointer from the swap chain vtable, then tears everything
/// down.
///
/// Returns `(present_fn_ptr, vtable_slot_ptr)` on success, or `None`.
unsafe fn discover_present_address() -> Option<(*const c_void, *mut *const c_void)> {
    // --- Create a hidden window -------------------------------------------
    let class_name = encode_wide("LodestoneHookDummy");
    let wc = WNDCLASSEXW {
        cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: dummy_wndproc,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: ptr::null_mut(),
        hIcon: ptr::null_mut(),
        hCursor: ptr::null_mut(),
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: ptr::null_mut(),
    };

    unsafe {
        RegisterClassExW(&wc);
    }

    let window_name = encode_wide("LodestoneHookDummyWindow");
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_name.as_ptr(),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            4,
            4,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };

    if hwnd.is_null() {
        return None;
    }

    // --- Load D3D11CreateDeviceAndSwapChain from d3d11.dll ----------------
    let dll_name = encode_wide("d3d11.dll");
    let module = unsafe { GetModuleHandleW(dll_name.as_ptr()) };
    if module.is_null() {
        unsafe { DestroyWindow(hwnd) };
        return None;
    }

    let proc_name = b"D3D11CreateDeviceAndSwapChain\0";
    let proc = unsafe { GetProcAddress(module, proc_name.as_ptr()) };
    if proc.is_null() {
        unsafe { DestroyWindow(hwnd) };
        return None;
    }

    let create_fn: D3D11CreateDeviceAndSwapChainFn = unsafe { mem::transmute(proc) };

    // --- Create dummy device + swap chain ---------------------------------
    let feature_level: u32 = D3D_FEATURE_LEVEL_11_0;

    let sd = DXGI_SWAP_CHAIN_DESC {
        BufferDesc: DXGI_MODE_DESC {
            Width: 4,
            Height: 4,
            RefreshRate: DXGI_RATIONAL {
                Numerator: 60,
                Denominator: 1,
            },
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            ScanlineOrdering: 0,
            Scaling: 0,
        },
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 1,
        OutputWindow: hwnd,
        Windowed: 1,
        SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
        Flags: 0,
    };

    let mut swap_chain: *mut c_void = ptr::null_mut();
    let mut device: *mut c_void = ptr::null_mut();
    let mut returned_feature_level: u32 = 0;
    let mut device_context: *mut c_void = ptr::null_mut();

    let hr = unsafe {
        create_fn(
            ptr::null(),
            D3D_DRIVER_TYPE_HARDWARE,
            ptr::null_mut(),
            0,
            &feature_level,
            1,
            D3D11_SDK_VERSION,
            &sd,
            &mut swap_chain,
            &mut device,
            &mut returned_feature_level,
            &mut device_context,
        )
    };

    if hr < 0 || swap_chain.is_null() {
        if !device_context.is_null() {
            unsafe { com_release(device_context) };
        }
        if !device.is_null() {
            unsafe { com_release(device) };
        }
        unsafe { DestroyWindow(hwnd) };
        return None;
    }

    // --- Read vtable[8] (Present) -----------------------------------------
    let vtable_ptr = unsafe { *(swap_chain as *const *mut *const c_void) };
    let present_slot = unsafe { vtable_ptr.add(VTABLE_PRESENT) };
    let present_fn = unsafe { *present_slot };

    // --- Release dummy objects --------------------------------------------
    unsafe {
        com_release(swap_chain);
        com_release(device_context);
        com_release(device);
        DestroyWindow(hwnd);
    }

    Some((present_fn, present_slot))
}

// ---------------------------------------------------------------------------
// Hooked Present implementation
// ---------------------------------------------------------------------------

/// The replacement `IDXGISwapChain::Present` that captures every frame.
unsafe extern "system" fn hooked_present(
    this: *mut c_void,
    sync_interval: u32,
    flags: u32,
) -> HRESULT {
    unsafe {
        let state = &mut HOOK_STATE;

        // Lazily initialise device / context on first call.
        if !state.initialized {
            if !initialise_from_swap_chain(state, this) {
                // Could not initialise — just forward the call.
                return call_original(state, this, sync_interval, flags);
            }
            state.initialized = true;
        }

        capture_frame(state, this);

        call_original(state, this, sync_interval, flags)
    }
}

/// Forward to the original `Present`.
#[inline]
unsafe fn call_original(
    state: &HookState,
    this: *mut c_void,
    sync_interval: u32,
    flags: u32,
) -> HRESULT {
    if let Some(original) = state.original_present {
        unsafe { original(this, sync_interval, flags) }
    } else {
        0 // S_OK — should never happen
    }
}

/// One-time initialisation: get the device + immediate context from the real
/// swap chain.
unsafe fn initialise_from_swap_chain(state: &mut HookState, swap_chain: *mut c_void) -> bool {
    // IDXGISwapChain inherits from IDXGIDeviceSubObject which has GetDevice,
    // but it's easier to use the fact that the swap chain *also* carries a
    // reference to the D3D11 device via GetBuffer → texture → GetDevice.
    //
    // Simpler approach: call GetBuffer(0) to get a texture, then read its
    // device via ID3D11DeviceChild::GetDevice (vtable index 3).

    let mut backbuffer: *mut c_void = ptr::null_mut();

    // GetBuffer(0, IID_ID3D11Texture2D, &backbuffer)
    type GetBufferFn =
        unsafe extern "system" fn(*mut c_void, u32, *const GUID, *mut *mut c_void) -> HRESULT;
    let get_buffer: GetBufferFn =
        unsafe { mem::transmute(vtable_fn(swap_chain, VTABLE_GET_BUFFER)) };
    let hr = unsafe { get_buffer(swap_chain, 0, &IID_ID3D11Texture2D, &mut backbuffer) };
    if hr < 0 || backbuffer.is_null() {
        return false;
    }

    // ID3D11DeviceChild::GetDevice (vtable index 3)
    type GetDeviceFn = unsafe extern "system" fn(*mut c_void, *mut *mut c_void);
    let get_device: GetDeviceFn = unsafe { mem::transmute(vtable_fn(backbuffer, 3)) };
    let mut device: *mut c_void = ptr::null_mut();
    unsafe { get_device(backbuffer, &mut device) };

    unsafe { com_release(backbuffer) };

    if device.is_null() {
        return false;
    }

    // GetImmediateContext
    type GetImmediateContextFn = unsafe extern "system" fn(*mut c_void, *mut *mut c_void);
    let get_ctx: GetImmediateContextFn =
        unsafe { mem::transmute(vtable_fn(device, VTABLE_GET_IMMEDIATE_CONTEXT)) };
    let mut context: *mut c_void = ptr::null_mut();
    unsafe { get_ctx(device, &mut context) };

    if context.is_null() {
        unsafe { com_release(device) };
        return false;
    }

    state.device = device;
    state.context = context;
    true
}

/// Capture the current back buffer into shared memory.
unsafe fn capture_frame(state: &mut HookState, swap_chain: *mut c_void) {
    if state.shared_header.is_null() || state.shared_base.is_null() {
        return;
    }

    // --- Get back buffer --------------------------------------------------
    let mut backbuffer: *mut c_void = ptr::null_mut();
    type GetBufferFn =
        unsafe extern "system" fn(*mut c_void, u32, *const GUID, *mut *mut c_void) -> HRESULT;
    let get_buffer: GetBufferFn =
        unsafe { mem::transmute(vtable_fn(swap_chain, VTABLE_GET_BUFFER)) };
    let hr = unsafe { get_buffer(swap_chain, 0, &IID_ID3D11Texture2D, &mut backbuffer) };
    if hr < 0 || backbuffer.is_null() {
        return;
    }

    // --- Read texture desc ------------------------------------------------
    let mut desc: D3D11_TEXTURE2D_DESC = unsafe { mem::zeroed() };
    type GetDescFn = unsafe extern "system" fn(*mut c_void, *mut D3D11_TEXTURE2D_DESC);
    let get_desc: GetDescFn =
        unsafe { mem::transmute(vtable_fn(backbuffer, VTABLE_TEXTURE2D_GET_DESC)) };
    unsafe { get_desc(backbuffer, &mut desc) };

    let width = desc.Width;
    let height = desc.Height;

    // Clamp to maximum capture resolution.
    if width == 0 || height == 0 || width > MAX_CAPTURE_WIDTH || height > MAX_CAPTURE_HEIGHT {
        unsafe { com_release(backbuffer) };
        return;
    }

    // --- Ensure staging texture matches current dimensions ----------------
    if state.staging_texture.is_null()
        || state.staging_width != width
        || state.staging_height != height
    {
        // Release old staging texture if any.
        if !state.staging_texture.is_null() {
            unsafe { com_release(state.staging_texture) };
            state.staging_texture = ptr::null_mut();
        }

        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: desc.Format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ,
            MiscFlags: 0,
        };

        type CreateTexture2DFn = unsafe extern "system" fn(
            *mut c_void,
            *const D3D11_TEXTURE2D_DESC,
            *const c_void,
            *mut *mut c_void,
        ) -> HRESULT;
        let create_tex: CreateTexture2DFn =
            unsafe { mem::transmute(vtable_fn(state.device, VTABLE_CREATE_TEXTURE2D)) };

        let mut staging: *mut c_void = ptr::null_mut();
        let hr = unsafe { create_tex(state.device, &staging_desc, ptr::null(), &mut staging) };
        if hr < 0 || staging.is_null() {
            unsafe { com_release(backbuffer) };
            return;
        }

        state.staging_texture = staging;
        state.staging_width = width;
        state.staging_height = height;
    }

    // --- CopyResource: backbuffer → staging -------------------------------
    type CopyResourceFn = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void);
    let copy_resource: CopyResourceFn =
        unsafe { mem::transmute(vtable_fn(state.context, VTABLE_CONTEXT_COPY_RESOURCE)) };
    unsafe { copy_resource(state.context, state.staging_texture, backbuffer) };

    // We're done with the back buffer reference.
    unsafe { com_release(backbuffer) };

    // --- Map staging texture for CPU read ---------------------------------
    let mut mapped: D3D11_MAPPED_SUBRESOURCE = unsafe { mem::zeroed() };
    type MapFn = unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        u32,
        u32,
        *mut D3D11_MAPPED_SUBRESOURCE,
    ) -> HRESULT;
    let map: MapFn = unsafe { mem::transmute(vtable_fn(state.context, VTABLE_CONTEXT_MAP)) };
    let hr = unsafe {
        map(
            state.context,
            state.staging_texture,
            0,
            D3D11_MAP_READ,
            0,
            &mut mapped,
        )
    };
    if hr < 0 {
        return;
    }

    // --- Copy pixels into shared memory -----------------------------------
    let header = unsafe { &mut *state.shared_header };

    // Write to the *inactive* buffer so the host can safely read the other.
    let write_buffer = header.active_buffer ^ 1;
    let buf_offset = SharedCaptureHeader::buffer_offset(write_buffer);
    let dst_base = unsafe { state.shared_base.add(buf_offset) };

    let src_pitch = mapped.RowPitch as usize;
    let dst_pitch = (width as usize) * (BYTES_PER_PIXEL as usize);

    // Ensure we don't write past the buffer boundary.
    if dst_pitch * (height as usize) > BUFFER_SIZE {
        // Frame too large — unmap and bail.
        type UnmapFn = unsafe extern "system" fn(*mut c_void, *mut c_void, u32);
        let unmap: UnmapFn =
            unsafe { mem::transmute(vtable_fn(state.context, VTABLE_CONTEXT_UNMAP)) };
        unsafe { unmap(state.context, state.staging_texture, 0) };
        return;
    }

    // Copy row-by-row to handle pitch differences.
    let copy_bytes_per_row = dst_pitch.min(src_pitch);
    for row in 0..(height as usize) {
        let src_row = unsafe { mapped.pData.add(row * src_pitch) };
        let dst_row = unsafe { dst_base.add(row * dst_pitch) };
        unsafe { ptr::copy_nonoverlapping(src_row, dst_row, copy_bytes_per_row) };
    }

    // --- Unmap ------------------------------------------------------------
    type UnmapFn = unsafe extern "system" fn(*mut c_void, *mut c_void, u32);
    let unmap: UnmapFn = unsafe { mem::transmute(vtable_fn(state.context, VTABLE_CONTEXT_UNMAP)) };
    unsafe { unmap(state.context, state.staging_texture, 0) };

    // --- Update header and signal -----------------------------------------
    header.width = width;
    header.height = height;
    header.pitch = dst_pitch as u32;
    header.format = match desc.Format {
        DXGI_FORMAT_B8G8R8A8_UNORM => FORMAT_BGRA8,
        _ => FORMAT_BGRA8, // TODO: handle more formats or convert
    };

    state.frame_index += 1;
    header.frame_index = state.frame_index;

    // Flip active buffer — the host reads from the old active_buffer.
    header.active_buffer = write_buffer;

    // Signal the host that a new frame is available.
    if state.ready_event != NULL_HANDLE {
        unsafe { SetEvent(state.ready_event) };
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Install the D3D11 Present hook.
///
/// `shared_base` must point to a valid mapped shared memory region that starts
/// with a [`SharedCaptureHeader`].
///
/// `ready_event` is a Win32 event handle signalled after each captured frame.
///
/// Returns `true` if the hook was installed successfully.
///
/// # Safety
///
/// Must be called from a thread where COM is initialised and d3d11.dll is
/// loaded in the process. `shared_base` must remain valid for the lifetime
/// of the hook.
pub unsafe fn install_hook(shared_base: *mut u8, ready_event: HANDLE) -> bool {
    let (present_fn, vtable_slot) = match unsafe { discover_present_address() } {
        Some(pair) => pair,
        None => return false,
    };

    // Note: the vtable_slot we got from the dummy swap chain will be the *same*
    // memory as the real game's swap chain vtable because D3D11 uses a shared
    // vtable for all instances of the same COM class. So patching it once
    // patches every IDXGISwapChain in the process.

    // Make the vtable page writable.
    let mut old_protect: u32 = 0;
    let ok = unsafe {
        VirtualProtect(
            vtable_slot as *mut c_void,
            mem::size_of::<*const c_void>(),
            PAGE_READWRITE,
            &mut old_protect,
        )
    };
    if ok == 0 {
        return false;
    }

    // Write our hook.
    let state = unsafe { &mut HOOK_STATE };
    state.original_present =
        Some(unsafe { mem::transmute::<*const c_void, PresentFn>(present_fn) });
    state.vtable_slot = vtable_slot;
    state.shared_header = shared_base as *mut SharedCaptureHeader;
    state.shared_base = shared_base;
    state.ready_event = ready_event;

    unsafe {
        ptr::write_volatile(vtable_slot, hooked_present as *const c_void);
    }

    // Restore original page protection.
    let mut dummy: u32 = 0;
    unsafe {
        VirtualProtect(
            vtable_slot as *mut c_void,
            mem::size_of::<*const c_void>(),
            old_protect,
            &mut dummy,
        );
    }

    true
}

/// Remove the Present hook and release D3D11 resources.
///
/// # Safety
///
/// Must be called from a context where it is safe to modify the vtable (e.g.
/// the game is no longer rendering or the render thread is synchronised).
pub unsafe fn uninstall_hook() {
    let state = unsafe { &mut HOOK_STATE };

    // Restore original vtable entry.
    if !state.vtable_slot.is_null()
        && let Some(original) = state.original_present
    {
        let mut old_protect: u32 = 0;
        let ok = unsafe {
            VirtualProtect(
                state.vtable_slot as *mut c_void,
                mem::size_of::<*const c_void>(),
                PAGE_READWRITE,
                &mut old_protect,
            )
        };
        if ok != 0 {
            unsafe {
                ptr::write_volatile(state.vtable_slot, original as *const c_void);
            }
            let mut dummy: u32 = 0;
            unsafe {
                VirtualProtect(
                    state.vtable_slot as *mut c_void,
                    mem::size_of::<*const c_void>(),
                    old_protect,
                    &mut dummy,
                );
            }
        }
    }

    // Release COM objects.
    if !state.staging_texture.is_null() {
        unsafe { com_release(state.staging_texture) };
    }
    if !state.context.is_null() {
        unsafe { com_release(state.context) };
    }
    if !state.device.is_null() {
        unsafe { com_release(state.device) };
    }

    // Reset state.
    unsafe {
        HOOK_STATE = HookState::new();
    }
}
