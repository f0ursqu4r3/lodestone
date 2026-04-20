//! Platform-native disk space queries, used to validate recording targets.
//!
//! Recording silently to a disk that's about to fill up is one of the worst
//! failure modes for a streaming app — the user loses their VOD. We don't
//! depend on a cross-platform crate for this; the project already follows
//! the pattern of raw `extern` declarations for Win32 and Core Foundation.

use std::path::Path;

/// Return the number of bytes available to the calling user on the volume
/// that contains `path`. Resolves `path` to its parent directory if needed
/// (since the file may not exist yet when we're doing a pre-flight check).
///
/// Returns `None` if the path can't be resolved or the syscall fails — the
/// caller should treat this as "unknown; don't block the recording" rather
/// than a hard stop.
pub fn available_bytes(path: &Path) -> Option<u64> {
    let probe = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    platform::available_bytes(&probe)
}

#[cfg(target_os = "windows")]
mod platform {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    #[repr(C)]
    struct UlargeInteger {
        quad_part: u64,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut UlargeInteger,
            lpTotalNumberOfBytes: *mut UlargeInteger,
            lpTotalNumberOfFreeBytes: *mut UlargeInteger,
        ) -> i32;
    }

    pub fn available_bytes(dir: &Path) -> Option<u64> {
        let mut wide: Vec<u16> = dir.as_os_str().encode_wide().collect();
        wide.push(0);
        let mut free_for_caller = UlargeInteger { quad_part: 0 };
        let mut total = UlargeInteger { quad_part: 0 };
        let mut total_free = UlargeInteger { quad_part: 0 };
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_for_caller,
                &mut total,
                &mut total_free,
            )
        };
        if ok == 0 {
            None
        } else {
            Some(free_for_caller.quad_part)
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    // statvfs layout is stable on macOS (darwin). We only touch f_frsize and
    // f_bavail, so field names elsewhere don't matter — but the struct size
    // and field order must match /usr/include/sys/statvfs.h exactly.
    #[repr(C)]
    struct Statvfs {
        f_bsize: u64,
        f_frsize: u64,
        f_blocks: u32,
        f_bfree: u32,
        f_bavail: u32,
        f_files: u32,
        f_ffree: u32,
        f_favail: u32,
        f_fsid: u64,
        f_flag: u64,
        f_namemax: u64,
    }

    unsafe extern "C" {
        fn statvfs(path: *const libc_char, buf: *mut Statvfs) -> i32;
    }

    #[allow(non_camel_case_types)]
    type libc_char = i8;

    pub fn available_bytes(dir: &Path) -> Option<u64> {
        let c_path = CString::new(dir.as_os_str().as_bytes()).ok()?;
        let mut buf = unsafe { std::mem::zeroed::<Statvfs>() };
        let rc = unsafe { statvfs(c_path.as_ptr() as *const libc_char, &mut buf) };
        if rc != 0 {
            return None;
        }
        Some(buf.f_frsize.saturating_mul(buf.f_bavail as u64))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    use std::path::Path;
    pub fn available_bytes(_dir: &Path) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_bytes_returns_some_for_temp_dir() {
        let tmp = std::env::temp_dir();
        let bytes = available_bytes(&tmp);
        // Any mounted filesystem has some free space; if this is zero the
        // test host is in a very bad state and plenty else will break too.
        assert!(bytes.is_some_and(|b| b > 0), "expected > 0 bytes, got {bytes:?}");
    }

    #[test]
    fn available_bytes_resolves_nonexistent_file_via_parent() {
        let mut path = std::env::temp_dir();
        path.push("lodestone_nonexistent_probe_file.mkv");
        assert!(available_bytes(&path).is_some_and(|b| b > 0));
    }
}
