use crate::Artifact;
use std::fs::File;

pub(crate) struct CheckedFile {
    pub(crate) handle: File,
    pub(crate) bytes: Vec<u8>,
}
pub(crate) fn read_verified(artifact: &Artifact, maximum: u64) -> Result<CheckedFile, String> {
    use std::io::Read;
    if !artifact.path.is_absolute()
        || artifact
            .path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Le chemin local est invalide.".into());
    }
    let parent = std::fs::symlink_metadata(
        artifact
            .path
            .parent()
            .ok_or("Le chemin local est invalide.")?,
    )
    .map_err(|_| "Le dossier local est indisponible.")?;
    let meta = std::fs::symlink_metadata(&artifact.path)
        .map_err(|_| "Le fichier local est indisponible.")?;
    if !parent.is_dir()
        || parent.file_type().is_symlink()
        || !meta.is_file()
        || meta.file_type().is_symlink()
    {
        return Err("Les liens et fichiers spéciaux ne sont pas acceptés.".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if (parent.file_attributes() | meta.file_attributes()) & 0x400 != 0 {
            return Err("Les liens Windows ne sont pas acceptés.".into());
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x00200000);
    }
    let mut handle = options
        .open(&artifact.path)
        .map_err(|_| "Le fichier local est indisponible.")?;
    let meta = handle
        .metadata()
        .map_err(|_| "Le fichier local est indisponible.")?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > maximum {
        return Err("Le fichier local est vide, spécial ou trop volumineux.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err("Les liens multiples ne sont pas acceptés.".into());
        }
    }
    #[cfg(windows)]
    {
        if !single_windows_file(&handle)? {
            return Err("Les liens Windows ne sont pas acceptés.".into());
        }
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(
            usize::try_from(meta.len()).map_err(|_| "Le fichier local est trop volumineux.")?,
        )
        .map_err(|_| "La mémoire disponible est insuffisante.")?;
    handle
        .by_ref()
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Le fichier local ne peut pas être lu.")?;
    if bytes.len() as u64 > maximum {
        return Err("Le fichier local dépasse la taille permise.".into());
    }
    use sha2::{Digest, Sha256};
    if format!("{:x}", Sha256::digest(&bytes)) != artifact.sha256 {
        return Err("L'empreinte du fichier local ne correspond pas.".into());
    }
    Ok(CheckedFile { handle, bytes })
}

pub(crate) fn current_executable_sha256() -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    #[cfg(target_os = "linux")]
    let path = std::path::PathBuf::from("/proc/self/exe");
    #[cfg(not(target_os = "linux"))]
    let path =
        std::env::current_exe().map_err(|_| "L'exécutable courant ne peut pas être identifié.")?;
    let mut file = File::open(path).map_err(|_| "L'exécutable courant ne peut pas être lu.")?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65_536];
    let mut total = 0u64;
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|_| "L'exécutable courant ne peut pas être lu.")?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > 512 * 1024 * 1024 {
            return Err("L'exécutable courant est trop volumineux.".into());
        }
        hash.update(&buffer[..n]);
    }
    if total == 0 {
        return Err("L'exécutable courant est vide.".into());
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn loaded_library_matches(
    expected: &File,
    address: *const std::ffi::c_void,
) -> Result<bool, String> {
    #[cfg(unix)]
    {
        use std::os::{unix::ffi::OsStrExt, unix::fs::MetadataExt};
        let mut info = std::mem::MaybeUninit::<libc::Dl_info>::uninit();
        // SAFETY: address is a function from ORT's live API table; dladdr writes the complete Dl_info on success.
        if unsafe { libc::dladdr(address, info.as_mut_ptr()) } == 0 {
            return Err("La bibliothèque active ne peut pas être identifiée.".into());
        }
        let info = unsafe { info.assume_init() };
        if info.dli_fname.is_null() {
            return Ok(false);
        }
        // SAFETY: dladdr returned a non-null, NUL-terminated path owned by the loader.
        let path = std::ffi::OsStr::from_bytes(
            unsafe { std::ffi::CStr::from_ptr(info.dli_fname) }.to_bytes(),
        );
        let actual = std::fs::metadata(path)
            .map_err(|_| "La bibliothèque active ne peut pas être vérifiée.")?;
        let expected = expected
            .metadata()
            .map_err(|_| "La bibliothèque vérifiée est indisponible.")?;
        Ok(actual.dev() == expected.dev() && actual.ino() == expected.ino())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetModuleHandleExW(
                flags: u32,
                address: *const u16,
                module: *mut *mut std::ffi::c_void,
            ) -> i32;
            fn GetModuleFileNameW(
                module: *mut std::ffi::c_void,
                buffer: *mut u16,
                size: u32,
            ) -> u32;
        }
        let mut module = std::ptr::null_mut();
        // SAFETY: FROM_ADDRESS treats the live function pointer as an address, not a UTF-16 string; no reference count is acquired.
        if unsafe { GetModuleHandleExW(0x4 | 0x2, address.cast(), &mut module) } == 0 {
            return Err("La bibliothèque Windows active ne peut pas être identifiée.".into());
        }
        let mut name = vec![0u16; 32_768];
        // SAFETY: module is live and the mutable buffer has the stated length.
        let n =
            unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) } as usize;
        if n == 0 || n >= name.len() {
            return Err("Le chemin de la bibliothèque Windows active est invalide.".into());
        }
        let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&name[..n]));
        let actual =
            File::open(path).map_err(|_| "La bibliothèque Windows active est indisponible.")?;
        Ok(windows_information(expected)?.1 == windows_information(&actual)?.1)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (expected, address);
        Err("Ce système n'est pas pris en charge.".into())
    }
}

