use std::path::{Path, PathBuf};

/// Returns `true` when the current process is running under Rosetta 2 translation
/// on Apple Silicon. This indicates an x86_64 binary executing on an arm64 host.
///
/// Always returns `false` on non-macOS platforms or native arm64 execution.
#[cfg(target_os = "macos")]
pub fn is_running_under_rosetta() -> bool {
    use std::ffi::CStr;
    use std::os::raw::{c_int, c_void};

    extern "C" {
        fn sysctlbyname(
            name: *const i8,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *mut c_void,
            newlen: usize,
        ) -> c_int;
    }

    let name = CStr::from_bytes_with_nul(b"sysctl.proc_translated\0")
        .expect("static CStr should be valid");
    let mut value: c_int = 0;
    let mut size: usize = std::mem::size_of::<c_int>();

    let result = unsafe {
        sysctlbyname(
            name.as_ptr(),
            &mut value as *mut c_int as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };

    result == 0 && value == 1
}

#[cfg(not(target_os = "macos"))]
pub fn is_running_under_rosetta() -> bool {
    false
}

/// Returns the architecture subdirectory name for the current process.
///
/// On macOS under Rosetta this returns `"x86_64"`, otherwise `"arm64"` on
/// Apple Silicon. On non-macOS platforms this returns the compile-time target
/// architecture.
pub fn runtime_arch() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        if is_running_under_rosetta() {
            "x86_64"
        } else {
            std::env::consts::ARCH
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::consts::ARCH
    }
}

/// Resolves the effective core root by appending the runtime architecture
/// subdirectory. If the arch-specific directory exists it is preferred;
/// otherwise the base path is returned unchanged for backwards compatibility.
pub fn resolve_arch_core_root(base_core_root: &Path) -> PathBuf {
    let arch_dir = base_core_root.join(runtime_arch());
    if arch_dir.is_dir() {
        arch_dir
    } else {
        base_core_root.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resolve_arch_core_root_returns_arch_dir_when_it_exists() {
        let tmp = tempdir().expect("tempdir");
        let arch = runtime_arch();
        let arch_dir = tmp.path().join(arch);
        fs::create_dir_all(&arch_dir).expect("create arch dir");

        let resolved = resolve_arch_core_root(tmp.path());
        assert_eq!(resolved, arch_dir);
    }

    #[test]
    fn resolve_arch_core_root_falls_back_to_base_when_no_arch_dir() {
        let tmp = tempdir().expect("tempdir");
        let resolved = resolve_arch_core_root(tmp.path());
        assert_eq!(resolved, tmp.path().to_path_buf());
    }

    #[test]
    fn runtime_arch_returns_nonempty_string() {
        assert!(!runtime_arch().is_empty());
    }
}
