use parole_core::{
    topic_classification_access::{
        ClassificationRequest, ClassificationStart, ClassificationWriter,
    },
    topic_classification_cache::{ClassificationResponse, EngineIdentity},
    topic_questions::{CandidateChoice, PreparedTopicQuestions},
    topic_selection::{CandidateScore, SelectionThresholds},
    Job, Segment,
};
use std::{fs, path::Path};
const ID: &str = "34343434-3434-4434-8434-343434343434";
const LOCK: &str = ".classification-writer-v1.lock";
fn job() -> Job {
    let mut job = Job::new("fictif.wav".into(), 1000, 1000);
    job.segments = vec![Segment::new(
        0,
        1000,
        "Le budget et le calendrier du projet Atlas restent à discuter.".into(),
    )];
    job
}
fn install(root: &Path) -> Job {
    let job = job();
    fs::create_dir(root.join(ID)).unwrap();
    fs::write(
        root.join(ID).join("travail.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    job
}
fn request(job: &Job) -> ClassificationRequest {
    ClassificationRequest {
        job_id: ID.into(),
        source_revision: PreparedTopicQuestions::prepare(ID, &job.segments)
            .unwrap()
            .question(0, &[])
            .unwrap()
            .source_revision,
        target: 0,
        choices: vec![CandidateChoice::Word {
            term: "budget".into(),
        }],
        engine: EngineIdentity {
            model_id: "fixture/writer".into(),
            model_revision: "v1".into(),
            weights_sha256: "1".repeat(64),
            tokenizer_sha256: "2".repeat(64),
            runtime_sha256: "3".repeat(64),
            engine_sha256: "4".repeat(64),
            options_sha256: "5".repeat(64),
        },
        thresholds: SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: 0.9,
        },
    }
}
// Scores fictifs uniquement : ces tests ne prétendent pas exécuter un modèle.
fn response(
    work: &parole_core::topic_classification_access::PreparedClassification,
) -> ClassificationResponse {
    ClassificationResponse {
        question_revision: work.plan().question().request_revision.clone(),
        engine_revision: work.plan().engine_revision().into(),
        scores: work
            .plan()
            .question()
            .candidates
            .iter()
            .map(|c| CandidateScore {
                candidate_id: c.candidate_id.clone(),
                score: 0.7,
            })
            .collect(),
    }
}
#[test]
fn one_writer_blocks_another_but_not_reading_and_drop_releases_it() {
    use parole_core::topic_classification_access::ClassificationStartError;
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let req = request(&job);
    let first = ClassificationWriter::open(root.path()).unwrap();
    let other = ClassificationWriter::open(root.path()).unwrap();
    let ClassificationStart::Compute(run) = first.try_begin(&req).unwrap() else {
        panic!("calcul attendu")
    };
    assert!(matches!(
        other.try_begin(&req),
        Err(ClassificationStartError::Busy)
    ));
    assert!(matches!(
        first.try_begin(&req),
        Err(ClassificationStartError::Busy)
    ));
    assert!(other.load(&req).unwrap().record.is_none());
    drop(run);
    assert!(matches!(
        other.try_begin(&req),
        Ok(ClassificationStart::Compute(_))
    ));
    assert!(root.path().join(LOCK).is_file());
    assert_eq!(fs::metadata(root.path().join(LOCK)).unwrap().len(), 0);
}
#[test]
fn a_cached_result_stays_available_while_another_plan_owns_the_writer() {
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let req = request(&job);
    let writer = ClassificationWriter::open(root.path()).unwrap();
    let ClassificationStart::Compute(run) = writer.try_begin(&req).unwrap() else {
        panic!("calcul")
    };
    let data = response(run.prepared());
    run.save(&data).unwrap();
    let mut next = request(&job);
    next.engine.model_revision = "v2".into();
    let ClassificationStart::Compute(_held) = writer.try_begin(&next).unwrap() else {
        panic!("calcul")
    };
    assert!(matches!(
        writer.try_begin(&req),
        Ok(ClassificationStart::Cached(_))
    ));
    assert!(writer.load(&req).unwrap().record.is_some());
}
#[test]
fn a_failed_save_releases_the_writer_without_storing_invalid_scores() {
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let req = request(&job);
    let writer = ClassificationWriter::open(root.path()).unwrap();
    let ClassificationStart::Compute(run) = writer.try_begin(&req).unwrap() else {
        panic!("calcul")
    };
    let mut data = response(run.prepared());
    data.scores[0].score = f64::NAN;
    assert!(run.save(&data).is_err());
    assert!(writer.load(&req).unwrap().record.is_none());
    assert!(matches!(
        writer.try_begin(&req),
        Ok(ClassificationStart::Compute(_))
    ));
}
#[test]
fn a_source_edited_after_preparation_is_not_published_and_releases_the_writer() {
    let root = tempfile::tempdir().unwrap();
    let mut job = install(root.path());
    let req = request(&job);
    let writer = ClassificationWriter::open(root.path()).unwrap();
    let ClassificationStart::Compute(run) = writer.try_begin(&req).unwrap() else {
        panic!("calcul")
    };
    let data = response(run.prepared());
    job.segments[0].text.push('!');
    fs::write(
        root.path().join(ID).join("travail.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    assert!(run.save(&data).is_err());
    let next = request(&job);
    assert!(writer.load(&next).unwrap().record.is_none());
    assert!(matches!(
        writer.try_begin(&next),
        Ok(ClassificationStart::Compute(_))
    ));
}
#[test]
fn a_nonempty_lock_file_is_never_truncated_or_treated_as_a_live_writer() {
    use parole_core::topic_classification_access::ClassificationStartError;
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let req = request(&job);
    let path = root.path().join(LOCK);
    fs::write(&path, b"temoin conserve").unwrap();
    let writer = ClassificationWriter::open(root.path()).unwrap();
    assert!(matches!(
        writer.try_begin(&req),
        Err(ClassificationStartError::Unavailable(_))
    ));
    assert_eq!(fs::read(path).unwrap(), b"temoin conserve");
    assert!(writer.load(&req).unwrap().record.is_none());
}
#[test]
fn non_absolute_or_parent_traversal_roots_are_refused() {
    let root = tempfile::tempdir().unwrap();
    assert!(ClassificationWriter::open(Path::new(".")).is_err());
    assert!(ClassificationWriter::open(&root.path().join("..")).is_err());
}
#[test]
fn distinct_libraries_have_distinct_writers() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let ja = install(a.path());
    let jb = install(b.path());
    let wa = ClassificationWriter::open(a.path()).unwrap();
    let wb = ClassificationWriter::open(b.path()).unwrap();
    let _a = wa.try_begin(&request(&ja)).unwrap();
    let _b = wb.try_begin(&request(&jb)).unwrap();
}
#[test]
fn a_directory_in_place_of_the_lock_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    fs::create_dir(root.path().join(LOCK)).unwrap();
    assert!(ClassificationWriter::open(root.path())
        .unwrap()
        .try_begin(&request(&job))
        .is_err());
}
#[test]
fn a_hard_link_to_an_empty_external_file_is_refused_without_changes() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let target = outside.path().join("temoin");
    fs::write(&target, []).unwrap();
    fs::hard_link(&target, root.path().join(LOCK)).unwrap();
    assert!(ClassificationWriter::open(root.path())
        .unwrap()
        .try_begin(&request(&job))
        .is_err());
    assert!(fs::read(target).unwrap().is_empty());
}
#[cfg(unix)]
#[test]
fn symbolic_lock_and_root_links_are_refused_and_parent_aliases_share_the_lock() {
    use parole_core::topic_classification_access::ClassificationStartError;
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let target = root.path().join("temoin");
    fs::write(&target, []).unwrap();
    symlink(&target, root.path().join(LOCK)).unwrap();
    assert!(ClassificationWriter::open(root.path())
        .unwrap()
        .try_begin(&request(&job))
        .is_err());
    assert!(fs::read(&target).unwrap().is_empty());
    let parent = tempfile::tempdir().unwrap();
    let data = parent.path().join("data");
    fs::create_dir(&data).unwrap();
    let job = install(&data);
    let direct = parent.path().join("direct");
    symlink(&data, &direct).unwrap();
    assert!(ClassificationWriter::open(&direct).is_err());
    let alias = parent.path().join("alias");
    symlink(parent.path(), &alias).unwrap();
    let first = ClassificationWriter::open(&data).unwrap();
    let other = ClassificationWriter::open(&alias.join("data")).unwrap();
    let _held = first.try_begin(&request(&job)).unwrap();
    assert!(matches!(
        other.try_begin(&request(&job)),
        Err(ClassificationStartError::Busy)
    ));
}
#[cfg(unix)]
#[test]
fn a_named_pipe_is_refused_without_waiting_for_a_peer() {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let name = CString::new(root.path().join(LOCK).as_os_str().as_bytes()).unwrap();
    // SAFETY: nom NUL-terminé dans une fixture privée et mode valide.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(ClassificationWriter::open(root.path())
        .unwrap()
        .try_begin(&request(&job))
        .is_err());
}
#[test]
fn read_is_inert_and_the_real_save_is_reused_instead_of_a_new_calculation() {
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let req = request(&job);
    let writer = ClassificationWriter::open(root.path()).unwrap();
    assert!(writer.load(&req).unwrap().record.is_none());
    assert!(!root.path().join(LOCK).exists());
    let ClassificationStart::Compute(run) = writer.try_begin(&req).unwrap() else {
        panic!("calcul attendu")
    };
    let result = response(run.prepared());
    let saved = run.save(&result).unwrap();
    assert!(saved.record.is_some());
    let other = ClassificationWriter::open(root.path()).unwrap();
    let ClassificationStart::Cached(reused) = other.try_begin(&req).unwrap() else {
        panic!("cache attendu")
    };
    assert_eq!(
        serde_json::to_value(saved).unwrap(),
        serde_json::to_value(reused).unwrap()
    );
}

