#[path = "../../src-tauri/src/presentation_store.rs"]
mod presentation_store;
use parole_core::{transcript_presentation::PresentationPreferences, Job};
use presentation_store::Store;

#[test]
fn cas_and_legacy_defaults_are_stable() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path());
    let mut global = store.load_defaults().unwrap();
    assert_eq!(global.revision, 0);
    global
        .preferences
        .speaker_colors
        .insert("fictional".into(), "#AaBbCc".into());
    global.preferences.screen.pause_ms = 3500;
    global = store.save_defaults(global.preferences, 0).unwrap();
    assert!(global.preferences.speaker_colors.is_empty());
    assert_eq!(global.revision, 1);
    assert!(store
        .save_defaults(PresentationPreferences::default(), 0)
        .is_err());
    let id = "00000000-0000-4000-8000-000000000001";
    store.create_job_dir(id).unwrap();
    assert!(store.create_job_dir(id).is_err());
    let job = Job::new("invented.wav".into(), 1000, 1000);
    let job_bytes = serde_json::to_vec(&job).unwrap();
    std::fs::write(
        tmp.path().join("jobs").join(id).join("travail.json"),
        &job_bytes,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(store.read_job(id).unwrap()).unwrap(),
        serde_json::to_value(&job).unwrap()
    );
    let old = store.load_job(id).unwrap();
    assert_eq!(old.preferences.screen.pause_ms, 2000);
    let snap = store.snapshot_new_job(id).unwrap();
    assert_eq!(snap.preferences.screen.pause_ms, 3500);
    assert!(snap.preferences.speaker_colors.is_empty());
    assert_eq!(store.load_job(id).unwrap().revision, 1);
    let mut invalid_color = PresentationPreferences::default();
    invalid_color
        .speaker_colors
        .insert("absent".into(), "#AABBCC".into());
    assert!(store.save_job(id, &job, invalid_color, 1).is_err());
    let updated = store
        .save_job(id, &job, PresentationPreferences::default(), 1)
        .unwrap();
    assert_eq!(updated.revision, 2);
    assert!(store
        .save_job(id, &job, PresentationPreferences::default(), 1)
        .is_err());
    assert_eq!(
        std::fs::read(tmp.path().join("jobs").join(id).join("travail.json")).unwrap(),
        job_bytes
    );
}

#[test]
fn corrupt_and_unknown_versions_are_never_overwritten() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path());
    let path = tmp.path().join("transcript-defaults.json");
    for bytes in [
        br#"{"revision":1,"preferences":{"schema_version":99}}"#.as_slice(),
        b"broken",
        br#"{"revision":9007199254740992,"preferences":{}}"#,
        br#"{"revision":1.5,"preferences":{}}"#,
    ] {
        std::fs::write(&path, bytes).unwrap();
        let state = store.load_defaults().unwrap();
        assert!(!state.writable);
        assert!(state.warning.is_some());
        assert!(store
            .save_defaults(PresentationPreferences::default(), 0)
            .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[cfg(unix)]
#[test]
fn irregular_destination_and_parent_are_refused() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path());
    let path = tmp.path().join("transcript-defaults.json");
    let outside = tmp.path().join("outside");
    std::fs::write(&outside, b"untouched").unwrap();
    symlink(&outside, &path).unwrap();
    assert!(store.load_defaults().is_err());
    assert!(store
        .save_defaults(PresentationPreferences::default(), 0)
        .is_err());
    assert_eq!(std::fs::read(&outside).unwrap(), b"untouched");
    std::fs::remove_file(&path).unwrap();
    std::fs::hard_link(&outside, &path).unwrap();
    assert!(store.load_defaults().is_err());
    assert!(store
        .save_defaults(PresentationPreferences::default(), 0)
        .is_err());
    std::fs::remove_file(&path).unwrap();
    let fifo = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert!(store.load_defaults().is_err());
    assert!(store
        .save_defaults(PresentationPreferences::default(), 0)
        .is_err());
    std::fs::remove_file(&path).unwrap();
    symlink(tmp.path(), tmp.path().join("jobs")).unwrap();
    assert!(store
        .load_job("00000000-0000-4000-8000-000000000001")
        .is_err());
    assert!(store
        .create_job_dir("00000000-0000-4000-8000-000000000001")
        .is_err());
}

