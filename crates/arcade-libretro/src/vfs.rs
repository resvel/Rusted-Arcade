use super::*;

fn with_vfs_handle<T>(
    stream: *mut RetroVfsFileHandle,
    f: impl FnOnce(&mut RetroVfsFileHandle) -> T,
) -> Option<T> {
    if stream.is_null() {
        None
    } else {
        Some(f(unsafe { &mut *stream }))
    }
}

pub(super) fn vfs_trace_enabled() -> bool {
    *VFS_TRACE_ENABLED
}

pub(super) fn vfs_seek_compat_enabled() -> bool {
    *VFS_SEEK_COMPAT_ENABLED
}

pub(super) fn vfs_disabled() -> bool {
    *VFS_DISABLED
}

pub(super) fn vfs_trace(message: impl AsRef<str>) {
    if vfs_trace_enabled() {
        eprintln!("[libretro-vfs] {}", message.as_ref());
    }
}

fn vfs_seek_whence_name(whence: i32) -> &'static str {
    match whence {
        RETRO_VFS_SEEK_POSITION_START => "start",
        RETRO_VFS_SEEK_POSITION_CURRENT => "current",
        RETRO_VFS_SEEK_POSITION_END => "end",
        _ => "unknown",
    }
}

fn is_archive_path(path: &CStr) -> bool {
    let path = path.to_string_lossy();
    path.ends_with(".zip") || path.ends_with(".7z")
}

pub(super) unsafe extern "C" fn retro_vfs_get_path(
    stream: *mut RetroVfsFileHandle,
) -> *const c_char {
    with_vfs_handle(stream, |handle| handle.path.as_ptr()).unwrap_or(std::ptr::null())
}

pub(super) unsafe extern "C" fn retro_vfs_open(
    path: *const c_char,
    mode: u32,
    _hints: u32,
) -> *mut RetroVfsFileHandle {
    if path.is_null() {
        return std::ptr::null_mut();
    }
    let path_str = unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .to_string();
    let mut options = OpenOptions::new();
    let wants_write = (mode & RETRO_VFS_FILE_ACCESS_WRITE) != 0;
    let wants_update = (mode & RETRO_VFS_FILE_ACCESS_UPDATE_EXISTING) != 0;
    let wants_read = (mode & RETRO_VFS_FILE_ACCESS_READ) != 0 || !wants_write || wants_update;
    options.read(wants_read);
    if wants_write || wants_update {
        options.write(true);
    }
    if wants_write && !wants_update {
        options.create(true).truncate(true);
    }

    let Ok(file) = options.open(&path_str) else {
        vfs_trace(format!(
            "open path={} mode=0x{mode:x} hints=0x{_hints:x} failed",
            path_str
        ));
        return std::ptr::null_mut();
    };
    let Ok(path) = CString::new(path_str) else {
        return std::ptr::null_mut();
    };
    let id = NEXT_VFS_HANDLE_ID.fetch_add(1, Ordering::Relaxed);
    vfs_trace(format!(
        "open id={id} path={} mode=0x{mode:x} hints=0x{_hints:x} read={} write={} update={}",
        path.to_string_lossy(),
        wants_read,
        wants_write,
        wants_update
    ));
    Box::into_raw(Box::new(RetroVfsFileHandle { id, file, path }))
}

pub(super) unsafe extern "C" fn retro_vfs_close(stream: *mut RetroVfsFileHandle) -> i32 {
    if stream.is_null() {
        return -1;
    }
    unsafe {
        drop(Box::from_raw(stream));
    }
    0
}

pub(super) unsafe extern "C" fn retro_vfs_size(stream: *mut RetroVfsFileHandle) -> i64 {
    with_vfs_handle(stream, |handle| {
        let result = handle
            .file
            .metadata()
            .map(|meta| meta.len() as i64)
            .unwrap_or(-1);
        vfs_trace(format!(
            "size id={} path={} -> {}",
            handle.id,
            handle.path.to_string_lossy(),
            result
        ));
        result
    })
    .unwrap_or(-1)
}

pub(super) unsafe extern "C" fn retro_vfs_tell(stream: *mut RetroVfsFileHandle) -> i64 {
    with_vfs_handle(stream, |handle| {
        handle
            .file
            .stream_position()
            .map(|pos| pos as i64)
            .unwrap_or(-1)
    })
    .unwrap_or(-1)
}

