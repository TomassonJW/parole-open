//! Préférences séparées de travail.json. La racine provient exclusivement du backend.
use parole_core::transcript_presentation::{
    validate_preferences, validate_preferences_for_job, PresentationPreferences, PresentationState,
};
use parole_core::Job;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

const LOCK_NAME: &str = ".presentation.lock";
const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

// Advisory OS lock: all Store writers cooperate, and closing the handle on crash
// releases it. Never remove or replace the lock inode while a Store is active.
fn acquire_lock(file: &File) -> io::Result<()> {
    let deadline = Instant::now() + LOCK_TIMEOUT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

const MAX_BYTES: usize = 256 * 1024;
const MAX_REVISION: u64 = (1 << 53) - 1;
const INVALID: &str =
    "Préférences de présentation illisibles ou version inconnue ; fichier conservé";
const UNSAFE: &str = "Emplacement des préférences inaccessible ou non sûr";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    revision: u64,
    preferences: PresentationPreferences,
}

pub struct Store {
    root: std::path::PathBuf,
    serial: Mutex<()>,
}
impl Store {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            serial: Mutex::new(()),
        }
    }
    pub fn create_job_dir(&self, id: &str) -> Result<(), String> {
        if !canonical_id(id) {
            return Err("Identifiant de travail invalide".into());
        }
        let root = fs_access::Directory::open(&self.root).map_err(|_| UNSAFE)?;
        let jobs = root.make_child("jobs").map_err(|_| UNSAFE)?;
        jobs.new_child(id).map_err(|_| UNSAFE)?;
        Ok(())
    }
    pub fn load_defaults(&self) -> Result<PresentationState, String> {
        let _guard = self.serial.lock().unwrap_or_else(|p| p.into_inner());
        self.load(None)
    }
    pub fn load_job(&self, id: &str) -> Result<PresentationState, String> {
        let _guard = self.serial.lock().unwrap_or_else(|p| p.into_inner());
        self.load(Some(id))
    }
    pub fn read_job(&self, id: &str) -> Result<Job, String> {
        let bytes = self
            .directory(Some(id))?
            .read("travail.json", 64 * 1024 * 1024)
            .map_err(|_| "Travail inaccessible ou non sûr")?
            .ok_or("Travail introuvable")?;
        serde_json::from_slice(&bytes).map_err(|_| "Travail endommagé".into())
    }
    pub fn save_defaults(
        &self,
        mut preferences: PresentationPreferences,
        expected: u64,
    ) -> Result<PresentationState, String> {
        validate_preferences(&preferences)?;
        preferences.speaker_colors.clear();
        let _guard = self.serial.lock().unwrap_or_else(|p| p.into_inner());
        let _lock = self.write_lock()?;
        self.save(None, preferences, expected)
    }
    pub fn save_job(
        &self,
        id: &str,
        job: &Job,
        preferences: PresentationPreferences,
        expected: u64,
    ) -> Result<PresentationState, String> {
        validate_preferences_for_job(&preferences, job)?;
        let _guard = self.serial.lock().unwrap_or_else(|p| p.into_inner());
        let _lock = self.write_lock()?;
        self.save(Some(id), preferences, expected)
    }
    /// À appeler après la création du dossier et avant la première sauvegarde du travail.
    pub fn snapshot_new_job(&self, id: &str) -> Result<PresentationState, String> {
        let _guard = self.serial.lock().unwrap_or_else(|p| p.into_inner());
        let _lock = self.write_lock()?;
        let defaults = self.load(None)?;
        if !defaults.writable {
            return Err("Réglages des futurs travaux endommagés ; création interrompue".into());
        }
        let mut preferences = defaults.preferences;
        preferences.speaker_colors.clear();
        self.save(Some(id), preferences, 0)
    }
    fn write_lock(&self) -> Result<File, String> {
        let root = fs_access::Directory::open(&self.root).map_err(|_| UNSAFE)?;
        root.lock().map_err(|_| UNSAFE.into())
    }
    fn directory(&self, id: Option<&str>) -> Result<fs_access::Directory, String> {
        let root = fs_access::Directory::open(&self.root).map_err(|_| UNSAFE)?;
        match id {
            None => Ok(root),
            Some(id) => {
                if !canonical_id(id) {
                    return Err("Identifiant de travail invalide".into());
                }
                root.child("jobs")
                    .and_then(|jobs| jobs.child(id))
                    .map_err(|_| UNSAFE.into())
            }
        }
    }
    fn name(id: Option<&str>) -> &'static str {
        if id.is_some() {
            "presentation.json"
        } else {
            "transcript-defaults.json"
        }
    }
    fn load(&self, id: Option<&str>) -> Result<PresentationState, String> {
        let dir = self.directory(id)?;
        let bytes = match dir.read(Self::name(id), MAX_BYTES) {
            Ok(value) => value,
            Err(e) if e.kind() == std::io::ErrorKind::OutOfMemory => {
                return Ok(state(
                    PresentationPreferences::default(),
                    0,
                    Some(INVALID.into()),
                    false,
                ));
            }
            Err(_) => return Err(UNSAFE.into()),
        };
        let Some(bytes) = bytes else {
            return Ok(state(PresentationPreferences::default(), 0, None, true));
        };
        let parsed = serde_json::from_slice::<Envelope>(&bytes)
            .ok()
            .filter(|env| {
                env.revision > 0
                    && env.revision <= MAX_REVISION
                    && validate_preferences(&env.preferences).is_ok()
                    && (id.is_some() || env.preferences.speaker_colors.is_empty())
            });
        match parsed {
            Some(env) => {
                if id.is_some() {
                    // Avant la création initiale de travail.json, le snapshot est valide.
                    // Une fois le travail présent, les identifiants de couleur doivent exister.
                    let bytes = dir
                        .read("travail.json", 64 * 1024 * 1024)
                        .map_err(|_| UNSAFE)?;
                    if let Some(bytes) = bytes {
                        let valid = serde_json::from_slice::<Job>(&bytes)
                            .ok()
                            .is_some_and(|job| {
                                validate_preferences_for_job(&env.preferences, &job).is_ok()
                            });
                        if !valid {
                            return Ok(state(
                                PresentationPreferences::default(),
                                0,
                                Some(INVALID.into()),
                                false,
                            ));
                        }
                    }
                }
                Ok(state(env.preferences, env.revision, None, true))
            }
            None => Ok(state(
                PresentationPreferences::default(),
                0,
                Some(INVALID.into()),
                false,
            )),
        }
    }
    fn save(
        &self,
        id: Option<&str>,
        preferences: PresentationPreferences,
        expected: u64,
    ) -> Result<PresentationState, String> {
        if expected > MAX_REVISION {
            return Err("Révision de présentation invalide".into());
        }
        let current = self.load(id)?;
        if !current.writable {
            return Err(INVALID.into());
        }
        if current.revision != expected {
            return Err("Présentation modifiée entre-temps ; rechargez-la".into());
        }
        let revision = expected
            .checked_add(1)
            .filter(|n| *n <= MAX_REVISION)
            .ok_or("Limite de révision atteinte")?;
        let bytes = serde_json::to_vec(&Envelope {
            revision,
            preferences: preferences.clone(),
        })
        .map_err(|_| "Préférences impossibles à encoder")?;
        if bytes.len() > MAX_BYTES {
            return Err("Préférences trop volumineuses".into());
        }
        self.directory(id)?
            .replace(Self::name(id), &bytes)
            .map_err(|_| "Préférences non enregistrées : emplacement inaccessible")?;
        Ok(state(preferences, revision, None, true))
    }
}
fn temporary_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
fn canonical_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
fn state(
    preferences: PresentationPreferences,
    revision: u64,
    warning: Option<String>,
    writable: bool,
) -> PresentationState {
    PresentationState {
        preferences,
        revision,
        warning,
        writable,
    }
}

