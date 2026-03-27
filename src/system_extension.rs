//! macOS system extension activation for the Lodestone virtual camera.
//!
//! Submits an activation request for the camera extension on every launch.
//! The request is idempotent — if the extension is already activated, macOS
//! reports success immediately.
//!
//! This module is only compiled on macOS (`#[cfg(target_os = "macos")]`).

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{AllocAnyThread, define_class, msg_send};
use objc2_foundation::{NSError, NSObject as FoundationNSObject, NSObjectProtocol, NSString};

// ---------------------------------------------------------------------------
// FFI: dispatch main queue
// ---------------------------------------------------------------------------
// `dispatch_get_main_queue()` is a C macro that expands to `&_dispatch_main_q`.
// We link the underlying symbol directly.
unsafe extern "C" {
    #[link_name = "_dispatch_main_q"]
    static DISPATCH_MAIN_Q: std::ffi::c_void;
}

/// Returns a pointer to the main dispatch queue.
fn dispatch_get_main_queue() -> *const std::ffi::c_void {
    unsafe { &DISPATCH_MAIN_Q as *const std::ffi::c_void }
}

// ---------------------------------------------------------------------------
// FFI: OSSystemExtensionRequest
// ---------------------------------------------------------------------------

/// Minimal binding for `OSSystemExtensionRequest`.
#[repr(C)]
pub struct OSSystemExtensionRequest {
    __inner: [u8; 0],
}

unsafe impl objc2::Message for OSSystemExtensionRequest {}
unsafe impl objc2::RefEncode for OSSystemExtensionRequest {
    const ENCODING_REF: objc2::Encoding =
        objc2::Encoding::Pointer(&objc2::Encoding::Struct("OSSystemExtensionRequest", &[]));
}

impl OSSystemExtensionRequest {
    fn class() -> &'static AnyClass {
        AnyClass::get(c"OSSystemExtensionRequest")
            .expect("OSSystemExtensionRequest class not found — is SystemExtensions.framework linked?")
    }

    /// `+[OSSystemExtensionRequest activationRequestForExtension:queue:]`
    fn activation_request(
        identifier: &NSString,
        queue: *const std::ffi::c_void,
    ) -> *mut AnyObject {
        let cls = Self::class();
        // Cast dispatch_queue_t to &AnyObject — msg_send! checks type encodings
        // and expects '@' (object) for the queue parameter, not '^v' (void pointer).
        let queue_obj: &AnyObject = unsafe { &*(queue as *const AnyObject) };
        unsafe { msg_send![cls, activationRequestForExtension: identifier, queue: queue_obj] }
    }
}

// ---------------------------------------------------------------------------
// FFI: OSSystemExtensionManager
// ---------------------------------------------------------------------------

/// Minimal binding for `OSSystemExtensionManager`.
#[repr(C)]
pub struct OSSystemExtensionManager {
    __inner: [u8; 0],
}

unsafe impl objc2::Message for OSSystemExtensionManager {}
unsafe impl objc2::RefEncode for OSSystemExtensionManager {
    const ENCODING_REF: objc2::Encoding =
        objc2::Encoding::Pointer(&objc2::Encoding::Struct("OSSystemExtensionManager", &[]));
}

impl OSSystemExtensionManager {
    fn class() -> &'static AnyClass {
        AnyClass::get(c"OSSystemExtensionManager")
            .expect("OSSystemExtensionManager class not found — is SystemExtensions.framework linked?")
    }

    /// `+[OSSystemExtensionManager sharedManager]`
    fn shared_manager() -> *mut AnyObject {
        let cls = Self::class();
        unsafe { msg_send![cls, sharedManager] }
    }
}

// ---------------------------------------------------------------------------
// Delegate: OSSystemExtensionRequestDelegate
// ---------------------------------------------------------------------------

/// Ivars for our delegate — none needed, we just log.
struct ExtensionDelegateIvars;

define_class!(
    #[unsafe(super(FoundationNSObject))]
    #[name = "LodestoneExtensionDelegate"]
    #[ivars = ExtensionDelegateIvars]
    struct ExtensionDelegate;

    unsafe impl NSObjectProtocol for ExtensionDelegate {}

    // ----- delegate methods (raw selectors, no protocol crate available) -----

    /// `request:didFinishWithResult:`
    /// Result enum: 0 = completed, 1 = willCompleteAfterReboot
    impl ExtensionDelegate {
        #[unsafe(method(request:didFinishWithResult:))]
        unsafe fn request_did_finish(&self, _request: *mut AnyObject, result: isize) {
            match result {
                0 => log::info!("Camera extension activated successfully"),
                1 => log::info!("Camera extension will complete activation after reboot"),
                other => log::info!("Camera extension activation finished with result {}", other),
            }
        }

        #[unsafe(method(request:didFailWithError:))]
        unsafe fn request_did_fail(&self, _request: *mut AnyObject, error: &NSError) {
            log::error!("Camera extension activation failed: {}", error);
        }

        #[unsafe(method(requestNeedsUserApproval:))]
        unsafe fn request_needs_user_approval(&self, _request: *mut AnyObject) {
            log::info!(
                "Camera extension needs user approval — \
                 open System Settings > General > Login Items & Extensions"
            );
        }

        #[unsafe(method(request:actionForReplacingExtension:withExtension:))]
        unsafe fn request_action_for_replacing(
            &self,
            _request: *mut AnyObject,
            _existing: *mut AnyObject,
            _replacement: *mut AnyObject,
        ) -> isize {
            log::info!("Replacing existing camera extension with updated version");
            1 // OSSystemExtensionReplacementAction.replace
        }
    }
);

impl ExtensionDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(ExtensionDelegateIvars);
        unsafe { msg_send![super(this), init] }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Submit an activation request for the Lodestone virtual camera extension.
///
/// This is idempotent and safe to call on every launch. If the extension is
/// already activated, macOS will report success immediately. The delegate is
/// leaked intentionally so it stays alive for the asynchronous callbacks.
pub fn activate_camera_extension() {
    let identifier = NSString::from_str("com.lodestone.camera-extension");
    let queue = dispatch_get_main_queue();

    // Create activation request
    let request = OSSystemExtensionRequest::activation_request(&identifier, queue);
    if request.is_null() {
        log::error!("Failed to create OSSystemExtensionRequest");
        return;
    }

    // Create and leak the delegate so it outlives the async callbacks
    let delegate = ExtensionDelegate::new();
    let delegate_ptr: *const ExtensionDelegate = &*delegate;

    // Set the delegate on the request: request.delegate = delegate
    unsafe {
        let _: () = msg_send![request, setDelegate: delegate_ptr];
    }

    // Submit request via shared manager
    let manager = OSSystemExtensionManager::shared_manager();
    if manager.is_null() {
        log::error!("Failed to get OSSystemExtensionManager sharedManager");
        return;
    }

    unsafe {
        let _: () = msg_send![manager, submitRequest: request];
    }

    // Leak the delegate so it stays alive for async callbacks
    std::mem::forget(delegate);

    log::info!("Submitted camera extension activation request");
}
