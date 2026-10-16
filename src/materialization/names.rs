//! Read-only name keys. Unestablished equivalence rejects the whole preflight.
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

#[derive(Clone, Copy)]
pub(super) enum Semantics {
    #[cfg(any(test, not(target_os = "macos")))]
    Bytes,
    #[cfg(any(test, target_os = "macos"))]
    AsciiSensitive,
    #[cfg(any(test, target_os = "macos"))]
    AsciiInsensitive,
}

const UNKNOWN: &str = "Cannot establish destination filesystem name equivalence safely";

pub(super) fn key(path: &Path) -> Result<PathBuf> {
    key_with(path, semantics)
}

#[cfg(target_os = "linux")]
fn semantics(parent: &Path) -> Result<Option<Semantics>> {
    use std::{fs::OpenOptions, os::fd::AsRawFd, os::unix::fs::OpenOptionsExt};

    let directory = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(parent)
    {
        Ok(directory) => directory,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            // No reachable directory namespace here. Retain the checked
            // prefix key; snapshots report non-directory ancestors per file.
            return Ok(None);
        }
        Err(_) => bail!(UNKNOWN),
    };
    let mut filesystem = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // Both queries use the same open directory, including at mount boundaries.
    if unsafe { libc::fstatfs(directory.as_raw_fd(), filesystem.as_mut_ptr()) } != 0 {
        bail!(UNKNOWN);
    }
    let filesystem = unsafe { filesystem.assume_init() };
    // Do not infer semantics from the OS or an underlying/overlay filesystem.
    // This intentionally supports only the ext family, with per-directory
    // casefold checked even when all supplied names happen to be ASCII.
    if filesystem.f_type != libc::EXT4_SUPER_MAGIC {
        bail!(UNKNOWN);
    }
    // The ioctl number encodes long, but the kernel returns an int. Reserve
    // space for either width and read the first native int on every endian.
    let mut flags = [0 as libc::c_int; 2];
    if unsafe {
        libc::ioctl(
            directory.as_raw_fd(),
            libc::FS_IOC_GETFLAGS,
            flags.as_mut_ptr(),
        )
    } != 0
    {
        bail!(UNKNOWN);
    }
    linux_ext_semantics(flags[0]).map(Some)
}

#[cfg(any(test, target_os = "linux"))]
fn linux_ext_semantics(flags: libc::c_int) -> Result<Semantics> {
    const FS_CASEFOLD_FL: libc::c_int = 0x4000_0000;
    if flags & FS_CASEFOLD_FL != 0 {
        bail!(UNKNOWN);
    }
    Ok(Semantics::Bytes)
}

#[cfg(target_os = "macos")]
fn semantics(parent: &Path) -> Result<Option<Semantics>> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    // Unreachable namespaces retain the checked prefix key. Snapshot planning
    // still refuses non-directory ancestors independently, without fetching.
    match std::fs::metadata(parent) {
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(None);
        }
        Ok(metadata) if !metadata.is_dir() => return Ok(None),
        Ok(_) => {}
        _ => bail!(UNKNOWN),
    }
    let literal = CString::new(parent.as_os_str().as_bytes())?;
    match unsafe { libc::pathconf(literal.as_ptr(), libc::_PC_CASE_SENSITIVE) } {
        0 => Ok(Some(Semantics::AsciiInsensitive)),
        1 => Ok(Some(Semantics::AsciiSensitive)),
        _ => bail!(UNKNOWN),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn semantics(_: &Path) -> Result<Option<Semantics>> {
    bail!(UNKNOWN)
}

pub(super) fn key_with(
    path: &Path,
    mut semantics: impl FnMut(&Path) -> Result<Option<Semantics>>,
) -> Result<PathBuf> {
    #[cfg(any(test, target_os = "macos"))]
    use std::os::unix::ffi::OsStrExt;

    let mut original = PathBuf::new();
    let mut key = PathBuf::new();
    let mut established = None;
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            if let Some(value) = semantics(&original)? {
                established = Some(value);
            }
            match established {
                #[cfg(any(test, not(target_os = "macos")))]
                Some(Semantics::Bytes) => key.push(name),
                #[cfg(any(test, target_os = "macos"))]
                Some(Semantics::AsciiSensitive | Semantics::AsciiInsensitive)
                    if !name.as_bytes().is_ascii() =>
                {
                    // Case sensitivity alone does not establish Unicode
                    // normalization equivalence, even on sensitive volumes.
                    bail!(UNKNOWN);
                }
                #[cfg(any(test, target_os = "macos"))]
                Some(Semantics::AsciiSensitive) => key.push(name),
                #[cfg(any(test, target_os = "macos"))]
                Some(Semantics::AsciiInsensitive) => key.push(name.to_ascii_lowercase()),
                None => bail!(UNKNOWN),
            }
        } else {
            key.push(component);
        }
        original.push(component);
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_components_inherit_only_established_semantics() {
        let key = |path: &str, semantics| {
            key_with(Path::new(path), |parent| {
                Ok((parent == Path::new("/")).then_some(semantics))
            })
            .unwrap()
        };
        assert_eq!(
            key("/Missing/Output", Semantics::AsciiInsensitive),
            key("/missing/output", Semantics::AsciiInsensitive)
        );
        assert_ne!(
            key("/Missing/Output", Semantics::Bytes),
            key("/missing/output", Semantics::Bytes)
        );
        assert!(key_with(Path::new("/unknown/output"), |_| Ok(None)).is_err());
    }

    #[test]
    fn mount_points_use_their_own_semantics() {
        let key = key_with(Path::new("/Volume/CaseSensitive/Output"), |parent| {
            Ok(Some(if parent == Path::new("/Volume") {
                Semantics::AsciiInsensitive
            } else {
                Semantics::AsciiSensitive
            }))
        })
        .unwrap();
        assert_eq!(key, Path::new("/Volume/casesensitive/Output"));
    }

    #[test]
    fn unicode_requires_byte_equivalence_not_just_case_sensitivity() {
        for semantics in [Semantics::AsciiSensitive, Semantics::AsciiInsensitive] {
            for name in ["/é", "/e\u{301}", "/K", "/missing/非ASCII"] {
                assert!(key_with(Path::new(name), |_| Ok(Some(semantics))).is_err());
            }
        }
        assert!(key_with(Path::new("/é"), |_| Ok(Some(Semantics::Bytes))).is_ok());
        assert!(linux_ext_semantics(0).is_ok());
        assert!(linux_ext_semantics(0x4000_0000).is_err());
    }
}
