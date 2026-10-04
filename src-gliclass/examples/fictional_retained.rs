//! Preuve fictive uniquement, pas un service produit ni une commande utilisateur.
use parole_core::{
    Job, Segment,
    topic_classification_access::{
        ClassificationRequest, ClassificationStart, ClassificationWriter,
    },
    topic_classification_cache::EngineIdentity,
    topic_questions::{CandidateChoice, PreparedTopicQuestions},
    topic_selection::SelectionThresholds,
};
use parole_gliclass::{
    Artifact, Gliclass, ModelBundle,
    retained::{RetainedClassification, RetainedError},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
const JOB: &str = "90909090-9090-4090-8090-909090909090";
const TAG: &str = "parole-fictional-retained-v1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCase {
    index: usize,
    snapshot_json: String,
    candidate_ids: Vec<String>,
    score_bits: Vec<u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    tag: String,
    engine: EngineIdentity,
    cases: Vec<StoredCase>,
    job_bytes: Vec<u8>,
}
fn bounded(path: &Path, max: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    File::open(path)
        .unwrap()
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= max);
    bytes
}
fn fictional_job() -> Job {
    let mut job = Job::new("paroles-fictives.wav".into(), 3000, 3000);
    job.segments = vec![
        Segment::new(0, 1000, "Le projet Atlas prépare le budget.".into()),
        Segment::new(
            1000,
            2000,
            "Le calendrier du projet Atlas reste à discuter.".into(),
        ),
        Segment::new(
            2000,
            3000,
            "Le dossier Luciole conserve les illustrations.".into(),
        ),
    ];
    job
}
fn choices(index: usize) -> Vec<CandidateChoice> {
    let mut choices = vec![
        CandidateChoice::WordInPossibleFolder {
            term: "budget".into(),
            folder: "Atlas".into(),
        },
        CandidateChoice::WordInPossibleFolder {
            term: "calendrier".into(),
            folder: "Atlas".into(),
        },
        CandidateChoice::WordInPossibleFolder {
            term: "illustrations".into(),
            folder: "Luciole".into(),
        },
    ];
    if index == 1 {
        choices.reverse();
    }
    if index == 2 {
        choices.clear();
    }
    choices
}