#[test]
fn persisted_unknown_speaker_is_readonly_and_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path());
    let id = "00000000-0000-4000-8000-000000000001";
    store.create_job_dir(id).unwrap();
    let job = Job::new("invented.wav".into(), 1000, 1000);
    let dir = tmp.path().join("jobs").join(id);
    std::fs::write(dir.join("travail.json"), serde_json::to_vec(&job).unwrap()).unwrap();
    let mut pref = PresentationPreferences::default();
    pref.speaker_colors
        .insert("unseen".into(), "#AABBCC".into());
    let bytes = serde_json::to_vec(&serde_json::json!({"revision":1,"preferences":pref})).unwrap();
    let path = dir.join("presentation.json");
    std::fs::write(&path, &bytes).unwrap();
    let state = store.load_job(id).unwrap();
    assert!(!state.writable);
    assert!(state.warning.is_some());
    assert!(store
        .save_job(id, &job, PresentationPreferences::default(), 1)
        .is_err());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn concurrent_cas_one_winner_and_oversize_is_readonly() {
    let tmp = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(Store::new(tmp.path()));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || {
                store
                    .save_defaults(PresentationPreferences::default(), 0)
                    .is_ok()
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|ok| *ok)
            .count(),
        1
    );
    assert_eq!(store.load_defaults().unwrap().revision, 1);
    let path = tmp.path().join("transcript-defaults.json");
    std::fs::write(&path, vec![b'x'; 256 * 1024 + 1]).unwrap();
    let state = store.load_defaults().unwrap();
    assert!(!state.writable);
    assert!(state.warning.is_some());
    assert!(store
        .save_defaults(PresentationPreferences::default(), 0)
        .is_err());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 256 * 1024 + 1);
}

#[test]
fn independent_stores_cannot_both_win_cas() {
    for _ in 0..32 {
        let tmp = tempfile::tempdir().unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(9));
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let root = tmp.path().to_owned();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let store = Store::new(&root); // independent Mutex per instance
                    let mut pref = PresentationPreferences::default();
                    pref.screen.pause_ms = 2000 + n * 500;
                    parole_core::transcript_presentation::validate_preferences(&pref).unwrap();
                    barrier.wait();
                    store.save_defaults(pref, 0).is_ok()
                })
            })
            .collect();
        barrier.wait();
        let winners = threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|ok| *ok)
            .count();
        assert_eq!(
            winners, 1,
            "multiple independent instances accepted revision 0"
        );
        assert_eq!(Store::new(tmp.path()).load_defaults().unwrap().revision, 1);
    }
}

