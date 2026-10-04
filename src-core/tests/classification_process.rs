use parole_core::classification_process::{ManagedProcess, StopReason};
use std::{process::Command, time::Duration};

fn fixture(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "fixture_entry", "--nocapture"]);
    command.env("PAROLE_PROCESS_FIXTURE_MODE", mode);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
}

#[test]
fn fixture_entry() {
    let Ok(mode) = std::env::var("PAROLE_PROCESS_FIXTURE_MODE") else {
        return;
    };
    match mode.as_str() {
        "exit" => std::process::exit(0),
        "error" => std::process::exit(19),
        "sleep" => {
            if let Some(path) = std::env::var_os("PAROLE_PROCESS_READY") {
                std::fs::write(path, std::process::id().to_string()).unwrap();
            }
            std::thread::sleep(Duration::from_secs(5));
            if let Some(path) = std::env::var_os("PAROLE_PROCESS_NATURAL_EXIT") {
                std::fs::write(path, b"natural-completion").unwrap();
            }
            std::process::exit(0);
        }
        "environment_parent" => {
            let mut child =
                ManagedProcess::spawn(fixture("check_environment"), Duration::from_secs(3))
                    .unwrap();
            std::process::exit(child.wait().unwrap().status.code().unwrap_or(20));
        }
        "check_environment" => {
            std::process::exit(if std::env::var_os("PAROLE_SYNTHETIC_AMBIENT").is_none() {
                // Valeur distincte du retour normal du harnais sans mode :
                // elle prouve que ce contrôle a réellement été exécuté.
                17
            } else {
                19
            })
        }
        "echo" => {
            use std::io::{Read, Write};
            let mut input = [0u8; 4];
            std::io::stdin().read_exact(&mut input).unwrap();
            assert_eq!(&input, b"ping");
            std::io::stdout().write_all(b"PAROLE_ACK").unwrap();
            std::io::stdout().flush().unwrap();
            std::io::stderr().write_all(b"synthetic-note").unwrap();
            std::io::stderr().flush().unwrap();
            std::process::exit(0);
        }
        _ => panic!("Mode de test inconnu"),
    }
}

#[test]
fn retains_the_real_pid_and_exit_status() {
    for (mode, expected) in [("exit", 0), ("error", 19)] {
        let mut child = ManagedProcess::spawn(fixture(mode), Duration::from_secs(10)).unwrap();
        let pid = child.pid();
        assert!(pid > 0 && pid != std::process::id());
        let outcome = child.wait().unwrap();
        assert_eq!(outcome.pid, pid);
        assert_eq!(outcome.status.code(), Some(expected));
    }
}

#[test]
fn cancellation_stops_and_reaps_the_owned_child() {
    let directory = tempfile::tempdir().unwrap();
    let ready = directory.path().join("ready");
    let mut command = fixture("sleep");
    command.env("PAROLE_PROCESS_READY", &ready);
    let mut child = ManagedProcess::spawn(command, Duration::from_secs(10)).unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(2);
    while !ready.exists() {
        assert!(
            std::time::Instant::now() < until,
            "Le processus de test ne démarre pas"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        std::fs::read_to_string(&ready).unwrap(),
        child.pid().to_string()
    );
    child.cancellation().cancel();
    let outcome = child.wait().unwrap();
    assert_eq!(outcome.stop_requested, Some(StopReason::Cancelled));
    assert!(!outcome.status.success());
    #[cfg(unix)]
    unsafe {
        assert_eq!(
            libc::waitpid(outcome.pid as i32, std::ptr::null_mut(), libc::WNOHANG),
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
}

#[test]
fn deadline_stops_and_reaps_the_owned_child() {
    let mut child = ManagedProcess::spawn(fixture("sleep"), Duration::from_millis(100)).unwrap();
    let outcome = child.wait().unwrap();
    assert_eq!(outcome.stop_requested, Some(StopReason::Deadline));
    assert!(!outcome.status.success());
    assert_eq!(child.poll().unwrap().unwrap().status, outcome.status);
}

#[cfg(unix)]
fn assert_reaped(pid: u32) {
    // Le PID provient exclusivement de l'enfant de ce test. S'il est encore
    // vivant, cette branche de nettoyage le termine et le récolte avant l'échec.
    let result = unsafe { libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) };
    let error = std::io::Error::last_os_error().raw_os_error();
    if result == 0 {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
            libc::waitpid(pid as i32, std::ptr::null_mut(), 0);
        }
    }
    assert_eq!(
        (result, error),
        (-1, Some(libc::ECHILD)),
        "Enfant non récolté"
    );
}

#[test]
#[cfg(unix)]
fn dropping_a_live_owner_kills_and_reaps() {
    let directory = tempfile::tempdir().unwrap();
    let natural_exit = directory.path().join("natural-exit");
    let mut command = fixture("sleep");
    command.env("PAROLE_PROCESS_NATURAL_EXIT", &natural_exit);
    let child = ManagedProcess::spawn(command, Duration::from_secs(10)).unwrap();
    let pid = child.pid();
    drop(child);
    assert_reaped(pid);
    assert!(
        !natural_exit.exists(),
        "Sortie naturelle observée au lieu de l'arrêt demandé"
    );
}

#[test]
#[cfg(unix)]
fn unwinding_the_owner_kills_and_reaps() {
    let directory = tempfile::tempdir().unwrap();
    let natural_exit = directory.path().join("natural-exit");
    let mut command = fixture("sleep");
    command.env("PAROLE_PROCESS_NATURAL_EXIT", &natural_exit);
    let child = ManagedProcess::spawn(command, Duration::from_secs(10)).unwrap();
    let pid = child.pid();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _owner = child;
            panic!("Abandon fictif du calcul");
        }))
        .is_err()
    );
    assert_reaped(pid);
    assert!(
        !natural_exit.exists(),
        "Sortie naturelle observée au lieu de l'arrêt demandé"
    );
}