struct Probe {
    child: std::process::Child,
    events: std::sync::mpsc::Receiver<String>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl Probe {
    fn start(root: &Path, mode: &str) -> Self {
        use std::{
            io::{BufRead, BufReader},
            process::{Command, Stdio},
            sync::mpsc,
            thread,
        };
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "process_lease_probe", "--ignored", "--nocapture"])
            .env_clear()
            .env("PAROLE_WRITER_TEST_ROOT", root)
            .env("PAROLE_WRITER_TEST_MODE", mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, events) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                match line {
                    Ok(line) => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            child,
            events,
            reader: Some(reader),
        }
    }
    fn expect(&self, marker: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let line = self
                .events
                .recv_timeout(left)
                .expect("marqueur du processus fictif absent");
            if line == marker {
                return;
            }
        }
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
    }
}
#[test]
#[ignore = "Entrée auxiliaire exécutée explicitement par le test de processus, pas un scénario sauté."]
fn process_lease_probe() {
    use std::io::{Read, Write};
    let Ok(root) = std::env::var("PAROLE_WRITER_TEST_ROOT") else {
        return;
    };
    let mode = std::env::var("PAROLE_WRITER_TEST_MODE").unwrap();
    let writer = ClassificationWriter::open(Path::new(&root)).unwrap();
    let req = request(&job());
    match (mode.as_str(), writer.try_begin(&req).unwrap()) {
        ("hold", ClassificationStart::Compute(run)) => {
            println!("WRITER_READY");
            std::io::stdout().flush().unwrap();
            // Pipe détenu par le parent : EOF libère aussi le verrou si le banc parent disparaît.
            let mut byte = [0u8; 1];
            let _ = std::io::stdin().read(&mut byte);
            drop(run);
        }
        ("cached", ClassificationStart::Cached(value)) => {
            assert!(value.record.is_some());
            println!("CACHE_READ");
            std::io::stdout().flush().unwrap();
        }
        _ => panic!("état inattendu du processus fictif"),
    }
}
#[test]
fn an_independent_process_excludes_writes_and_its_death_allows_recovery() {
    use parole_core::topic_classification_access::ClassificationStartError;
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let req = request(&job);
    let mut probe = Probe::start(root.path(), "hold");
    probe.expect("WRITER_READY");
    let holder_pid = probe.child.id();
    assert_ne!(holder_pid, std::process::id());
    let writer = ClassificationWriter::open(root.path()).unwrap();
    assert!(matches!(
        writer.try_begin(&req),
        Err(ClassificationStartError::Busy)
    ));
    assert!(writer.load(&req).unwrap().record.is_none());
    let before = fs::metadata(root.path().join(LOCK))
        .unwrap()
        .modified()
        .unwrap();
    probe.child.kill().unwrap();
    assert!(!probe.child.wait().unwrap().success());
    let ClassificationStart::Compute(run) = writer.try_begin(&req).unwrap() else {
        panic!("reprise attendue")
    };
    let value = response(run.prepared());
    run.save(&value).unwrap();
    assert_eq!(
        fs::metadata(root.path().join(LOCK))
            .unwrap()
            .modified()
            .unwrap(),
        before
    );
    let mut reader = Probe::start(root.path(), "cached");
    reader.expect("CACHE_READ");
    let reader_pid = reader.child.id();
    assert_ne!(reader_pid, holder_pid);
    assert!(reader.child.wait().unwrap().success());
    assert_eq!(
        fs::metadata(root.path().join(LOCK))
            .unwrap()
            .modified()
            .unwrap(),
        before
    );
    println!(
        "{}",
        serde_json::json!({"proof":"writer-process-v1","parent_pid":std::process::id(),"holder_pid":holder_pid,"reader_pid":reader_pid,"holder_killed":true,"same_lock_mtime":true,"cached_after_restart":true})
    );
}
