use parole_core::{
    topic_classification_cache::{
        ClassificationCache, ClassificationPlan, ClassificationResponse, EngineIdentity,
    },
    topic_questions::{CandidateChoice, PreparedTopicQuestions},
    topic_selection::{CandidateScore, ReviewState, SelectionThresholds},
    Segment,
};
use std::{
    fs,
    path::{Path, PathBuf},
};
const JOB: &str = "abababab-abab-4bab-8bab-abababababab";
fn source() -> Vec<Segment> {
    vec![
        Segment::new(0, 900, "Le projet Atlas prépare le budget.".into()),
        Segment::new(
            1_000,
            1_900,
            "Le calendrier et le budget restent à discuter.".into(),
        ),
        Segment::new(
            2_000,
            2_900,
            "Le dossier Luciole conserve les illustrations.".into(),
        ),
    ]
}
fn choices() -> Vec<CandidateChoice> {
    vec![
        CandidateChoice::Word {
            term: "budget".into(),
        },
        CandidateChoice::Word {
            term: "calendrier".into(),
        },
    ]
}
fn thresholds() -> SelectionThresholds {
    SelectionThresholds {
        uncertain_from: 0.5,
        proposed_from: 0.9,
    }
}
fn engine() -> EngineIdentity {
    EngineIdentity {
        model_id: "fixture/modele-fictif".into(),
        model_revision: "revision-fictive-v1".into(),
        weights_sha256: "a".repeat(64),
        tokenizer_sha256: "b".repeat(64),
        runtime_sha256: "c".repeat(64),
        engine_sha256: "d".repeat(64),
        options_sha256: "e".repeat(64),
    }
}
fn plan(segments: &[Segment]) -> ClassificationPlan {
    ClassificationPlan::new(
        &PreparedTopicQuestions::prepare(JOB, segments).unwrap(),
        1,
        &choices(),
        engine(),
        thresholds(),
    )
    .unwrap()
}
fn response(plan: &ClassificationPlan, values: &[f64]) -> ClassificationResponse {
    ClassificationResponse {
        question_revision: plan.question().request_revision.clone(),
        engine_revision: plan.engine_revision().to_owned(),
        scores: plan
            .question()
            .candidates
            .iter()
            .zip(values)
            .map(|(c, score)| CandidateScore {
                candidate_id: c.candidate_id.clone(),
                score: *score,
            })
            .collect(),
    }
}
fn path(root: &Path, plan: &ClassificationPlan) -> PathBuf {
    root.join(format!("classification-{}.json", plan.cache_key()))
}

#[test]
fn cache_identity_changes_with_every_engine_field_and_threshold_bit() {
    let segments = source();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let original = plan(&segments);
    let mut variants = Vec::new();
    for index in 0..7 {
        let mut changed = engine();
        match index {
            0 => changed.model_id.push_str("-autre"),
            1 => changed.model_revision.push_str("-autre"),
            2 => changed.weights_sha256 = "f".repeat(64),
            3 => changed.tokenizer_sha256 = "f".repeat(64),
            4 => changed.runtime_sha256 = "f".repeat(64),
            5 => changed.engine_sha256 = "f".repeat(64),
            _ => changed.options_sha256 = "f".repeat(64),
        }
        let variant =
            ClassificationPlan::new(&prepared, 1, &choices(), changed, thresholds()).unwrap();
        assert_ne!(original.engine_revision(), variant.engine_revision());
        assert_ne!(
            original.cache_key(),
            variant.cache_key(),
            "engine field omitted"
        );
        variants.push(variant.cache_key().to_owned());
    }
    for changed in [
        SelectionThresholds {
            uncertain_from: f64::from_bits(0.5f64.to_bits() + 1),
            proposed_from: 0.9,
        },
        SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: f64::from_bits(0.9f64.to_bits() + 1),
        },
    ] {
        let variant = ClassificationPlan::new(&prepared, 1, &choices(), engine(), changed).unwrap();
        assert_eq!(original.engine_revision(), variant.engine_revision());
        assert_ne!(
            original.cache_key(),
            variant.cache_key(),
            "threshold bit omitted"
        );
        variants.push(variant.cache_key().to_owned());
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let cache = ClassificationCache::new(&root);
    cache
        .save(&original, &response(&original, &[1.0, 0.6]))
        .unwrap();
    let other = ClassificationPlan::new(
        &prepared,
        1,
        &choices(),
        engine(),
        SelectionThresholds {
            uncertain_from: 0.4,
            proposed_from: 0.8,
        },
    )
    .unwrap();
    assert!(cache.load(&other).unwrap().is_none());
    assert!(cache.load(&original).unwrap().is_some());
    assert_eq!(
        variants.len(),
        variants
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    );
}