#[cfg(unix)]
mod fs_access {
    use std::{
        ffi::CString,
        fs::{File, OpenOptions},
        io::{self, Read, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{MetadataExt, OpenOptionsExt},
        },
        path::Path,
    };
    pub struct Directory(File);
    fn name(s: &str) -> io::Result<CString> {
        if s.is_empty()
            || s == "."
            || s == ".."
            || s.bytes().any(|b| matches!(b, b'/' | b'\\' | b':' | 0))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        CString::new(s).map_err(|_| io::ErrorKind::InvalidInput.into())
    }
    impl Directory {
        pub fn lock(&self) -> io::Result<File> {
            let lock_name = name(super::LOCK_NAME)?;
            let fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    lock_name.as_ptr(),
                    libc::O_RDWR
                        | libc::O_CREAT
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK
                        | libc::O_CLOEXEC,
                    // C varargs promote mode_t (u16 on macOS) to an integer.
                    0o600 as libc::c_uint,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let file = unsafe { File::from_raw_fd(fd) };
            let meta = file.metadata()?;
            if !meta.is_file() || meta.nlink() != 1 {
                return Err(io::ErrorKind::InvalidData.into());
            }
            super::acquire_lock(&file)?;
            // Ensure the locked inode still owns the pathname after acquisition.
            let check_fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    lock_name.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            };
            if check_fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let check = unsafe { File::from_raw_fd(check_fd) };
            let current = check.metadata()?;
            if current.dev() != meta.dev() || current.ino() != meta.ino() || current.nlink() != 1 {
                return Err(io::ErrorKind::InvalidData.into());
            }
            Ok(file)
        }
        pub fn open(path: &Path) -> io::Result<Self> {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)?;
            if !file.metadata()?.is_dir() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            Ok(Self(file))
        }
        pub fn new_child(&self, child: &str) -> io::Result<Self> {
            let child_name = name(child)?;
            let rc = unsafe { libc::mkdirat(self.0.as_raw_fd(), child_name.as_ptr(), 0o700) };
            if rc < 0 {
                return Err(io::Error::last_os_error());
            }
            self.child(child)
        }
        pub fn make_child(&self, child: &str) -> io::Result<Self> {
            let child_name = name(child)?;
            let rc = unsafe { libc::mkdirat(self.0.as_raw_fd(), child_name.as_ptr(), 0o700) };
            if rc < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
                return Err(io::Error::last_os_error());
            }
            self.child(child)
        }
        pub fn child(&self, child: &str) -> io::Result<Self> {
            let child = name(child)?;
            // SAFETY: parent descriptor lives through call; NUL-terminated name, no O_CREAT.
            let fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    child.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: fd newly acquired and transferred exactly once.
            Ok(Self(unsafe { File::from_raw_fd(fd) }))
        }
        pub fn read(&self, child: &str, limit: usize) -> io::Result<Option<Vec<u8>>> {
            let child = name(child)?;
            // O_NONBLOCK prevents a FIFO from stalling before fstat rejects it.
            let fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    child.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                let error = io::Error::last_os_error();
                return if error.kind() == io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(error)
                };
            }
            let mut file = unsafe { File::from_raw_fd(fd) };
            let meta = file.metadata()?;
            if !meta.is_file() || meta.nlink() != 1 {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if meta.len() > limit as u64 {
                return Err(io::ErrorKind::OutOfMemory.into());
            }
            let mut bytes = Vec::new();
            (&mut file).take(limit as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 != meta.len() || file.metadata()?.len() != meta.len() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            Ok(Some(bytes))
        }
        pub fn replace(&self, child: &str, bytes: &[u8]) -> io::Result<()> {
            // Caller holds the application mutex. Check the old inode by descriptor before replacement.
            self.read(child, super::MAX_BYTES)?;
            let child = name(child)?;
            let temporary = name(&format!(".presentation-{}.tmp", super::temporary_id()))?;
            let fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    temporary.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    // C varargs promote mode_t (u16 on macOS) to an integer.
                    0o600 as libc::c_uint,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut file = unsafe { File::from_raw_fd(fd) };
            let result = (|| {
                file.write_all(bytes)?;
                file.sync_all()?;
                // Recheck destination. Rename replaces rather than deleting the old file first.
                self.read(
                    child.to_str().map_err(|_| io::ErrorKind::InvalidInput)?,
                    super::MAX_BYTES,
                )?;
                let status = unsafe {
                    libc::renameat(
                        self.0.as_raw_fd(),
                        temporary.as_ptr(),
                        self.0.as_raw_fd(),
                        child.as_ptr(),
                    )
                };
                if status < 0 {
                    return Err(io::Error::last_os_error());
                }
                self.0.sync_all()
            })();
            if result.is_err() {
                unsafe { libc::unlinkat(self.0.as_raw_fd(), temporary.as_ptr(), 0) };
            }
            result
        }
    }
}

