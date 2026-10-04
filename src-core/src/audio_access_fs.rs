//! Traversée relative à des répertoires ouverts, jamais à un chemin vérifié puis rouvert.
use std::{fs::File, io, io::Read, path::Path};
pub(super) struct Directory(File);
impl Directory {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        let file = platform::root(path)?;
        if !file.metadata()?.is_dir() || platform::reparse(&file)? {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(Self(file))
    }
    pub(super) fn child(&self, name: &str) -> io::Result<Self> {
        let file = self.open_child(name, true)?;
        if !file.metadata()?.is_dir() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(Self(file))
    }
    fn open_child(&self, name: &str, directory: bool) -> io::Result<File> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.bytes().any(|b| matches!(b, b'/' | b'\\' | b':' | 0))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let file = platform::child(&self.0, name, directory)?;
        if platform::reparse(&file)? {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(file)
    }
    /// Fichier persistant du seul écrivain coopératif, sans troncature ni suppression.
    pub(super) fn classification_writer_file(&self) -> io::Result<File> {
        let file = platform::lock_file(&self.0, ".classification-writer-v1.lock")?;
        let metadata = file.metadata()?;
        if platform::reparse(&file)?
            || !metadata.is_file()
            || !platform::single_link(&file)?
            || metadata.len() != 0
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(file)
    }
    pub(super) fn read(&self, name: &str, limit: usize) -> io::Result<Vec<u8>> {
        let file = self.open_child(name, false)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || !platform::single_link(&file)? || metadata.len() > limit as u64 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let maximum = u64::try_from(limit)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(io::ErrorKind::InvalidInput)?;
        let mut bytes = Vec::new();
        (&file).take(maximum).read_to_end(&mut bytes)?;
        if bytes.len() > limit
            || bytes.len() as u64 != metadata.len()
            || file.metadata()?.len() != metadata.len()
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(bytes)
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{
        ffi::CString,
        fs::OpenOptions,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::OpenOptionsExt,
        },
    };
    pub(super) fn root(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
    }
    pub(super) fn child(parent: &File, name: &str, directory: bool) -> io::Result<File> {
        let name = CString::new(name).map_err(|_| io::ErrorKind::InvalidInput)?;
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | libc::O_CLOEXEC
            | if directory { libc::O_DIRECTORY } else { 0 };
        // SAFETY: parent remains open; name is NUL-terminated; no creation flags, no variadic mode.
        let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful openat transfers this fresh descriptor to File exactly once.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    pub(super) fn lock_file(parent: &File, name: &str) -> io::Result<File> {
        let name = CString::new(name).map_err(|_| io::ErrorKind::InvalidInput)?;
        let flags =
            libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
        // SAFETY: parent reste ouvert, nom simple fixe terminé par NUL et mode fourni avec O_CREAT.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                flags,
                0o600 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: le descripteur neuf est transféré exactement une fois.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    pub(super) fn reparse(_file: &File) -> io::Result<bool> {
        Ok(false)
    }
    pub(super) fn single_link(file: &File) -> io::Result<bool> {
        use std::os::unix::fs::MetadataExt;
        Ok(file.metadata()?.nlink() == 1)
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::c_void,
        fs::OpenOptions,
        mem::size_of,
        os::windows::{
            fs::{MetadataExt, OpenOptionsExt},
            io::{AsRawHandle, FromRawHandle},
        },
        ptr,
    };
    type Handle = *mut c_void;
    const FILE_READ_DATA_OR_LIST_DIRECTORY: u32 = 0x1;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const SYNCHRONIZE: u32 = 0x00100000;
    const FILE_SHARE_ALL: u32 = 7;
    const FILE_OPEN: u32 = 1;
    const FILE_OPEN_IF: u32 = 3;
    const FILE_WRITE_DATA: u32 = 0x2;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
    const OBJ_CASE_INSENSITIVE: u32 = 0x40;
    const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x20;
    const FILE_NON_DIRECTORY_FILE: u32 = 0x40;
    const FILE_OPEN_REPARSE_POINT: u32 = 0x00200000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x00200000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *mut u16,
    }
    #[repr(C)]
    struct ObjectAttributes {
        length: u32,
        root_directory: Handle,
        object_name: *mut UnicodeString,
        attributes: u32,
        security_descriptor: *mut c_void,
        security_quality_of_service: *mut c_void,
    }
    #[repr(C)]
    struct IoStatusBlock {
        status_or_pointer: usize,
        information: usize,
    }
    #[link(name = "ntdll")]
    extern "system" {
        fn NtCreateFile(
            handle: *mut Handle,
            desired_access: u32,
            attributes: *mut ObjectAttributes,
            status: *mut IoStatusBlock,
            allocation_size: *mut i64,
            file_attributes: u32,
            share_access: u32,
            disposition: u32,
            options: u32,
            ea_buffer: *mut c_void,
            ea_length: u32,
        ) -> i32;
    }
    pub(super) fn root(path: &Path) -> io::Result<File> {
        // BACKUP_SEMANTICS opens a directory, without requesting backup privileges.
        // OPEN_REPARSE_POINT opens the link itself so metadata can reject it.
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_ALL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }
    pub(super) fn child(parent: &File, name: &str, directory: bool) -> io::Result<File> {
        child_mode(parent, name, directory, false)
    }
    pub(super) fn lock_file(parent: &File, name: &str) -> io::Result<File> {
        child_mode(parent, name, false, true)
    }
    fn child_mode(parent: &File, name: &str, directory: bool, lock: bool) -> io::Result<File> {
        let mut wide: Vec<u16> = name.encode_utf16().collect();
        let length = u16::try_from(
            wide.len()
                .checked_mul(2)
                .ok_or(io::ErrorKind::InvalidInput)?,
        )
        .map_err(|_| io::ErrorKind::InvalidInput)?;
        let mut name = UnicodeString {
            length,
            maximum_length: length,
            buffer: wide.as_mut_ptr(),
        };
        let mut attributes = ObjectAttributes {
            length: size_of::<ObjectAttributes>() as u32,
            root_directory: parent.as_raw_handle(),
            object_name: &mut name,
            attributes: OBJ_CASE_INSENSITIVE,
            security_descriptor: ptr::null_mut(),
            security_quality_of_service: ptr::null_mut(),
        };
        let mut status = IoStatusBlock {
            status_or_pointer: 0,
            information: 0,
        };
        let mut handle: Handle = ptr::null_mut();
        // Nom relatif simple. FILE_OPEN en lecture ; seul le verrou fixe emploie FILE_OPEN_IF sans écrasement.
        // Synchronous I/O + OPEN_REPARSE_POINT; directories checked from the returned handle.
        let options = FILE_SYNCHRONOUS_IO_NONALERT
            | FILE_OPEN_REPARSE_POINT
            | if directory {
                0
            } else {
                FILE_NON_DIRECTORY_FILE
            };
        // SAFETY: ABI structures are repr(C); all pointed buffers and the parent handle live through
        // this synchronous call; allocation/EA are absent. Successful ownership moves into File.
        let result = unsafe {
            NtCreateFile(
                &mut handle,
                FILE_READ_DATA_OR_LIST_DIRECTORY
                    | FILE_READ_ATTRIBUTES
                    | SYNCHRONIZE
                    | if lock { FILE_WRITE_DATA } else { 0 },
                &mut attributes,
                &mut status,
                ptr::null_mut(),
                if lock { FILE_ATTRIBUTE_NORMAL } else { 0 },
                FILE_SHARE_ALL,
                if lock { FILE_OPEN_IF } else { FILE_OPEN },
                options,
                ptr::null_mut(),
                0,
            )
        };
        if result < 0 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if handle.is_null() || handle as isize == -1 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(unsafe { File::from_raw_handle(handle) })
    }
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
    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(handle: Handle, information: *mut FileInformation) -> i32;
    }
    pub(super) fn single_link(file: &File) -> io::Result<bool> {
        let mut information = std::mem::MaybeUninit::<FileInformation>::uninit();
        // SAFETY: the live handle is a regular disk file; the output buffer has the Win32 layout.
        let result =
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { information.assume_init() }.links == 1)
    }
    const _: () = assert!(size_of::<FileInformation>() == 52);
    const _: () = assert!(size_of::<ObjectAttributes>() == 6 * size_of::<usize>());
    const _: () = assert!(size_of::<UnicodeString>() == 2 * size_of::<usize>());
    const _: () = assert!(size_of::<IoStatusBlock>() == 2 * size_of::<usize>());
    pub(super) fn reparse(file: &File) -> io::Result<bool> {
        Ok(file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    }
}