#[test]
fn incomplete_engine_or_invalid_thresholds_cannot_create_a_plan() {
    let segments = source();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    for i in 0..9 {
        let mut invalid = engine();
        match i {
            0 => invalid.model_id.clear(),
            1 => invalid.model_revision.clear(),
            2 => invalid.model_id = " modele".into(),
            3 => invalid.model_revision = "x\ny".into(),
            4 => invalid.weights_sha256 = "A".repeat(64),
            5 => invalid.tokenizer_sha256 = "b".repeat(63),
            6 => invalid.runtime_sha256 = "z".repeat(64),
            7 => invalid.engine_sha256.clear(),
            _ => invalid.options_sha256 = "e".repeat(65),
        }
        assert!(
            ClassificationPlan::new(&prepared, 1, &choices(), invalid, thresholds()).is_err(),
            "invalid engine {i}"
        );
    }
    for thresholds in [
        SelectionThresholds {
            uncertain_from: f64::NAN,
            proposed_from: 0.9,
        },
        SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: 0.5,
        },
        SelectionThresholds {
            uncertain_from: -0.1,
            proposed_from: 0.9,
        },
        SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: f64::INFINITY,
        },
    ] {
        assert!(ClassificationPlan::new(&prepared, 1, &choices(), engine(), thresholds).is_err());
    }
}

#[test]
fn a_wrong_question_or_engine_response_is_rejected_without_writing() {
    let segments = source();
    let plan = plan(&segments);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let cache = ClassificationCache::new(&root);
    for i in 0..4 {
        let mut wrong = response(&plan, &[1.0, 0.6]);
        match i {
            0 => wrong.question_revision = "f".repeat(64),
            1 => wrong.engine_revision = "f".repeat(64),
            2 => wrong.question_revision.clear(),
            _ => wrong.engine_revision.clear(),
        }
        assert!(cache.save(&plan, &wrong).is_err(), "wrong binding {i}");
        assert!(!root.exists());
    }
}

#[test]
fn repeated_save_is_idempotent_and_divergent_scores_never_replace_valid_data() {
    let segments = source();
    let plan = plan(&segments);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let cache = ClassificationCache::new(&root);
    let response = response(&plan, &[1.0, 0.6]);
    cache.save(&plan, &response).unwrap();
    let file = fs::File::options()
        .write(true)
        .open(path(&root, &plan))
        .unwrap();
    file.set_times(
        fs::FileTimes::new()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(60)),
    )
    .unwrap();
    let before = fs::read(path(&root, &plan)).unwrap();
    let modified = file.metadata().unwrap().modified().unwrap();
    drop(file);
    ClassificationCache::new(&root)
        .save(&plan, &response)
        .unwrap();
    assert_eq!(
        fs::metadata(path(&root, &plan))
            .unwrap()
            .modified()
            .unwrap(),
        modified,
        "identical save rewrote file"
    );
    let mut different = response;
    different.scores[0].score = 0.95;
    assert!(ClassificationCache::new(&root)
        .save(&plan, &different)
        .is_err());
    assert_eq!(fs::read(path(&root, &plan)).unwrap(), before);
    assert_eq!(
        cache.load(&plan).unwrap().unwrap().selection().assessments[0].score,
        1.0
    );
}

