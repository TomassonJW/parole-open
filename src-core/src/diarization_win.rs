//! Chargement de la bibliothèque de diarisation sous Windows 10/11.
use super::*;
use std::os::windows::ffi::OsStrExt;

const LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR: u32 = 0x00000100;
const LOAD_LIBRARY_SEARCH_DEFAULT_DIRS: u32 = 0x00001000;
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryExW(path: *const u16, reserved: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(handle: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}

pub fn open(path: &Path) -> Result<*mut c_void> {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY : chemin UTF-16 terminé par NUL ; chargement des dépendances du même dossier.
    let handle = unsafe {
        LoadLibraryExW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        )
    };
    if handle.is_null() {
        return Err(DiarizationError::Library(format!(
            "bibliothèque locale indisponible (erreur Windows {})",
            unsafe { GetLastError() }
        )));
    }
    Ok(handle)
}
pub fn sym(handle: *mut c_void, name: &str) -> Result<*mut c_void> {
    let symbol =
        CString::new(name).map_err(|_| DiarizationError::Library("symbole invalide".into()))?;
    // SAFETY : poignée créée par LoadLibraryExW, chaîne C valide.
    let ptr = unsafe { GetProcAddress(handle, symbol.as_ptr()) };
    if ptr.is_null() {
        return Err(DiarizationError::Library(format!(
            "symbole absent : {name}"
        )));
    }
    Ok(ptr)
}
pub fn close(handle: *mut c_void) {
    // SAFETY : poignée créée par LoadLibraryExW et libérée une seule fois.
    unsafe {
        FreeLibrary(handle);
    }
}
