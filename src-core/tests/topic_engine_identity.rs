use parole_core::{
    topic_classification_cache::{ClassificationPlan, EngineIdentity},
    topic_questions::{CandidateChoice, PreparedTopicQuestions},
    topic_selection::SelectionThresholds,
    Segment,
};
fn engine() -> EngineIdentity {
    EngineIdentity {
        model_id: "fixture/modèle".into(),
        model_revision: "révision-一".into(),
        weights_sha256: "1".repeat(64),
        tokenizer_sha256: "2".repeat(64),
        runtime_sha256: "3".repeat(64),
        engine_sha256: "4".repeat(64),
        options_sha256: "5".repeat(64),
    }
}
#[test]
fn every_identity_field_changes_revision_and_invalid_metadata_is_rejected() {
    let original = engine();
    let baseline = original.revision().unwrap();
    let changes: Vec<fn(&mut EngineIdentity)> = vec![
        |e| e.model_id.push('2'),
        |e| e.model_revision.push('2'),
        |e| e.weights_sha256 = "a".repeat(64),
        |e| e.tokenizer_sha256 = "b".repeat(64),
        |e| e.runtime_sha256 = "c".repeat(64),
        |e| e.engine_sha256 = "d".repeat(64),
        |e| e.options_sha256 = "e".repeat(64),
    ];
    let mut all = std::collections::HashSet::new();
    all.insert(baseline.clone());
    for change in &changes {
        let mut altered = original.clone();
        change(&mut altered);
        let revision = altered.revision().unwrap();
        assert_ne!(revision, baseline);
        assert!(all.insert(revision));
    }
    assert_eq!(all.len(), changes.len() + 1);
    for field in 0..7 {
        let mut bad = original.clone();
        match field {
            0 => bad.model_id.clear(),
            1 => bad.model_revision = " bad ".into(),
            2 => bad.weights_sha256 = "A".repeat(64),
            3 => bad.tokenizer_sha256.clear(),
            4 => bad.runtime_sha256 = "z".repeat(64),
            5 => bad.engine_sha256 = "1".repeat(63),
            _ => bad.options_sha256 = "1".repeat(65),
        };
        assert!(bad.revision().is_err());
    }
}

#[test]
fn canonical_revision_preserves_the_preexisting_plan_identity() {
    let source = [Segment::new(0, 1000, "Le budget reste à discuter.".into())];
    let prepared =
        PreparedTopicQuestions::prepare("99999999-9999-4999-8999-999999999999", &source).unwrap();
    let engine = engine();
    let plan = ClassificationPlan::new(
        &prepared,
        0,
        &[CandidateChoice::Word {
            term: "budget".into(),
        }],
        engine.clone(),
        SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: 0.9,
        },
    )
    .unwrap();
    assert_eq!(
        plan.engine_revision(),
        "db894f8137748eaccb7af54f86d93377732a525b6b2311debdaf84541ddb26a2"
    );
    assert_eq!(
        plan.cache_key(),
        "9d19d7a860dda490c82165f84463ff63d1d531510af0fb49a7085d3cdff72c6b"
    );
    assert_eq!(engine.revision().unwrap(), plan.engine_revision());
}
