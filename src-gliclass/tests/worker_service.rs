//! Tests de la composition service/processus/protocole/cache, données fictives seulement.
use parole_core::{
    Job, Segment,
    topic_classification_access::{ClassificationLibrary, ClassificationRequest},
    topic_questions::PreparedTopicQuestions,
    topic_selection::SelectionThresholds,
};
use parole_gliclass::{
    Artifact, ModelBundle,
    worker_service::{ServiceCancellation, WorkerService},
};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
const JOB: &str = "56565656-5656-4656-8656-565656565656";
const SERVICE_BUDGET: Duration = Duration::from_secs(30);
fn bundle(root: &Path) -> ModelBundle {
    let fake = |name: &str, digit: &str| Artifact {
        path: root.join(name),
        sha256: digit.repeat(64),
    };
    ModelBundle {
        model_id: "fixture/absent".into(),
        model_revision: "fixture-v1".into(),
        weights: fake("missing.onnx", "1"),
        tokenizer: fake("missing-tokenizer", "2"),
        library: fake("missing-runtime", "3"),
    }
}
fn install(root: &Path) -> Job {
    let mut job = Job::new("fiction.wav".into(), 1000, 1000);
    job.segments = vec![Segment::new(
        0,
        1000,
        "Le budget fictif du projet Atlas.".into(),
    )];
    fs::create_dir(root.join(JOB)).unwrap();
    fs::write(
        root.join(JOB).join("travail.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    job
}
fn request(job: &Job, service: &WorkerService) -> ClassificationRequest {
    ClassificationRequest {
        job_id: JOB.into(),
        source_revision: PreparedTopicQuestions::prepare(JOB, &job.segments)
            .unwrap()
            .question(0, &[])
            .unwrap()
            .source_revision,
        target: 0,
        choices: vec![],
        engine: service.expected_identity().unwrap(),
        thresholds: SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: 0.9,
        },
    }
}
fn fixture_for_extension(extension: &str) -> PathBuf {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_parole-topic-worker"));
    binary
        .parent()
        .unwrap()
        .join("examples")
        .join("worker_transport_fixture")
        .with_extension(extension)
}
fn fixture() -> PathBuf {
    let path = fixture_for_extension(std::env::consts::EXE_EXTENSION);
    assert!(
        path.is_file(),
        "Construire le programme de test avec cargo build --example worker_transport_fixture avant les tests."
    );
    path
}
fn case(
    exe: PathBuf,
    mode: Option<&str>,
    budget: Duration,
) -> (tempfile::TempDir, WorkerService, ClassificationRequest) {
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    if let Some(mode) = mode {
        fs::write(root.path().join("fixture-mode.txt"), mode).unwrap();
    }
    let service =
        WorkerService::new(exe, root.path().to_path_buf(), bundle(root.path()), budget).unwrap();
    let req = request(&job, &service);
    (root, service, req)
}
fn marker(root: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(root.join("fixture-ready.json")).unwrap()).unwrap()
}
fn assert_canonical_child(root: &Path) {
    let ready = marker(root);
    assert_eq!(ready["stage"], "canonical-ready");
    assert_eq!(ready["model_loaded"], false);
    assert_eq!(ready["inference_count"], 0);
    assert_eq!(ready["saved"], true);
    assert!(ready["pid"].as_u64().unwrap() > 0);
    assert!(!root.join("missing.onnx").exists());
}
fn reject_canonical_reply(mode: &str, invalid_cache: bool) {
    let (root, service, req) = case(fixture(), Some(mode), SERVICE_BUDGET);
    let result = service.classify(&req, &ServiceCancellation::default());
    assert_canonical_child(root.path());
    let loaded = ClassificationLibrary::open(root.path()).unwrap().load(&req);
    if invalid_cache {
        assert!(loaded.is_err());
    } else {
        assert!(loaded.unwrap().record.is_some());
    }
    assert!(
        result.is_err(),
        "La présence du cache ne doit pas faire accepter la réponse hostile."
    );
    let error = result.err().unwrap();
    assert!(!error.contains(root.path().to_str().unwrap()));
    assert!(!error.contains("budget fictif"));
}
fn assert_child_reaped(root: &Path) {
    #[cfg(unix)]
    {
        let pid = marker(root)["pid"].as_u64().unwrap();
        assert!(pid > 0 && pid <= libc::pid_t::MAX as u64);
        // Signal 0 observes existence only; it never terminates a process.
        assert_eq!(unsafe { libc::kill(pid as libc::pid_t, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
    #[cfg(not(unix))]
    let _ = root;
}
fn cancel_after_ready(
    root: &Path,
    service: &WorkerService,
    req: &ClassificationRequest,
    expected_stage: &str,
) {
    let cancellation = ServiceCancellation::default();
    thread::scope(|scope| {
        let worker = scope.spawn(|| service.classify(req, &cancellation));
        let deadline = Instant::now() + SERVICE_BUDGET;
        while !root.join("fixture-ready.json").exists() {
            if worker.is_finished() || Instant::now() >= deadline {
                cancellation.cancel();
                panic!("L'enfant n'a pas atteint la barrière d'annulation.");
            }
            thread::sleep(Duration::from_millis(2));
        }
        let ready = marker(root);
        let begin = Instant::now();
        cancellation.cancel();
        fs::write(root.join("fixture-release.txt"), b"release").unwrap();
        let error = worker.join().unwrap().err().unwrap();
        assert_eq!(ready["stage"], expected_stage);
        assert!(ready["pid"].as_u64().unwrap() > 0);
        assert_eq!(error, "Classement local annulé.");
        assert!(begin.elapsed() < Duration::from_secs(5));
    });
    assert_child_reaped(root);
}
#[test]
fn fixture_path_respects_platform_executable_extension() {
    assert_eq!(
        fixture_for_extension("exe").file_name().unwrap(),
        "worker_transport_fixture.exe"
    );
    assert_eq!(
        fixture_for_extension("").file_name().unwrap(),
        "worker_transport_fixture"
    );
    assert_eq!(
        fixture(),
        fixture_for_extension(std::env::consts::EXE_EXTENSION)
    );
}
#[test]
fn canonical_control_and_cache_reuse_start_only_one_child() {
    let (root, service, req) = case(fixture(), Some("canonical-valid"), SERVICE_BUDGET);
    let first = service
        .classify(&req, &ServiceCancellation::default())
        .unwrap();
    assert_canonical_child(root.path());
    let second = service
        .classify(&req, &ServiceCancellation::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
    assert_eq!(
        fs::read_to_string(root.path().join("fixture-starts.txt"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}
#[test]
fn canonical_wrong_run_id_is_rejected() {
    reject_canonical_reply("canonical-wrong-run", false);
}
macro_rules! canonical_rejection {
    ($name:ident, $mode:literal, $invalid_cache:literal) => {
        #[test]
        fn $name() {
            reject_canonical_reply($mode, $invalid_cache);
        }
    };
}
canonical_rejection!(
    canonical_wrong_job_is_rejected,
    "canonical-wrong-job",
    false
);
canonical_rejection!(
    canonical_wrong_source_is_rejected,
    "canonical-wrong-source",
    false
);
canonical_rejection!(
    canonical_wrong_target_is_rejected,
    "canonical-wrong-target",
    false
);
canonical_rejection!(
    canonical_wrong_question_is_rejected,
    "canonical-wrong-question",
    false
);
canonical_rejection!(
    canonical_wrong_engine_is_rejected,
    "canonical-wrong-engine",
    false
);
canonical_rejection!(
    canonical_extra_scores_are_rejected,
    "canonical-extra-scores",
    false
);
canonical_rejection!(
    canonical_missing_reply_is_rejected,
    "canonical-no-reply",
    false
);
canonical_rejection!(
    canonical_trailing_byte_is_rejected,
    "canonical-trailing",
    false
);
canonical_rejection!(
    canonical_stale_source_is_rejected,
    "canonical-stale-source",
    true
);
canonical_rejection!(
    canonical_corrupt_cache_is_rejected,
    "canonical-corrupt-cache",
    true
);
#[test]
fn real_child_saves_empty_classification_and_canonical_read_needs_no_model() {
    let (root, service, req) = case(
        PathBuf::from(env!("CARGO_BIN_EXE_parole-topic-worker")),
        None,
        SERVICE_BUDGET,
    );
    let result = service
        .classify(&req, &ServiceCancellation::default())
        .unwrap();
    assert!(
        result
            .record
            .as_ref()
            .unwrap()
            .selection()
            .assessments
            .is_empty()
    );
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        serde_json::to_value(
            ClassificationLibrary::open(root.path())
                .unwrap()
                .load(&req)
                .unwrap()
        )
        .unwrap()
    );
    // Deuxième appel : le résultat conservé est relu sans nouvel enfant ni modèle.
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::to_value(
            service
                .classify(&req, &ServiceCancellation::default())
                .unwrap()
        )
        .unwrap()
    );
    assert!(!root.path().join("missing.onnx").exists());
}
#[test]
fn zero_exit_without_canonical_record_is_not_success() {
    let (root, service, req) = case(fixture(), Some("zero"), SERVICE_BUDGET);
    assert!(
        service
            .classify(&req, &ServiceCancellation::default())
            .is_err()
    );
    assert_eq!(marker(root.path())["stage"], "transport-ready");
    assert!(
        ClassificationLibrary::open(root.path())
            .unwrap()
            .load(&req)
            .unwrap()
            .record
            .is_none()
    );
}
#[test]
fn hostile_responses_and_stderr_never_become_success() {
    for mode in [
        "wrong-id",
        "truncated",
        "oversized",
        "saved-no-cache",
        "stderr",
    ] {
        let (root, service, req) = case(fixture(), Some(mode), SERVICE_BUDGET);
        let error = service
            .classify(&req, &ServiceCancellation::default())
            .err()
            .unwrap();
        assert!(!error.contains(root.path().to_str().unwrap()));
        assert!(!error.contains("fiction"));
        assert_eq!(marker(root.path())["stage"], "transport-ready");
        assert!(
            ClassificationLibrary::open(root.path())
                .unwrap()
                .load(&req)
                .unwrap()
                .record
                .is_none()
        );
    }
}
#[test]
fn blocked_message_obeys_deadline_after_child_started() {
    let (root, service, req) = case(fixture(), Some("blocked"), Duration::from_secs(2));
    let begin = Instant::now();
    let error = service
        .classify(&req, &ServiceCancellation::default())
        .err()
        .unwrap();
    assert_eq!(marker(root.path())["stage"], "blocked-ready");
    assert_eq!(error, "Le délai du classement local est dépassé.");
    assert!(begin.elapsed() < SERVICE_BUDGET);
    assert_child_reaped(root.path());
}
#[test]
fn blocked_message_obeys_cancellation_after_child_started() {
    let (root, service, req) = case(fixture(), Some("blocked"), SERVICE_BUDGET);
    cancel_after_ready(root.path(), &service, &req, "blocked-ready");
}
#[test]
fn cancellation_does_not_accept_a_late_reply() {
    let (root, service, req) = case(fixture(), Some("canonical-hold"), SERVICE_BUDGET);
    cancel_after_ready(root.path(), &service, &req, "canonical-ready");
    assert_canonical_child(root.path());
    assert!(
        ClassificationLibrary::open(root.path())
            .unwrap()
            .load(&req)
            .unwrap()
            .record
            .is_some()
    );
}
#[test]
fn deadline_preserves_a_canonical_result_saved_before_the_reply() {
    let (root, service, req) = case(fixture(), Some("canonical-hold"), Duration::from_secs(12));
    let begin = Instant::now();
    let error = service
        .classify(&req, &ServiceCancellation::default())
        .err()
        .unwrap();
    assert_canonical_child(root.path());
    assert_eq!(error, "Le délai du classement local est dépassé.");
    assert!(begin.elapsed() < SERVICE_BUDGET);
    assert!(
        ClassificationLibrary::open(root.path())
            .unwrap()
            .load(&req)
            .unwrap()
            .record
            .is_some()
    );
    assert_child_reaped(root.path());
}
#[test]
fn cancellation_before_launch_starts_no_child_and_saves_no_result() {
    let (root, service, req) = case(fixture(), Some("canonical-valid"), SERVICE_BUDGET);
    let cancellation = ServiceCancellation::default();
    cancellation.cancel();
    let error = service.classify(&req, &cancellation).err().unwrap();
    assert_eq!(error, "Classement local annulé.");
    assert!(!root.path().join("fixture-starts.txt").exists());
    assert!(!root.path().join("fixture-ready.json").exists());
    assert!(
        ClassificationLibrary::open(root.path())
            .unwrap()
            .load(&req)
            .unwrap()
            .record
            .is_none()
    );
}
