//! File copies for snapshots. Native APIs can clone data blocks; the caller
//! owns the staging directory and removes it if copying or cancellation fails.
use std::{fs, io, path::Path};

type Progress<'a> = dyn FnMut(u64) -> io::Result<()> + 'a;

pub fn copy_file(source: &Path, target: &Path, progress: &mut Progress<'_>) -> io::Result<u64> {
    progress(0)?;
    let meta = fs::symlink_metadata(source)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Expected a regular backup file",
        ));
    }
    #[cfg(target_os = "macos")]
    if mac::try_clone(source, target)? {
        fs::File::open(target)?.sync_all()?;
        progress(meta.len())?;
        return Ok(meta.len());
    }
    #[cfg(windows)]
    {
        windows::copy(source, target, meta.len(), progress)?;
        return Ok(meta.len());
    }
    #[cfg(not(windows))]
    buffered_copy(source, target, progress)
}

#[cfg(any(not(windows), test))]
fn buffered_copy(source: &Path, target: &Path, progress: &mut Progress<'_>) -> io::Result<u64> {
    use io::{Read, Write};
    let mut input = fs::File::open(source)?;
    let meta = input.metadata()?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut buffer = vec![0; 1024 * 1024];
    let mut bytes = 0;
    loop {
        progress(0)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        bytes += count as u64;
        progress(count as u64)?;
    }
    output.set_permissions(meta.permissions())?;
    output.sync_all()?;
    Ok(bytes)
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac {
    use super::*;
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    pub(super) fn try_clone(source: &Path, target: &Path) -> io::Result<bool> {
        let source = CString::new(source.as_os_str().as_bytes())?;
        let target = CString::new(target.as_os_str().as_bytes())?;
        // CLONE_NOFOLLOW | CLONE_NOOWNERCOPY; source and destination are
        // distinct, NUL-terminated paths alive for this synchronous call.
        let result = unsafe { libc::clonefile(source.as_ptr(), target.as_ptr(), 0x0001 | 0x0002) };
        if result == 0 {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EXDEV | libc::ENOTSUP | libc::ENOSYS) => Ok(false),
            _ => Err(error),
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows {
    use super::*;
    use std::{
        ffi::c_void,
        os::windows::ffi::OsStrExt,
        panic::{catch_unwind, AssertUnwindSafe},
    };
    use windows_sys::Win32::Storage::FileSystem::*;

    struct Context<'a> {
        progress: &'a mut Progress<'a>,
        reported: u64,
        length: u64,
        error: Option<io::Error>,
    }

    unsafe extern "system" fn notify(
        message: *const COPYFILE2_MESSAGE,
        context: *const c_void,
    ) -> COPYFILE2_MESSAGE_ACTION {
        // CopyFile2 calls back synchronously. The context is exclusively
        // borrowed for the call; the message and its tagged union are OS-owned.
        let context = unsafe { &mut *context.cast_mut().cast::<Context<'_>>() };
        let message = unsafe { &*message };
        if context.error.is_some() {
            return COPYFILE2_PROGRESS_CANCEL;
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            let copied = if message.Type == COPYFILE2_CALLBACK_CHUNK_FINISHED {
                unsafe { message.Info.ChunkFinished.uliTotalBytesTransferred }.min(context.length)
            } else {
                context.reported
            };
            let delta = copied.saturating_sub(context.reported);
            (context.progress)(delta)?;
            context.reported += delta;
            Ok(())
        }))
        .unwrap_or_else(|_| Err(io::Error::other("Backup progress callback failed")));
        match result {
            Ok(()) => COPYFILE2_PROGRESS_CONTINUE,
            Err(error) => {
                context.error = Some(error);
                COPYFILE2_PROGRESS_CANCEL
            }
        }
    }

    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        // Canonicalize the existing parent too, so new targets use extended
        // paths and work beyond MAX_PATH, including on UNC shares.
        let absolute = if path.exists() {
            path.canonicalize()?
        } else {
            let parent = path
                .parent()
                .ok_or_else(|| io::Error::other("Missing copy destination parent"))?;
            parent.canonicalize()?.join(
                path.file_name()
                    .ok_or_else(|| io::Error::other("Missing copy filename"))?,
            )
        };
        let mut value: Vec<u16> = absolute.as_os_str().encode_wide().collect();
        if value.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "NUL in backup path",
            ));
        }
        value.push(0);
        Ok(value)
    }

    pub(super) fn copy(
        source: &Path,
        target: &Path,
        length: u64,
        progress: &mut Progress<'_>,
    ) -> io::Result<()> {
        let source = wide(source)?;
        let destination = wide(target)?;
        let mut context = Context {
            progress,
            reported: 0,
            length,
            error: None,
        };
        let parameters = COPYFILE2_EXTENDED_PARAMETERS {
            dwSize: std::mem::size_of::<COPYFILE2_EXTENDED_PARAMETERS>() as u32,
            dwCopyFlags: COPY_FILE_FAIL_IF_EXISTS,
            pfCancel: std::ptr::null_mut(),
            pProgressRoutine: Some(notify),
            pvCallbackContext: (&mut context as *mut Context<'_>).cast(),
        };
        // All pointer targets outlive CopyFile2. Supported ReFS volumes can
        // clone automatically; NTFS and other volumes use native copying.
        let result = unsafe { CopyFile2(source.as_ptr(), destination.as_ptr(), &parameters) };
        if let Some(error) = context.error {
            return Err(error);
        }
        if result < 0 {
            return Err(io::Error::other(format!(
                "Windows copy failed (HRESULT {result:#010x})"
            )));
        }
        // A cloned or empty file need not generate chunk callbacks.
        (context.progress)(length.saturating_sub(context.reported))?;
        let permissions = fs::metadata(target)?.permissions();
        // FlushFileBuffers needs a writable handle. Restore the source's
        // read-only attribute after flushing the newly created backup file.
        if permissions.readonly() {
            let mut writable = permissions.clone();
            writable.set_readonly(false);
            fs::set_permissions(target, writable)?;
        }
        let synced = fs::OpenOptions::new()
            .write(true)
            .open(target)
            .and_then(|file| file.sync_all());
        if permissions.readonly() {
            fs::set_permissions(target, permissions)?;
        }
        synced
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn native_copy_is_independent_and_reports_the_file_size() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.dat");
        let target = dir.path().join("backup.dat");
        fs::write(&source, vec![7; 2 * 1024 * 1024]).unwrap();
        let mut reported = 0;
        let copied = copy_file(&source, &target, &mut |bytes| {
            reported += bytes;
            Ok(())
        })
        .unwrap();
        assert_eq!(copied, reported);
        assert_eq!(copied, 2 * 1024 * 1024);
        // In-place writes exercise copy-on-write, rather than replacing inodes.
        use io::Write;
        fs::OpenOptions::new()
            .write(true)
            .open(&source)
            .unwrap()
            .write_all(b"changed")
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), vec![7; 2 * 1024 * 1024]);
        fs::remove_file(source).unwrap();
        assert_eq!(fs::metadata(target).unwrap().len(), copied);
    }

    #[test]
    fn existing_destination_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("backup");
        fs::write(&source, b"new").unwrap();
        fs::write(&target, b"saved").unwrap();
        assert!(copy_file(&source, &target, &mut |_| Ok(())).is_err());
        assert_eq!(fs::read(target).unwrap(), b"saved");
    }

    #[test]
    fn buffered_fallback_copies_and_stops_between_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("backup");
        fs::write(&source, vec![3; 3 * 1024 * 1024]).unwrap();
        let error = buffered_copy(&source, &target, &mut |bytes| {
            if bytes > 0 {
                Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(fs::metadata(&target).unwrap().len() < fs::metadata(&source).unwrap().len());
        fs::remove_file(&target).unwrap();
        buffered_copy(&source, &target, &mut |_| Ok(())).unwrap();
        assert_eq!(fs::read(source).unwrap(), fs::read(target).unwrap());
    }

    #[test]
    fn native_copy_honours_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("backup");
        fs::write(&source, vec![8; 2 * 1024 * 1024]).unwrap();
        let result = copy_file(&source, &target, &mut |bytes| {
            if bytes > 0 {
                Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"))
            } else {
                Ok(())
            }
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
        assert_eq!(fs::metadata(source).unwrap().len(), 2 * 1024 * 1024);
    }
}