fn request(job: &Job, engine: &EngineIdentity, index: usize) -> ClassificationRequest {
    ClassificationRequest {
        job_id: JOB.into(),
        source_revision: PreparedTopicQuestions::prepare(JOB, &job.segments)
            .unwrap()
            .question(1, &[])
            .unwrap()
            .source_revision,
        target: 1,
        choices: choices(index),
        engine: engine.clone(),
        thresholds: SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: 0.9,
        },
    }
}
fn read_job(root: &Path) -> (Vec<u8>, Job) {
    assert_eq!(bounded(&root.join("fixture-kind.txt"), 128), TAG.as_bytes());
    let bytes = bounded(&root.join(JOB).join("travail.json"), 1024 * 1024);
    let job = serde_json::from_slice(&bytes).expect("Travail fictif");
    (bytes, job)
}
fn fail_init(root: &Path, bundle_file: &Path) {
    fs::create_dir(root).expect("Racine neuve obligatoire");
    fs::write(root.join("fixture-kind.txt"), TAG).unwrap();
    fs::create_dir(root.join(JOB)).unwrap();
    let job = fictional_job();
    fs::write(
        root.join(JOB).join("travail.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    let good: ModelBundle = serde_json::from_slice(&bounded(bundle_file, 16 * 1024)).unwrap();
    let invalid = b"Fichier fictif volontairement invalide, pas un modele ONNX.";
    let invalid_path = root.join("invalid-model.fixture");
    fs::write(&invalid_path, invalid).unwrap();
    let mut bad = good.clone();
    bad.model_id = "fixture/initialisation-invalide".into();
    bad.model_revision = "fixture-invalid-v1".into();
    bad.weights = Artifact {
        path: invalid_path,
        sha256: format!("{:x}", Sha256::digest(invalid)),
    };
    let mut failed = Gliclass::new(bad).unwrap();
    let req = request(&job, &failed.expected_cache_identity().unwrap(), 0);
    let writer = ClassificationWriter::open(root).unwrap();
    let error = failed
        .classify_retained(&writer, &req)
        .err()
        .expect("Échec natif attendu");
    assert_eq!(
        error,
        RetainedError::Classify("Le modèle local ne peut pas être chargé avec ces options.".into())
    );
    assert!(!failed.is_loaded());
    assert_eq!(failed.inference_count(), 0);
    assert!(writer.load(&req).unwrap().record.is_none());
    assert!(matches!(
        writer.try_begin(&req).unwrap(),
        ClassificationStart::Compute(_)
    ));
    let mut retry = Gliclass::new(good).unwrap();
    let valid = request(&job, &retry.expected_cache_identity().unwrap(), 0);
    let retry_error = retry
        .classify_retained(&writer, &valid)
        .err()
        .expect("Processus neuf requis");
    assert_eq!(
        retry_error,
        RetainedError::Classify(
            "Une autre instance possède déjà le moteur local dans ce processus.".into()
        )
    );
    assert!(!retry.is_loaded());
    assert_eq!(retry.inference_count(), 0);
    assert!(matches!(
        writer.try_begin(&valid).unwrap(),
        ClassificationStart::Compute(_)
    ));
    assert!(!root.join(JOB).join("topics-classified-v1").exists());
    println!(
        "{}",
        serde_json::json!({"mode":"fail-init","pid":std::process::id(),"inferences":0,"initialization_failed":true,"same_process_retry_refused":true,"writer_released":true,"no_result_saved":true})
    );
}
fn infer(root: &Path, bundle_file: &Path) {
    let (job_bytes, job) = read_job(root);
    assert!(!root.join("retained-state.json").exists());
    let bundle: ModelBundle = serde_json::from_slice(&bounded(bundle_file, 16 * 1024)).unwrap();
    let mut model = Gliclass::new(bundle).unwrap();
    let engine = model.expected_cache_identity().unwrap();
    let writer = ClassificationWriter::open(root).unwrap();
    let mut cases = Vec::new();
    for index in [0, 2] {
        let req = request(&job, &engine, index);
        let result = model.classify_retained(&writer, &req).unwrap();
        assert!(matches!(&result, RetainedClassification::Saved(_)));
        let snap = result.snapshot();
        let scores = &snap.record.as_ref().unwrap().selection().assessments;
        let case = StoredCase {
            index,
            snapshot_json: serde_json::to_string(snap).unwrap(),
            candidate_ids: scores.iter().map(|v| v.candidate_id.clone()).collect(),
            score_bits: scores.iter().map(|v| v.score.to_bits()).collect(),
        };
        let again = model.classify_retained(&writer, &req).unwrap();
        assert!(matches!(&again, RetainedClassification::Reused(_)));
        assert_eq!(
            serde_json::to_string(again.snapshot()).unwrap(),
            case.snapshot_json
        );
        cases.push(case);
    }
    assert_eq!(model.inference_count(), 1);
    assert!(model.is_loaded());
    assert_eq!(
        bounded(&root.join(JOB).join("travail.json"), 1024 * 1024),
        job_bytes
    );
    fs::write(
        root.join("retained-state.json"),
        serde_json::to_vec(&State {
            tag: TAG.into(),
            engine,
            cases,
            job_bytes,
        })
        .unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        serde_json::json!({"mode":"infer","pid":std::process::id(),"inferences":model.inference_count(),"cases":2,"loaded":model.is_loaded(),"same_process_reuse":true})
    );
}
fn read(root: &Path) {
    let (job_bytes, job) = read_job(root);
    let state: State =
        serde_json::from_slice(&bounded(&root.join("retained-state.json"), 2 * 1024 * 1024))
            .unwrap();
    assert_eq!(state.tag, TAG);
    assert_eq!(state.job_bytes, job_bytes);
    let fake = |name: &str, digit: &str| Artifact {
        path: root.join(name),
        sha256: digit.repeat(64),
    };
    // Descripteur fictif différent, dont les fichiers sont absents : la relecture n’en a pas besoin.
    let mut absent = Gliclass::new(ModelBundle {
        model_id: "fixture/aucun-modele".into(),
        model_revision: "absent-v1".into(),
        weights: fake("deliberately-missing.onnx", "1"),
        tokenizer: fake("deliberately-missing-tokenizer.json", "2"),
        library: fake("deliberately-missing-runtime", "3"),
    })
    .unwrap();
    let writer = ClassificationWriter::open(root).unwrap();
    for case in &state.cases {
        let result = absent
            .classify_retained(&writer, &request(&job, &state.engine, case.index))
            .unwrap();
        assert!(matches!(&result, RetainedClassification::Reused(_)));
        assert_eq!(
            serde_json::to_string(result.snapshot()).unwrap(),
            case.snapshot_json
        );
        let scores = &result
            .snapshot()
            .record
            .as_ref()
            .unwrap()
            .selection()
            .assessments;
        assert_eq!(
            scores
                .iter()
                .map(|s| s.candidate_id.clone())
                .collect::<Vec<_>>(),
            case.candidate_ids
        );
        assert_eq!(
            scores.iter().map(|s| s.score.to_bits()).collect::<Vec<_>>(),
            case.score_bits
        );
    }
    assert!(!absent.is_loaded());
    assert_eq!(absent.inference_count(), 0);
    println!(
        "{}",
        serde_json::json!({"mode":"read","pid":std::process::id(),"inferences":0,"cases":state.cases.len(),"model_loaded":false,"model_descriptor":"fictitious, unavailable and unused","snapshots_and_score_bits_identical":true})
    );
}
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    assert_eq!(
        args.len(),
        5,
        "mode, racine fictive, descripteur, garde explicite"
    );
    let mode = args[1].to_str().expect("Mode");
    let root = PathBuf::from(&args[2]);
    let bundle = PathBuf::from(&args[3]);
    assert_eq!(args[4], "fictional-fixture-only");
    assert!(
        root.is_absolute()
            && !root
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
    );
    match mode {
        "fail-init" => fail_init(&root, &bundle),
        "infer" => infer(&root, &bundle),
        "read" => read(&root),
        _ => panic!("Mode inconnu"),
    }
}