#[test]
fn cas_across_real_processes() {
    use std::{
        process::Command,
        time::{Duration, Instant},
    };
    if let Ok(root) = std::env::var("PAROLE_CAS_CHILD_ROOT") {
        let id = std::env::var("PAROLE_CAS_CHILD_ID").unwrap();
        let root = std::path::PathBuf::from(root);
        std::fs::write(root.join(format!("ready-{id}")), b"ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("go").exists() {
            assert!(Instant::now() < deadline, "parent never released workers");
            std::thread::sleep(Duration::from_millis(2));
        }
        let mut pref = PresentationPreferences::default();
        pref.screen.pause_ms = 2000 + id.parse::<u32>().unwrap() * 500;
        parole_core::transcript_presentation::validate_preferences(&pref).unwrap();
        let outcome = match Store::new(&root).save_defaults(pref, 0) {
            Ok(state) if state.revision == 1 => "win",
            Err(_) => "reject",
            other => panic!("unexpected CAS outcome: {other:?}"),
        };
        std::fs::write(root.join(format!("done-{id}")), outcome).unwrap();
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut children: Vec<_> = (0..8)
        .map(|id| {
            Command::new(&exe)
                .args(["--exact", "cas_across_real_processes", "--nocapture"])
                .env("PAROLE_CAS_CHILD_ROOT", tmp.path())
                .env("PAROLE_CAS_CHILD_ID", id.to_string())
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    let deadline = Instant::now() + Duration::from_secs(10);
    while (0..8).any(|id| !tmp.path().join(format!("ready-{id}")).exists()) {
        assert!(Instant::now() < deadline, "child startup timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
    std::fs::write(tmp.path().join("go"), b"go").unwrap();
    for child in &mut children {
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "child hung in CAS");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(child.wait().unwrap().success());
    }
    let results: Vec<_> = (0..8)
        .map(|id| std::fs::read_to_string(tmp.path().join(format!("done-{id}"))).unwrap())
        .collect();
    assert_eq!(
        results
            .iter()
            .filter(|value| value.as_str() == "win")
            .count(),
        1,
        "{results:?}"
    );
    assert!(results
        .iter()
        .all(|value| value == "win" || value == "reject"));
    assert_eq!(Store::new(tmp.path()).load_defaults().unwrap().revision, 1);
}

#[cfg(unix)]
#[test]
fn unsafe_lock_links_are_refused_without_mutating_targets() {
    use std::os::unix::fs::symlink;
    for hard in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("outside");
        let lock = tmp.path().join(".presentation.lock");
        std::fs::write(&target, b"untouched").unwrap();
        if hard {
            std::fs::hard_link(&target, &lock).unwrap();
        } else {
            symlink(&target, &lock).unwrap();
        }
        let store = Store::new(tmp.path());
        assert!(store
            .save_defaults(PresentationPreferences::default(), 0)
            .is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
        assert!(!tmp.path().join("transcript-defaults.json").exists());
    }
}

#[test]
fn held_lock_fails_closed_then_releases() {
    let tmp = tempfile::tempdir().unwrap();
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true).write(true).create(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.share_mode(0x1 | 0x2);
    }
    let file = opts.open(tmp.path().join(".presentation.lock")).unwrap();
    file.lock().unwrap();
    let store = Store::new(tmp.path());
    assert!(store
        .save_defaults(PresentationPreferences::default(), 0)
        .is_err());
    assert!(!tmp.path().join("transcript-defaults.json").exists());
    drop(file);
    assert_eq!(
        store
            .save_defaults(PresentationPreferences::default(), 0)
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn incompatible_lock_file_is_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let lock = tmp.path().join(".presentation.lock");
    std::fs::write(&lock, b"sentinel").unwrap();
    let mut permissions = std::fs::metadata(&lock).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&lock, permissions).unwrap();
    let store = Store::new(tmp.path());
    assert!(store
        .save_defaults(PresentationPreferences::default(), 0)
        .is_err());
    assert_eq!(std::fs::read(&lock).unwrap(), b"sentinel");
    assert!(!tmp.path().join("transcript-defaults.json").exists());
}

#[cfg(windows)]
#[test]
fn hardlinked_lock_is_refused_on_windows() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("outside");
    std::fs::write(&target, b"untouched").unwrap();
    std::fs::hard_link(&target, tmp.path().join(".presentation.lock")).unwrap();
    assert!(Store::new(tmp.path())
        .save_defaults(PresentationPreferences::default(), 0)
        .is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
}

#[cfg(unix)]
#[test]
fn pre_rename_write_failure_preserves_old_bytes() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path());
    store
        .save_defaults(PresentationPreferences::default(), 0)
        .unwrap();
    let path = tmp.path().join("transcript-defaults.json");
    let before = std::fs::read(&path).unwrap();
    let original = std::fs::metadata(tmp.path()).unwrap().permissions();
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = store.save_defaults(PresentationPreferences::default(), 1);
    std::fs::set_permissions(tmp.path(), original).unwrap();
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn invalid_revision_and_write_failure_preserve_file() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path());
    let state = store
        .save_defaults(PresentationPreferences::default(), 0)
        .unwrap();
    let path = tmp.path().join("transcript-defaults.json");
    let bytes = std::fs::read(&path).unwrap();
    assert!(store
        .save_defaults(state.preferences.clone(), u64::MAX)
        .is_err());
    assert_eq!(bytes, std::fs::read(&path).unwrap());
    let mut invalid = state.preferences;
    invalid.screen.pause_ms = 7;
    assert!(store.save_defaults(invalid, 1).is_err());
    assert_eq!(bytes, std::fs::read(&path).unwrap());
}