#[test]
fn zero_budget_is_rejected_before_launch() {
    assert!(
        matches!(ManagedProcess::spawn(fixture("exit"), Duration::ZERO),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput)
    );
}

#[test]
fn relative_program_is_rejected_before_path_lookup() {
    assert!(
        matches!(ManagedProcess::spawn(Command::new("parole-no-path-search"), Duration::from_secs(1)),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput)
    );
}

#[test]
fn ambient_environment_does_not_reach_the_child() {
    let mut command = fixture("environment_parent");
    command.env("PAROLE_SYNTHETIC_AMBIENT", "synthetic-public-test-value");
    let mut parent = ManagedProcess::spawn(command, Duration::from_secs(10)).unwrap();
    assert_eq!(
        parent.wait().unwrap().status.code(),
        Some(17),
        "Contrôle positif de l'environnement non exécuté"
    );
}

#[test]
fn pipes_are_handed_out_once_without_losing_process_ownership() {
    use std::io::{Read, Write};
    use std::process::Stdio;
    let mut command = fixture("echo");
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = ManagedProcess::spawn(command, Duration::from_secs(10)).unwrap();
    let mut pipes = child.take_pipes();
    let again = child.take_pipes();
    assert!(again.stdin.is_none() && again.stdout.is_none() && again.stderr.is_none());
    pipes
        .stdin
        .take()
        .expect("Entrée perdue")
        .write_all(b"ping")
        .unwrap();
    let outcome = child.wait().unwrap();
    assert!(outcome.status.success());
    let mut stdout = String::new();
    let mut stderr = String::new();
    pipes
        .stdout
        .take()
        .unwrap()
        .take(256)
        .read_to_string(&mut stdout)
        .unwrap();
    pipes
        .stderr
        .take()
        .unwrap()
        .take(256)
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(stdout.ends_with("PAROLE_ACK"));
    assert_eq!(stderr, "synthetic-note");
    #[cfg(unix)]
    assert_reaped(outcome.pid);
}

#[test]
fn old_cancellation_cannot_target_a_fresh_run_or_rewrite_a_terminal_result() {
    let mut old = ManagedProcess::spawn(fixture("error"), Duration::from_secs(10)).unwrap();
    let stale = old.cancellation();
    let finished = old.wait().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let ready = directory.path().join("fresh-ready");
    let mut command = fixture("sleep");
    command.env("PAROLE_PROCESS_READY", &ready);
    let mut fresh = ManagedProcess::spawn(command, Duration::from_secs(10)).unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(2);
    while !ready.exists() {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    stale.cancel();
    stale.cancel();
    assert!(fresh.poll().unwrap().is_none());
    let unchanged = old.wait().unwrap();
    assert_eq!(unchanged.status, finished.status);
    assert_eq!(unchanged.stop_requested, None);
    assert_eq!(unchanged.pid, finished.pid);
    fresh.cancellation().cancel();
    let stopped = fresh.wait().unwrap();
    assert_eq!(stopped.stop_requested, Some(StopReason::Cancelled));
    #[cfg(unix)]
    assert_reaped(stopped.pid);
}

#[test]
fn cancellation_from_another_thread_reaches_blocking_wait() {
    let mut child = ManagedProcess::spawn(fixture("sleep"), Duration::from_secs(10)).unwrap();
    let cancellation = child.cancellation();
    let request = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        cancellation.cancel();
    });
    let outcome = child.wait().unwrap();
    request.join().unwrap();
    assert_eq!(outcome.stop_requested, Some(StopReason::Cancelled));
    #[cfg(unix)]
    assert_reaped(outcome.pid);
}

#[test]
fn launch_failure_has_no_running_state_and_allows_a_fresh_launch() {
    let directory = tempfile::tempdir().unwrap();
    let command = Command::new(directory.path().join("missing-program"));
    assert!(
        matches!(ManagedProcess::spawn(command, Duration::from_secs(1)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    );
    let mut child = ManagedProcess::spawn(fixture("exit"), Duration::from_secs(10)).unwrap();
    assert!(child.wait().unwrap().status.success());
}

#[test]
fn unrepresentable_deadline_is_rejected_before_launch() {
    assert!(
        matches!(ManagedProcess::spawn(fixture("exit"), Duration::MAX),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput)
    );
}
