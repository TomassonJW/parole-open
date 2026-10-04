//! Publication privée : contenu synchronisé, renommage, puis barrière de répertoire.
//! Les garanties restent celles du système de fichiers et du périphérique.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub(super) fn atomic_write(destination: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_with_publish(destination, bytes, publish)
}

fn atomic_write_with_publish(
    destination: &Path,
    bytes: &[u8],
    publish: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let name = destination
        .file_name()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    for _ in 0..16 {
        let mut temporary_name = name.to_os_string();
        temporary_name.push(format!(
            ".part-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = parent.join(temporary_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&temporary) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let written = file.write_all(bytes).and_then(|_| file.sync_all());
        drop(file);
        let result = written.and_then(|_| publish(&temporary, destination));
        if result.is_err() {
            // Ne jamais restaurer/effacer la destination : le renommage peut
            // avoir réussi avant l'échec d'une barrière. La tranche reste non confirmée.
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Fichiers temporaires audio indisponibles",
    ))
}

#[cfg(unix)]
fn publish(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)?;
    fs::File::open(destination.parent().ok_or(io::ErrorKind::InvalidInput)?)?.sync_all()
}

#[cfg(windows)]
fn publish(temporary: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    // canonicalize fournit le chemin absolu verbatim, y compris au-delà de MAX_PATH.
    let existing = fs::canonicalize(temporary)?;
    let parent = fs::canonicalize(destination.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    let replacement = parent.join(destination.file_name().ok_or(io::ErrorKind::InvalidInput)?);
    let existing: Vec<u16> = existing.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement: Vec<u16> = replacement
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: deux chemins Windows possédés, terminés par NUL et vivants pendant
    // l'appel synchrone ; signature Kernel32 officielle, aucun pointeur conservé.
    let result = unsafe {
        MoveFileExW(
            existing.as_ptr(),
            replacement.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn publish(_temporary: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Publication audio durable non prise en charge",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{process_chunks, Job, Stage};

    #[test]
    fn publie_les_octets_exacts_sans_temporary_restant() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("audio.wav");
        fs::write(&destination, b"ancienne version").unwrap();
        atomic_write(&destination, b"nouveaux octets exacts").unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"nouveaux octets exacts");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn erreur_avant_publication_preserve_la_destination() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("audio.wav");
        fs::write(&destination, b"ancienne version").unwrap();
        let result = atomic_write_with_publish(&destination, b"nouvelle version", |_, _| {
            Err(io::ErrorKind::PermissionDenied.into())
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"ancienne version");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn erreur_de_barriere_apres_renommage_ne_confirme_pas_le_travail() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("audio.wav");
        let state = directory.path().join("travail.json");
        let mut job = Job::new("fictif.wav".into(), 1_000, 1_000);
        let result = process_chunks(&mut job, &state, |_, _, _| {
            atomic_write_with_publish(
                &destination,
                b"octets deja publies",
                |temporary, final_path| {
                    fs::rename(temporary, final_path)?;
                    Err(io::Error::other(
                        "Défaillance simulée de synchronisation du répertoire",
                    ))
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(Vec::new())
        });
        assert!(result.is_err());
        let saved: Job = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
        assert_eq!(saved.completed_chunks, 0);
        assert_eq!(saved.stage, Stage::Interrupted);
        // La destination n'est pas effacée sous prétexte que la barrière a échoué.
        assert_eq!(fs::read(destination).unwrap(), b"octets deja publies");
        assert!(fs::read_dir(directory.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".part-")));
    }
}