pub(super) unsafe extern "C" fn retro_vfs_seek(
    stream: *mut RetroVfsFileHandle,
    offset: i64,
    whence: i32,
) -> i64 {
    with_vfs_handle(stream, |handle| {
        let before = handle
            .file
            .stream_position()
            .map(|pos| pos as i64)
            .unwrap_or(-1);
        let seek_from = match whence {
            RETRO_VFS_SEEK_POSITION_START => {
                if offset < 0 {
                    if vfs_seek_compat_enabled() && offset == -1 && is_archive_path(&handle.path) {
                        let result = handle
                            .file
                            .seek(SeekFrom::Start(0))
                            .map(|pos| pos as i64)
                            .unwrap_or(-1);
                        vfs_trace(format!(
                            "seek-compat id={} path={} whence={} offset={} before={} -> {}",
                            handle.id,
                            handle.path.to_string_lossy(),
                            vfs_seek_whence_name(whence),
                            offset,
                            before,
                            result
                        ));
                        return result;
                    }
                    vfs_trace(format!(
                        "seek id={} path={} whence={} offset={} before={} -> -1",
                        handle.id,
                        handle.path.to_string_lossy(),
                        vfs_seek_whence_name(whence),
                        offset,
                        before
                    ));
                    return -1;
                }
                SeekFrom::Start(offset as u64)
            }
            RETRO_VFS_SEEK_POSITION_CURRENT => SeekFrom::Current(offset),
            RETRO_VFS_SEEK_POSITION_END => SeekFrom::End(offset),
            _ => {
                vfs_trace(format!(
                    "seek id={} path={} whence={} offset={} before={} -> -1",
                    handle.id,
                    handle.path.to_string_lossy(),
                    vfs_seek_whence_name(whence),
                    offset,
                    before
                ));
                return -1;
            }
        };
        let result = handle
            .file
            .seek(seek_from)
            .map(|pos| pos as i64)
            .unwrap_or(-1);
        vfs_trace(format!(
            "seek id={} path={} whence={} offset={} before={} -> {}",
            handle.id,
            handle.path.to_string_lossy(),
            vfs_seek_whence_name(whence),
            offset,
            before,
            result
        ));
        result
    })
    .unwrap_or(-1)
}

pub(super) unsafe extern "C" fn retro_vfs_read(
    stream: *mut RetroVfsFileHandle,
    buffer: *mut c_void,
    len: u64,
) -> i64 {
    if buffer.is_null() {
        return -1;
    }
    with_vfs_handle(stream, |handle| {
        let before = handle
            .file
            .stream_position()
            .map(|pos| pos as i64)
            .unwrap_or(-1);
        let slice = unsafe { std::slice::from_raw_parts_mut(buffer as *mut u8, len as usize) };
        let result = handle
            .file
            .read(slice)
            .map(|read| read as i64)
            .unwrap_or(-1);
        let after = handle
            .file
            .stream_position()
            .map(|pos| pos as i64)
            .unwrap_or(-1);
        vfs_trace(format!(
            "read id={} path={} len={} before={} -> {} after={}",
            handle.id,
            handle.path.to_string_lossy(),
            len,
            before,
            result,
            after
        ));
        result
    })
    .unwrap_or(-1)
}

pub(super) unsafe extern "C" fn retro_vfs_write(
    stream: *mut RetroVfsFileHandle,
    buffer: *const c_void,
    len: u64,
) -> i64 {
    if buffer.is_null() {
        return -1;
    }
    with_vfs_handle(stream, |handle| {
        let slice = unsafe { std::slice::from_raw_parts(buffer as *const u8, len as usize) };
        handle
            .file
            .write(slice)
            .map(|written| written as i64)
            .unwrap_or(-1)
    })
    .unwrap_or(-1)
}

pub(super) unsafe extern "C" fn retro_vfs_flush(stream: *mut RetroVfsFileHandle) -> i32 {
    with_vfs_handle(stream, |handle| {
        handle.file.flush().map(|_| 0).unwrap_or(-1)
    })
    .unwrap_or(-1)
}

pub(super) unsafe extern "C" fn retro_vfs_remove(path: *const c_char) -> i32 {
    if path.is_null() {
        return -1;
    }
    let path = unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .to_string();
    fs::remove_file(path).map(|_| 0).unwrap_or(-1)
}

pub(super) unsafe extern "C" fn retro_vfs_rename(
    old_path: *const c_char,
    new_path: *const c_char,
) -> i32 {
    if old_path.is_null() || new_path.is_null() {
        return -1;
    }
    let old_path = unsafe { CStr::from_ptr(old_path) }
        .to_string_lossy()
        .to_string();
    let new_path = unsafe { CStr::from_ptr(new_path) }
        .to_string_lossy()
        .to_string();
    fs::rename(old_path, new_path).map(|_| 0).unwrap_or(-1)
}

pub(super) unsafe extern "C" fn retro_vfs_truncate(
    stream: *mut RetroVfsFileHandle,
    length: i64,
) -> i64 {
    if length < 0 {
        return -1;
    }
    with_vfs_handle(stream, |handle| {
        handle.file.set_len(length as u64).map(|_| 0).unwrap_or(-1)
    })
    .unwrap_or(-1)
}