#[cfg(windows)]
fn single_windows_file(file: &File) -> Result<bool, String> {
    Ok(windows_information(file)?.0)
}
#[cfg(windows)]
fn windows_information(file: &File) -> Result<(bool, [u32; 3]), String> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    struct FileInformation {
        attributes: u32,
        creation: [u32; 2],
        access: [u32; 2],
        write: [u32; 2],
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    const _: () = assert!(std::mem::size_of::<FileInformation>() == 52);
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            handle: *mut std::ffi::c_void,
            information: *mut FileInformation,
        ) -> i32;
    }
    let mut information = std::mem::MaybeUninit::<FileInformation>::uninit();
    // SAFETY: the file handle is live and the output buffer has the Win32 layout.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) } == 0 {
        return Err("Les propriétés du fichier Windows ne peuvent pas être vérifiées.".into());
    }
    // SAFETY: the successful Win32 call fully initialized the structure.
    let information = unsafe { information.assume_init() };
    Ok((
        information.links == 1 && information.attributes & 0x400 == 0,
        [
            information.volume,
            information.index_high,
            information.index_low,
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{Read, Seek, SeekFrom},
    };
    #[cfg(unix)]
    #[test]
    fn file_and_parent_links_are_rejected_before_reading() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("real");
        fs::create_dir(&parent).unwrap();
        let path = parent.join("fixture.bin");
        fs::write(&path, b"x").unwrap();
        let hash = parole_core::language::sha256_hex(b"x");
        let alias = temp.path().join("alias.bin");
        symlink(&path, &alias).unwrap();
        assert!(
            read_verified(
                &Artifact {
                    path: alias,
                    sha256: hash.clone()
                },
                64
            )
            .is_err()
        );
        let parent_alias = temp.path().join("parent-alias");
        symlink(&parent, &parent_alias).unwrap();
        assert!(
            read_verified(
                &Artifact {
                    path: parent_alias.join("fixture.bin"),
                    sha256: hash.clone()
                },
                64
            )
            .is_err()
        );
        let hard = temp.path().join("hard.bin");
        fs::hard_link(&path, &hard).unwrap();
        assert!(
            read_verified(
                &Artifact {
                    path: hard,
                    sha256: hash
                },
                64
            )
            .is_err()
        );
    }
    #[test]
    fn wrong_digests_empty_files_and_size_limits_are_errors() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bytes");
        fs::write(&path, b"abcd").unwrap();
        let mut spec = Artifact {
            path: path.clone(),
            sha256: "f".repeat(64),
        };
        assert!(read_verified(&spec, 4).is_err());
        spec.sha256 = parole_core::language::sha256_hex(b"abcd");
        assert!(read_verified(&spec, 3).is_err());
        assert_eq!(read_verified(&spec, 4).unwrap().bytes, b"abcd");
        fs::write(&path, []).unwrap();
        spec.sha256 = parole_core::language::sha256_hex(b"");
        assert!(read_verified(&spec, 4).is_err());
        let dir = Artifact {
            path: temp.path().into(),
            sha256: spec.sha256,
        };
        assert!(read_verified(&dir, 64).is_err());
    }
    #[test]
    fn runtime_origin_checks_the_live_code_not_a_filename_claim() {
        let real = File::open(std::env::current_exe().unwrap()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let other = temp.path().join("not-the-executable");
        fs::write(&other, b"fake").unwrap();
        let address =
            runtime_origin_checks_the_live_code_not_a_filename_claim as *const std::ffi::c_void;
        assert!(loaded_library_matches(&real, address).unwrap());
        assert!(!loaded_library_matches(&File::open(other).unwrap(), address).unwrap());
        let fingerprint = current_executable_sha256().unwrap();
        assert_eq!(fingerprint.len(), 64);
    }
    #[test]
    fn returns_the_exact_verified_bytes_and_their_open_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("fixture.bin");
        let bytes = b"octets fictifs exacts";
        fs::write(&path, bytes).unwrap();
        let mut checked = read_verified(
            &Artifact {
                path,
                sha256: parole_core::language::sha256_hex(bytes),
            },
            64,
        )
        .unwrap();
        assert_eq!(checked.bytes, bytes);
        checked.handle.seek(SeekFrom::Start(0)).unwrap();
        let mut reread = Vec::new();
        checked.handle.read_to_end(&mut reread).unwrap();
        assert_eq!(reread, bytes);
    }
}