#[cfg(windows)]
mod fs_access {
    use std::{
        fs::{self, File, OpenOptions},
        io::{self, Read, Write},
        os::windows::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
            io::AsRawHandle,
        },
        path::{Path, PathBuf},
    };
    #[repr(C)]
    struct ByHandleInfo {
        attributes: u32,
        created: [u32; 2],
        accessed: [u32; 2],
        written: [u32; 2],
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(
            handle: *mut std::ffi::c_void,
            info: *mut ByHandleInfo,
        ) -> i32;
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    fn one_link(file: &File) -> io::Result<bool> {
        let mut info = std::mem::MaybeUninit::<ByHandleInfo>::uninit();
        let result = unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { info.assume_init() }.links == 1)
    }
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x00200000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    pub struct Directory {
        path: PathBuf,
        _file: File, // pin directory handle across operations
    }
    fn plain(meta: &fs::Metadata) -> bool {
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    }
    fn safe_name(s: &str) -> io::Result<()> {
        if s.is_empty()
            || s == "."
            || s == ".."
            || s.bytes().any(|b| matches!(b, b'/' | b'\\' | b':' | 0))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
    impl Directory {
        pub fn lock(&self) -> io::Result<File> {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .share_mode(0x1 | 0x2) // other writers may open; neither can unlink it
                .open(self.path.join(super::LOCK_NAME))?;
            let meta = file.metadata()?;
            if !meta.is_file() || !plain(&meta) || !one_link(&file)? {
                return Err(io::ErrorKind::InvalidData.into());
            }
            super::acquire_lock(&file)?;
            Ok(file)
        }
        pub fn open(path: &Path) -> io::Result<Self> {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .share_mode(0x1 | 0x2) // deny FILE_SHARE_DELETE while the path is used
                .open(path)?;
            let meta = file.metadata()?;
            if !meta.is_dir() || !plain(&meta) {
                return Err(io::ErrorKind::InvalidData.into());
            }
            Ok(Self {
                path: path.to_owned(),
                _file: file,
            })
        }
        pub fn new_child(&self, name: &str) -> io::Result<Self> {
            safe_name(name)?;
            let path = self.path.join(name);
            fs::create_dir(&path)?;
            Self::open(&path)
        }
        pub fn make_child(&self, name: &str) -> io::Result<Self> {
            safe_name(name)?;
            let path = self.path.join(name);
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
            Self::open(&path)
        }
        pub fn child(&self, name: &str) -> io::Result<Self> {
            safe_name(name)?;
            Self::open(&self.path.join(name))
        }
        pub fn read(&self, name: &str, limit: usize) -> io::Result<Option<Vec<u8>>> {
            safe_name(name)?;
            let mut file = match OpenOptions::new()
                .read(true)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.path.join(name))
            {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            };
            let meta = file.metadata()?;
            if !meta.is_file() || !plain(&meta) || !one_link(&file)? {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if meta.len() > limit as u64 {
                return Err(io::ErrorKind::OutOfMemory.into());
            }
            let mut bytes = Vec::new();
            (&mut file).take(limit as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 != meta.len() || file.metadata()?.len() != meta.len() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            Ok(Some(bytes))
        }
        pub fn replace(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
            self.read(name, super::MAX_BYTES)?;
            let tmp = self
                .path
                .join(format!(".presentation-{}.tmp", super::temporary_id()));
            let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
            let result = (|| {
                file.write_all(bytes)?;
                file.sync_all()?; // flush regular writable temporary, before touching destination
                drop(file); // allow Windows rename even with restrictive sharing
                self.read(name, super::MAX_BYTES)?;
                let dest = self.path.join(name);
                let from: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
                let to: Vec<u16> = dest.as_os_str().encode_wide().chain(Some(0)).collect();
                // Same-directory atomic replacement; WRITE_THROUGH requests a flushed move.
                // This does not guarantee survival of a sudden power loss on every filesystem.
                const REPLACE_EXISTING: u32 = 0x1;
                const WRITE_THROUGH: u32 = 0x8;
                if unsafe {
                    MoveFileExW(from.as_ptr(), to.as_ptr(), REPLACE_EXISTING | WRITE_THROUGH)
                } == 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(tmp);
            }
            result
        }
    }
}