fn replace_json(file: &Path, value: &serde_json::Value, resign: bool) {
    let mut value = value.clone();
    if resign {
        value["payload_sha256"] =
            parole_core::language::sha256_hex(&serde_json::to_vec(&value["payload"]).unwrap())
                .into();
    }
    fs::write(file, serde_json::to_vec(&value).unwrap()).unwrap();
}
#[test]
fn corrupted_or_rebound_saved_records_are_errors_and_never_repaired() {
    let segments = source();
    let plan = plan(&segments);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let cache = ClassificationCache::new(&root);
    let response = response(&plan, &[1.0, 0.6]);
    cache.save(&plan, &response).unwrap();
    let file = path(&root, &plan);
    let pristine: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert!(pristine["payload_sha256"]
        .as_str()
        .is_some_and(|s| s.len() == 64));
    for i in 0..13 {
        let mut wrong = pristine.clone();
        match i {
            0 => wrong["schema_version"] = 2.into(),
            1 => wrong["policy_revision"] = 2.into(),
            2 => wrong["payload_sha256"] = "f".repeat(64).into(),
            3 => wrong["payload"]["cache_key"] = "f".repeat(64).into(),
            4 => wrong["payload"]["question"]["target"]["text"] = "Texte altéré".into(),
            5 => wrong["payload"]["engine"]["runtime_sha256"] = "f".repeat(64).into(),
            6 => wrong["payload"]["threshold_bits"][0] = 0.4f64.to_bits().into(),
            7 => wrong["payload"]["summary"][0] = "confirmed".into(),
            8 => wrong["payload"]["summary"][1][0][1] = "below_threshold".into(),
            9 => wrong["payload"]["scores"][0]["score_bits"] = f64::NAN.to_bits().into(),
            10 => wrong["payload"]["scores"][0]["candidate_id"] = "autre".into(),
            11 => wrong["payload"]["confirmed"] = true.into(),
            _ => wrong["human_confirmation"] = true.into(),
        }
        replace_json(&file, &wrong, i != 2);
        let before = fs::read(&file).unwrap();
        assert!(cache.load(&plan).is_err(), "accepted corruption {i}");
        assert!(
            cache.save(&plan, &response).is_err(),
            "silently repaired corruption {i}"
        );
        assert_eq!(fs::read(&file).unwrap(), before);
    }
    // Une mutation numérique cohérente mais non réempreintée ne doit pas passer.
    let mut wrong = pristine.clone();
    wrong["payload"]["scores"][0]["score_bits"] = 0.99f64.to_bits().into();
    replace_json(&file, &wrong, false);
    assert!(cache.load(&plan).is_err());
    replace_json(&file, &pristine, false);
    assert!(cache.load(&plan).unwrap().is_some());
}

#[test]
fn untrusted_relative_or_parent_traversal_roots_are_refused_even_when_absent() {
    let segments = source();
    let plan = plan(&segments);
    let relative = Path::new("PAROLE_UNTRUSTED_RELATIVE_TEST_NEVER_CREATED");
    assert!(!relative.exists());
    let cache = ClassificationCache::new(relative);
    assert!(cache.load(&plan).is_err());
    assert!(cache.save(&plan, &response(&plan, &[1.0, 0.6])).is_err());
    assert!(!relative.exists());
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("absent").join("..").join("out");
    let cache = ClassificationCache::new(&root);
    assert!(cache.load(&plan).is_err());
    assert!(cache.save(&plan, &response(&plan, &[1.0, 0.6])).is_err());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn created_cache_is_private_and_a_writable_shared_directory_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let segments = source();
    let plan = plan(&segments);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let cache = ClassificationCache::new(&root);
    let response = response(&plan, &[1.0, 0.6]);
    cache.save(&plan, &response).unwrap();
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(path(&root, &plan))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let before = fs::read(path(&root, &plan)).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(cache.load(&plan).is_err());
    assert!(cache.save(&plan, &response).is_err());
    assert_eq!(fs::read(path(&root, &plan)).unwrap(), before);
}

