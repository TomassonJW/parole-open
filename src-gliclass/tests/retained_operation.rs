use parole_core::{
    Job, Segment,
    topic_classification_access::{ClassificationRequest, ClassificationWriter},
    topic_questions::PreparedTopicQuestions,
    topic_selection::SelectionThresholds,
};
use parole_gliclass::{Artifact, Gliclass, ModelBundle, retained::RetainedClassification};
use std::{fs, path::Path};
const JOB: &str = "56565656-5656-4656-8656-565656565656";
fn bundle(root: &Path) -> ModelBundle {
    ModelBundle {
        model_id: "fixture/moteur-absent".into(),
        model_revision: "fixture-v1".into(),
        weights: Artifact {
            path: root.join("missing.onnx"),
            sha256: "1".repeat(64),
        },
        tokenizer: Artifact {
            path: root.join("missing.json"),
            sha256: "2".repeat(64),
        },
        library: Artifact {
            path: root.join("missing-library"),
            sha256: "3".repeat(64),
        },
    }
}
fn install(root: &Path) -> Job {
    let mut job = Job::new("paroles-fictives.wav".into(), 2000, 2000);
    job.segments = vec![
        Segment::new(
            0,
            1000,
            "Le budget du projet Atlas reste à discuter.".into(),
        ),
        Segment::new(1000, 2000, "Le calendrier sera précisé.".into()),
    ];
    fs::create_dir(root.join(JOB)).unwrap();
    fs::write(
        root.join(JOB).join("travail.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    job
}
fn request(job: &Job, model: &Gliclass) -> ClassificationRequest {
    ClassificationRequest {
        job_id: JOB.into(),
        source_revision: PreparedTopicQuestions::prepare(JOB, &job.segments)
            .unwrap()
            .question(0, &[])
            .unwrap()
            .source_revision,
        target: 0,
        choices: vec![],
        engine: model.expected_cache_identity().unwrap(),
        thresholds: SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: 0.9,
        },
    }
}
#[test]
fn empty_plan_is_saved_once_then_reused_without_opening_model_files() {
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let mut engine = Gliclass::new(bundle(root.path())).unwrap();
    let req = request(&job, &engine);
    let writer = ClassificationWriter::open(root.path()).unwrap();
    let first = engine.classify_retained(&writer, &req).unwrap();
    assert!(matches!(&first, RetainedClassification::Saved(_)));
    assert_eq!(
        first
            .snapshot()
            .record
            .as_ref()
            .unwrap()
            .selection()
            .assessments
            .len(),
        0
    );
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
    let mut absent = bundle(root.path());
    absent.model_revision.push('2');
    let mut other = Gliclass::new(absent).unwrap();
    let again = other.classify_retained(&writer, &req).unwrap();
    assert!(matches!(&again, RetainedClassification::Reused(_)));
    assert_eq!(
        serde_json::to_value(first.snapshot()).unwrap(),
        serde_json::to_value(again.snapshot()).unwrap()
    );
    assert!(!other.is_loaded());
    assert_eq!(other.inference_count(), 0);
}

#[test]
fn a_busy_writer_is_reported_without_loading_the_model() {
    use parole_core::topic_classification_access::{ClassificationStart, ClassificationStartError};
    use parole_gliclass::retained::RetainedError;
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let mut engine = Gliclass::new(bundle(root.path())).unwrap();
    let mut req = request(&job, &engine);
    req.choices = vec![parole_core::topic_questions::CandidateChoice::Word {
        term: "budget".into(),
    }];
    let writer = ClassificationWriter::open(root.path()).unwrap();
    let held = writer.try_begin(&req).unwrap();
    assert!(matches!(&held, ClassificationStart::Compute(_)));
    assert_eq!(
        engine.classify_retained(&writer, &req).err(),
        Some(RetainedError::Start(ClassificationStartError::Busy))
    );
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
    assert!(writer.load(&req).unwrap().record.is_none());
    drop(held);
    assert!(matches!(
        engine.classify_retained(&writer, &req).err(),
        Some(RetainedError::Classify(_))
    ));
    assert!(matches!(
        writer.try_begin(&req).unwrap(),
        ClassificationStart::Compute(_)
    ));
}
#[test]
fn failed_model_never_stores_a_result_and_releases_the_writer() {
    use parole_core::topic_classification_access::ClassificationStart;
    use parole_gliclass::retained::RetainedError;
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let mut engine = Gliclass::new(bundle(root.path())).unwrap();
    let mut req = request(&job, &engine);
    req.choices = vec![parole_core::topic_questions::CandidateChoice::Word {
        term: "budget".into(),
    }];
    let writer = ClassificationWriter::open(root.path()).unwrap();
    assert!(matches!(
        engine.classify_retained(&writer, &req).err(),
        Some(RetainedError::Classify(_))
    ));
    assert!(writer.load(&req).unwrap().record.is_none());
    assert!(!root.path().join(JOB).join("topics-classified-v1").exists());
    assert!(matches!(
        writer.try_begin(&req).unwrap(),
        ClassificationStart::Compute(_)
    ));
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
}
#[test]
fn cached_result_stays_readable_while_a_different_plan_is_computing() {
    use parole_core::topic_classification_access::ClassificationStart;
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let mut engine = Gliclass::new(bundle(root.path())).unwrap();
    let req = request(&job, &engine);
    let writer = ClassificationWriter::open(root.path()).unwrap();
    let first = engine.classify_retained(&writer, &req).unwrap();
    let mut other = request(&job, &engine);
    other.choices = vec![parole_core::topic_questions::CandidateChoice::Word {
        term: "budget".into(),
    }];
    let second_writer = ClassificationWriter::open(root.path()).unwrap();
    let held = second_writer.try_begin(&other).unwrap();
    assert!(matches!(&held, ClassificationStart::Compute(_)));
    let cached = engine.classify_retained(&writer, &req).unwrap();
    assert!(matches!(&cached, RetainedClassification::Reused(_)));
    assert_eq!(
        serde_json::to_value(first.snapshot()).unwrap(),
        serde_json::to_value(cached.snapshot()).unwrap()
    );
    assert_eq!(engine.inference_count(), 0);
    assert!(!engine.is_loaded());
    assert!(matches!(
        writer.try_begin(&other),
        Err(parole_core::topic_classification_access::ClassificationStartError::Busy)
    ));
    drop(held);
}
#[test]
fn stale_source_is_an_access_error_not_a_model_result() {
    use parole_core::topic_classification_access::ClassificationStartError;
    use parole_gliclass::retained::RetainedError;
    let root = tempfile::tempdir().unwrap();
    let mut job = install(root.path());
    let mut engine = Gliclass::new(bundle(root.path())).unwrap();
    let req = request(&job, &engine);
    let writer = ClassificationWriter::open(root.path()).unwrap();
    engine.classify_retained(&writer, &req).unwrap();
    job.segments[0].text.push('!');
    fs::write(
        root.path().join(JOB).join("travail.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        engine.classify_retained(&writer, &req).err(),
        Some(RetainedError::Start(ClassificationStartError::Unavailable(
            _
        )))
    ));
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
}

#[test]
fn corrupted_saved_data_is_an_access_error_not_a_recalculation() {
    use parole_core::topic_classification_access::ClassificationStartError;
    use parole_gliclass::retained::RetainedError;
    let root = tempfile::tempdir().unwrap();
    let job = install(root.path());
    let mut engine = Gliclass::new(bundle(root.path())).unwrap();
    let req = request(&job, &engine);
    let writer = ClassificationWriter::open(root.path()).unwrap();
    engine.classify_retained(&writer, &req).unwrap();
    let entries: Vec<_> = fs::read_dir(root.path().join(JOB).join("topics-classified-v1"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1);
    fs::write(&entries[0], b"{}").unwrap();
    assert!(matches!(
        engine.classify_retained(&writer, &req).err(),
        Some(RetainedError::Start(ClassificationStartError::Unavailable(
            _
        )))
    ));
    assert_eq!(fs::read(&entries[0]).unwrap(), b"{}");
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
}
