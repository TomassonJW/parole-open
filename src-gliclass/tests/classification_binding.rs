use parole_core::{
    Job, Segment,
    topic_classification_access::{ClassificationLibrary, ClassificationRequest},
    topic_questions::{CandidateChoice, PreparedTopicQuestions},
    topic_selection::SelectionThresholds,
};
use parole_gliclass::{Artifact, Gliclass, ModelBundle};
use std::{fs, path::Path};
const JOB: &str = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";
fn bundle(root: &Path) -> ModelBundle {
    ModelBundle {
        model_id: "fixture/modele-fictif".into(),
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
fn job() -> Job {
    let mut job = Job::new("fictif.wav".into(), 2000, 2000);
    job.segments = vec![
        Segment::new(
            0,
            1000,
            "Le budget du projet Atlas reste à discuter.".into(),
        ),
        Segment::new(1000, 2000, "Le calendrier sera précisé.".into()),
    ];
    job
}
fn install(root: &Path, job: &Job) {
    fs::create_dir(root.join(JOB)).unwrap();
    fs::write(
        root.join(JOB).join("travail.json"),
        serde_json::to_vec(job).unwrap(),
    )
    .unwrap();
}
fn request(
    job: &Job,
    engine: parole_core::topic_classification_cache::EngineIdentity,
) -> ClassificationRequest {
    ClassificationRequest {
        job_id: JOB.into(),
        source_revision: PreparedTopicQuestions::prepare(JOB, &job.segments)
            .unwrap()
            .question(0, &[])
            .unwrap()
            .source_revision,
        target: 0,
        choices: vec![],
        engine,
        thresholds: SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: 0.9,
        },
    }
}
#[test]
fn expected_identity_and_empty_roundtrip_need_no_model_files_or_inference() {
    let root = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let job = job();
    install(root.path(), &job);
    let mut engine = Gliclass::new(bundle(models.path())).unwrap();
    let identity = engine.expected_cache_identity().unwrap();
    assert_eq!(identity.model_id, "fixture/modele-fictif");
    assert_eq!(identity.model_revision, "fixture-v1");
    assert_eq!(identity.weights_sha256, "1".repeat(64));
    assert_eq!(identity.tokenizer_sha256, "2".repeat(64));
    assert_eq!(identity.runtime_sha256, "3".repeat(64));
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut executable = fs::File::open(std::env::current_exe().unwrap()).unwrap();
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = executable.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    assert_eq!(identity.engine_sha256, format!("{:x}", digest.finalize()));
    assert_eq!(identity.engine_sha256.len(), 64);
    assert_eq!(identity.options_sha256.len(), 64);
    assert!(identity.revision().is_ok());
    assert_eq!(identity, engine.expected_cache_identity().unwrap());
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
    assert_eq!(fs::read_dir(models.path()).unwrap().count(), 0);
    let req = request(&job, identity);
    let library = ClassificationLibrary::open(root.path()).unwrap();
    let work = library.prepare(&req).unwrap();
    let result = engine.classify_prepared(&work).unwrap();
    assert!(result.scores.is_empty());
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
    let stored = library.save(&work, &result).unwrap();
    drop(engine);
    let loaded = ClassificationLibrary::open(root.path())
        .unwrap()
        .load(&req)
        .unwrap();
    assert_eq!(
        serde_json::to_value(stored).unwrap(),
        serde_json::to_value(loaded).unwrap()
    );
    assert_eq!(fs::read_dir(models.path()).unwrap().count(), 0);
}
#[test]
fn a_nonempty_plan_with_correct_identity_must_reach_the_real_runtime() {
    let root = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let job = job();
    install(root.path(), &job);
    let mut engine = Gliclass::new(bundle(models.path())).unwrap();
    let mut req = request(&job, engine.expected_cache_identity().unwrap());
    req.choices = vec![CandidateChoice::Word {
        term: "budget".into(),
    }];
    let library = ClassificationLibrary::open(root.path()).unwrap();
    let work = library.prepare(&req).unwrap();
    // Vrais artefacts absents : aucune réponse de classement ne peut être inventée.
    assert!(engine.classify_prepared(&work).is_err());
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
    assert!(!root.path().join(JOB).join("topics-classified-v1").exists());
}
#[test]
fn a_wrong_expected_engine_is_rejected_before_trying_missing_models() {
    let root = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let job = job();
    install(root.path(), &job);
    let mut engine = Gliclass::new(bundle(models.path())).unwrap();
    let mut identity = engine.expected_cache_identity().unwrap();
    identity.model_revision.push('2');
    let mut req = request(&job, identity);
    req.choices = vec![CandidateChoice::Word {
        term: "budget".into(),
    }];
    let library = ClassificationLibrary::open(root.path()).unwrap();
    let work = library.prepare(&req).unwrap();
    assert_eq!(
        engine.classify_prepared(&work).err().as_deref(),
        Some("Le moteur local ne correspond pas au classement préparé.")
    );
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
    assert!(!root.path().join(JOB).join("topics-classified-v1").exists());
}