#[test]
fn full_source_target_job_and_order_are_bound_without_erasing_old_results() {
    let mut segments = source();
    segments.push(Segment::new(
        3_000,
        3_900,
        "Parole conservée hors voisinage immédiat.".into(),
    ));
    let original = plan(&segments);
    let mut changed = segments.clone();
    changed[3].text.push_str(" Autre.");
    let mut variants = vec![plan(&changed)];
    changed = segments.clone();
    changed[1].speaker_id = Some("voix originale".into());
    variants.push(plan(&changed));
    changed = segments.clone();
    changed[1].translated_text = Some("Traduction conservée".into());
    variants.push(plan(&changed));
    changed = segments.clone();
    changed[1].start_ms += 1;
    variants.push(plan(&changed));
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    variants
        .push(ClassificationPlan::new(&prepared, 0, &choices(), engine(), thresholds()).unwrap());
    let mut reversed = choices();
    reversed.reverse();
    variants
        .push(ClassificationPlan::new(&prepared, 1, &reversed, engine(), thresholds()).unwrap());
    variants.push(
        ClassificationPlan::new(
            &PreparedTopicQuestions::prepare("dededede-dede-4ede-8ede-dededededede", &segments)
                .unwrap(),
            1,
            &choices(),
            engine(),
            thresholds(),
        )
        .unwrap(),
    );
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let cache = ClassificationCache::new(&root);
    cache
        .save(&original, &response(&original, &[1.0, 0.6]))
        .unwrap();
    let before = fs::read(path(&root, &original)).unwrap();
    for variant in &variants {
        assert_ne!(original.cache_key(), variant.cache_key());
        assert!(cache.load(variant).unwrap().is_none());
        // Copier une entrée valide sous un autre nom ne lui donne aucune autorité.
        fs::write(path(&root, variant), &before).unwrap();
        assert!(cache.load(variant).is_err());
    }
    assert_eq!(fs::read(path(&root, &original)).unwrap(), before);
    assert!(cache.load(&original).unwrap().is_some());
}
#[test]
fn invalid_scores_are_atomic_even_with_an_existing_record() {
    let segments = source();
    let plan = plan(&segments);
    for existing in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cache");
        let cache = ClassificationCache::new(&root);
        let before = if existing {
            cache.save(&plan, &response(&plan, &[1.0, 0.6])).unwrap();
            Some(fs::read(path(&root, &plan)).unwrap())
        } else {
            None
        };
        for i in 0..10 {
            let mut bad = response(&plan, &[1.0, 0.6]);
            match i {
                0 => bad.scores.clear(),
                1 => {
                    bad.scores.pop();
                }
                2 => bad.scores.push(bad.scores[0].clone()),
                3 => bad.scores.reverse(),
                4 => bad.scores[1].candidate_id = bad.scores[0].candidate_id.clone(),
                5 => bad.scores[0].candidate_id.push(' '),
                6 => bad.scores[0].score = f64::NAN,
                7 => bad.scores[0].score = f64::INFINITY,
                8 => bad.scores[0].score = -0.1,
                _ => bad.scores[0].score = 1.01,
            }
            assert!(cache.save(&plan, &bad).is_err(), "score fault {i}");
            if let Some(ref bytes) = before {
                assert_eq!(&fs::read(path(&root, &plan)).unwrap(), bytes);
            } else {
                assert!(!root.exists());
            }
        }
    }
}
#[test]
fn float_bits_and_all_four_states_survive_readback_without_confirmation() {
    let segments = source();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let cases = [
        (&[][..], &[][..], ReviewState::NoCandidates),
        (
            &choices()[..],
            &[-0.0, f64::from_bits(1)][..],
            ReviewState::NoSuggestion,
        ),
        (&choices()[..], &[1.0, 0.5][..], ReviewState::Ambiguous),
        (&choices()[..], &[1.0, 0.9][..], ReviewState::Proposed),
    ];
    for (i, (choices, scores, state)) in cases.iter().enumerate() {
        let plan = ClassificationPlan::new(&prepared, 1, choices, engine(), thresholds()).unwrap();
        let root = temp.path().join(i.to_string());
        let cache = ClassificationCache::new(&root);
        cache.save(&plan, &response(&plan, scores)).unwrap();
        let loaded = ClassificationCache::new(&root)
            .load(&plan)
            .unwrap()
            .unwrap();
        assert_eq!(&loaded.selection().state, state);
        for (a, b) in loaded.selection().assessments.iter().zip(*scores) {
            assert_eq!(a.score.to_bits(), b.to_bits());
        }
        let wire = serde_json::to_string(&loaded).unwrap();
        assert!(!wire.contains("confirmed"));
        if choices.is_empty() {
            assert!(loaded.question().model_input.is_none());
        }
    }
    // Valeur à un bit sous le seuil fort : jamais arrondie vers la bande proposée.
    let t = SelectionThresholds {
        uncertain_from: f64::from_bits(0.5f64.to_bits() + 1),
        proposed_from: f64::from_bits(0.9f64.to_bits() + 1),
    };
    let plan = ClassificationPlan::new(&prepared, 1, &choices(), engine(), t).unwrap();
    let cache = ClassificationCache::new(&temp.path().join("edges"));
    let values = [0.9, t.proposed_from];
    cache.save(&plan, &response(&plan, &values)).unwrap();
    let loaded = cache.load(&plan).unwrap().unwrap();
    assert_eq!(loaded.selection().state, ReviewState::Ambiguous);
    assert_eq!(
        loaded.selection().thresholds.uncertain_from.to_bits(),
        t.uncertain_from.to_bits()
    );
    assert_eq!(
        loaded.selection().thresholds.proposed_from.to_bits(),
        t.proposed_from.to_bits()
    );
    for (a, b) in loaded.selection().assessments.iter().zip(values) {
        assert_eq!(a.score.to_bits(), b.to_bits());
    }
}
#[test]
fn malformed_oversized_noncanonical_and_duplicate_json_never_looks_absent() {
    let segments = source();
    let plan = plan(&segments);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let cache = ClassificationCache::new(&root);
    cache.save(&plan, &response(&plan, &[1.0, 0.6])).unwrap();
    let file = path(&root, &plan);
    let original = fs::read(&file).unwrap();
    let text = String::from_utf8(original.clone()).unwrap();
    let duplicate = text.replacen("\"target\":{", "\"target\":null,\"target\":{", 1);
    assert_ne!(duplicate, text);
    let mut padded = original.clone();
    padded.push(b' ');
    for bytes in [
        Vec::new(),
        b"{}".to_vec(),
        b"not json".to_vec(),
        padded,
        duplicate.into_bytes(),
    ] {
        fs::write(&file, bytes).unwrap();
        assert!(cache.load(&plan).is_err());
    }
    let handle = fs::File::create(&file).unwrap();
    handle
        .set_len(parole_core::topic_classification_cache::MAX_CLASSIFICATION_BYTES as u64 + 1)
        .unwrap();
    assert!(cache.load(&plan).is_err());
    drop(handle);
    fs::write(&file, original).unwrap();
    assert!(cache.load(&plan).unwrap().is_some());
    // Une cible qui est un répertoire n'est ni un cache absent ni réparable automatiquement.
    let other_root = temp.path().join("other");
    fs::create_dir(&other_root).unwrap();
    fs::create_dir(path(&other_root, &plan)).unwrap();
    let other = ClassificationCache::new(&other_root);
    assert!(other.load(&plan).is_err());
    assert!(other.save(&plan, &response(&plan, &[1.0, 0.6])).is_err());
}
#[cfg(unix)]
#[test]
fn symbolic_hard_and_directory_links_are_refused_without_touching_targets() {
    use std::os::unix::fs::symlink;
    let segments = source();
    let plan = plan(&segments);
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    let cache = ClassificationCache::new(&real);
    cache.save(&plan, &response(&plan, &[1.0, 0.6])).unwrap();
    let before = fs::read(path(&real, &plan)).unwrap();
    let symbolic_root = temp.path().join("symbolic");
    fs::create_dir(&symbolic_root).unwrap();
    symlink(path(&real, &plan), path(&symbolic_root, &plan)).unwrap();
    let hard_root = temp.path().join("hard");
    fs::create_dir(&hard_root).unwrap();
    fs::hard_link(path(&real, &plan), path(&hard_root, &plan)).unwrap();
    let directory_link = temp.path().join("directory-link");
    symlink(&real, &directory_link).unwrap();
    let dangling_root = temp.path().join("dangling");
    fs::create_dir(&dangling_root).unwrap();
    symlink(temp.path().join("never"), path(&dangling_root, &plan)).unwrap();
    for root in [
        &real,
        &symbolic_root,
        &hard_root,
        &directory_link,
        &dangling_root,
    ] {
        let cache = ClassificationCache::new(root);
        assert!(cache.load(&plan).is_err());
        assert!(cache.save(&plan, &response(&plan, &[1.0, 0.6])).is_err());
    }
    assert_eq!(fs::read(path(&real, &plan)).unwrap(), before);
    assert!(!temp.path().join("never").exists());
}

