use parole_core::{
    Segment,
    topic_questions::{CandidateChoice, PreparedTopicQuestions},
};
use parole_gliclass::{Artifact, Gliclass, ModelBundle, Outcome};

fn unavailable() -> ModelBundle {
    let root = std::env::temp_dir().join("parole-files-that-do-not-exist");
    ModelBundle {
        model_id: "fixture/modele-fictif".into(),
        model_revision: "fixture-v1".into(),
        weights: Artifact {
            path: root.join("model.onnx"),
            sha256: "a".repeat(64),
        },
        tokenizer: Artifact {
            path: root.join("tokenizer.json"),
            sha256: "b".repeat(64),
        },
        library: Artifact {
            path: root.join("runtime"),
            sha256: "c".repeat(64),
        },
    }
}
fn source() -> Vec<Segment> {
    vec![
        Segment::new(0, 900, "Le projet Atlas prépare le budget.".into()),
        Segment::new(1_000, 1_900, "Le calendrier reste à préciser.".into()),
    ]
}
const JOB: &str = "99999999-9999-4999-8999-999999999999";

#[test]
fn malformed_bundle_descriptors_are_rejected_without_opening_files() {
    for i in 0..9 {
        let mut bundle = unavailable();
        match i {
            0 => bundle.model_id.clear(),
            1 => bundle.model_revision = " ".into(),
            2 => bundle.model_id = "modele\nconfus".into(),
            3 => bundle.weights.sha256 = "a".repeat(63),
            4 => bundle.tokenizer.sha256 = "A".repeat(64),
            5 => bundle.library.sha256 = "z".repeat(64),
            6 => bundle.weights.path = "relative.onnx".into(),
            7 => bundle.tokenizer.path = std::env::temp_dir().join("..").join("tokenizer.json"),
            _ => bundle.library.path = "https://example.invalid/runtime".into(),
        }
        assert!(Gliclass::new(bundle).is_err(), "descriptor {i}");
    }
}

fn toy_tokenizer(temp: &std::path::Path) -> Artifact {
    let added=[("[UNK]",0),("[PAD]",1),("<<LABEL>>",3),("<<SEP>>",4)].iter().map(|(s,id)| serde_json::json!({"id":id,"content":s,"single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true})).collect::<Vec<_>>();
    let value = serde_json::json!({"version":"1.0","truncation":null,"padding":null,"added_tokens":added,"normalizer":null,"pre_tokenizer":{"type":"WhitespaceSplit"},"post_processor":null,"decoder":null,"model":{"type":"WordLevel","vocab":{"[UNK]":0,"[PAD]":1,"budget":2,"<<LABEL>>":3,"<<SEP>>":4},"unk_token":"[UNK]"}});
    let bytes = serde_json::to_vec(&value).unwrap();
    let path = temp.join("toy-tokenizer.json");
    std::fs::write(&path, &bytes).unwrap();
    Artifact {
        path,
        sha256: parole_core::language::sha256_hex(&bytes),
    }
}
#[test]
fn source_errors_hash_failures_and_long_inputs_never_reach_the_model() {
    let segments = source();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let choices = [CandidateChoice::Word {
        term: "budget".into(),
    }];
    let mut engine = Gliclass::new(unavailable()).unwrap();
    assert_eq!(
        engine.classify(&prepared, 999, &choices).unwrap_err(),
        prepared.question(999, &choices).unwrap_err()
    );
    let temp = tempfile::tempdir().unwrap();
    let mut bundle = unavailable();
    bundle.tokenizer = toy_tokenizer(temp.path());
    bundle.tokenizer.sha256 = "f".repeat(64);
    let mut engine = Gliclass::new(bundle.clone()).unwrap();
    assert!(
        engine
            .classify(&prepared, 0, &choices)
            .unwrap_err()
            .contains("empreinte")
    );
    assert_eq!(engine.inference_count(), 0);
    assert!(!engine.is_loaded());
    assert!(matches!(
        engine.classify(&prepared, 0, &[]).unwrap(),
        Outcome::NoCandidates { .. }
    ));
    bundle.tokenizer = toy_tokenizer(temp.path());
    let mut engine = Gliclass::new(bundle).unwrap();
    let segments = (0..3)
        .map(|i| {
            Segment::new(
                i * 1000,
                i * 1000 + 900,
                format!("budget {}", "oui ".repeat(200)),
            )
        })
        .collect::<Vec<_>>();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    assert!(
        engine
            .classify(&prepared, 1, &choices)
            .unwrap_err()
            .contains("512")
    );
    assert_eq!(engine.inference_count(), 0);
    assert!(!engine.is_loaded());
}

#[test]
fn no_candidates_preserves_the_real_question_without_loading_any_model_file() {
    let segments = source();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let expected = prepared.question(1, &[]).unwrap();
    let mut engine = Gliclass::new(unavailable()).unwrap();
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
    let outcome = engine.classify(&prepared, 1, &[]).unwrap();
    match outcome {
        Outcome::NoCandidates { question } => assert_eq!(
            serde_json::to_value(question).unwrap(),
            serde_json::to_value(expected).unwrap()
        ),
        _ => panic!("Une liste vide ne doit pas produire de scores."),
    }
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
}