#[cfg(unix)]
#[test]
fn a_linked_parent_is_rejected_before_any_cache_directory_is_created() {
    use std::os::unix::fs::symlink;
    let segments = source();
    let plan = plan(&segments);
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let alias = temp.path().join("alias");
    symlink(&target, &alias).unwrap();
    let root = alias.join("absent-cache");
    let cache = ClassificationCache::new(&root);
    assert!(cache.load(&plan).is_err());
    assert!(cache.save(&plan, &response(&plan, &[1.0, 0.6])).is_err());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
}

#[test]
fn a_real_question_survives_disk_and_a_new_store_without_inference() {
    let segments = source();
    let plan = plan(&segments);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("classifications");
    let cache = ClassificationCache::new(&root);
    assert!(cache.load(&plan).unwrap().is_none());
    assert!(!root.exists());
    // Scores artificiels de contrat, pas une prétendue sortie de modèle.
    let saved = cache.save(&plan, &response(&plan, &[1.0, 0.6])).unwrap();
    assert_eq!(saved.selection().state, ReviewState::Ambiguous);
    assert_eq!(saved.selection().assessments.len(), 2);
    assert_eq!(saved.question().target.text, segments[1].text);
    assert_eq!(saved.question().target.start_ms, 1_000);
    assert_eq!(
        saved.question().previous.as_ref().unwrap().text,
        segments[0].text
    );
    assert_eq!(
        saved.question().next.as_ref().unwrap().text,
        segments[2].text
    );
    let bytes = fs::read(path(&root, &plan)).unwrap();
    assert!(!bytes.is_empty());
    let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(wire["schema_version"], 1);
    assert_eq!(
        wire["payload"]["question"]["request_revision"],
        plan.question().request_revision
    );
    assert_eq!(
        wire["payload"]["engine"]["model_id"],
        "fixture/modele-fictif"
    );
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains("confirmed"));
    drop(cache);
    let reopened = ClassificationCache::new(&root)
        .load(&plan)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&saved).unwrap(),
        serde_json::to_value(&reopened).unwrap()
    );
    assert_eq!(fs::read(path(&root, &plan)).unwrap(), bytes);
}
