use parole_core::{
    topic_questions::{CandidateChoice, PreparedTopicQuestions},
    Segment,
};

const JOB: &str = "77777777-7777-4777-8777-777777777777";
fn source() -> Vec<Segment> {
    let mut segments = vec![
        Segment::new(100, 800, "Le projet Atlas prépare le budget.".into()),
        Segment::new(900, 1_400, "Ce calendrier reste à discuter.".into()),
        Segment::new(
            1_500,
            2_300,
            "Le dossier Luciole conserve les illustrations.".into(),
        ),
        Segment::new(
            2_400,
            3_500,
            "Le projet Atlas révise le calendrier et le budget.".into(),
        ),
    ];
    segments[1].speaker_id = Some("voix-originale".into());
    segments[1].translated_text = Some("TRADUCTION NON UTILISÉE".into());
    segments
}
fn word(term: &str) -> CandidateChoice {
    CandidateChoice::Word { term: term.into() }
}

#[test]
fn explicit_folder_pairs_require_same_passage_evidence_and_keep_order() {
    let segments = source();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let choices = vec![
        CandidateChoice::WordInPossibleFolder {
            term: "illustrations".into(),
            folder: "Luciole".into(),
        },
        CandidateChoice::WordInPossibleFolder {
            term: "budget".into(),
            folder: "Atlas".into(),
        },
        CandidateChoice::PossibleFolder {
            name: "Atlas".into(),
        },
    ];
    let question = prepared.question(1, &choices).unwrap();
    assert_eq!(
        question
            .candidates
            .iter()
            .map(|c| c.choice.clone())
            .collect::<Vec<_>>(),
        choices
    );
    assert_eq!(
        question.model_input.as_ref().unwrap().labels,
        vec![
            "illustrations (dossier possible : Luciole)",
            "budget (dossier possible : Atlas)",
            "Dossier possible : Atlas"
        ]
    );
    assert_eq!(question.question.as_deref(), Some("Ce passage concerne-t-il « illustrations (dossier possible : Luciole) » ou « budget (dossier possible : Atlas) » ou « Dossier possible : Atlas » ?"));
    let paired = &question.candidates[0];
    assert!(!paired.lexical_link);
    assert_eq!(
        paired
            .word_evidence
            .iter()
            .map(|e| e.segment_index)
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(
        paired
            .folder_evidence
            .iter()
            .map(|e| e.segment_index)
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(
        &segments[2].text[paired.folder_evidence[0].byte_start..paired.folder_evidence[0].byte_end],
        "Luciole"
    );
    assert!(question.candidates[2].word_evidence.is_empty());
    assert_eq!(question.candidates[2].folder_evidence.len(), 2);
    let invalid = CandidateChoice::WordInPossibleFolder {
        term: "illustrations".into(),
        folder: "Atlas".into(),
    };
    assert!(prepared.question(1, &[word("budget"), invalid]).is_err());
}

#[test]
fn invalid_candidate_lists_fail_atomically_without_normalizing_identifiers() {
    let segments = source();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    for choices in [
        vec![word("budget"), word("budget")],
        vec![word(" budget")],
        vec![word("Budget")],
        vec![word("inexistant")],
        vec![CandidateChoice::PossibleFolder {
            name: "atlas".into(),
        }],
        vec![
            word("budget"),
            CandidateChoice::PossibleFolder {
                name: "Imaginaire".into(),
            },
        ],
    ] {
        assert!(
            prepared.question(1, &choices).is_err(),
            "invalid choices accepted"
        );
    }
    let many = (0..26)
        .map(|i| format!("terme{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let segments = vec![Segment::new(0, 100, many)];
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let choices = (0..26)
        .map(|i| word(&format!("terme{i}")))
        .collect::<Vec<_>>();
    assert_eq!(
        prepared
            .question(0, &choices[..25])
            .unwrap()
            .candidates
            .len(),
        25
    );
    assert!(prepared.question(0, &choices).is_err());
}

#[test]
fn preparation_rejects_invalid_source_identity_times_and_metadata() {
    let segments = source();
    for id in [
        "",
        "../travail",
        "77777777-7777-4777-8777-77777777777",
        "77777777-7777-4777-8777-77777777777G",
    ] {
        assert!(PreparedTopicQuestions::prepare(id, &segments).is_err());
    }
    let mut invalid = source();
    invalid[1].end_ms = 10;
    assert!(PreparedTopicQuestions::prepare(JOB, &invalid).is_err());
    let mut invalid = source();
    invalid[1].end_ms = 9_007_199_254_740_992;
    assert!(PreparedTopicQuestions::prepare(JOB, &invalid).is_err());
    let mut invalid = source();
    invalid[1].speaker_id = Some("v".repeat(257));
    assert!(PreparedTopicQuestions::prepare(JOB, &invalid).is_err());
    let mut invalid = source();
    invalid[1].translated_text = Some("t".repeat(16_385));
    assert!(PreparedTopicQuestions::prepare(JOB, &invalid).is_err());
    let empty = Vec::new();
    assert!(PreparedTopicQuestions::prepare(JOB, &empty)
        .unwrap()
        .question(0, &[])
        .is_err());
}

#[test]
fn model_input_rejects_reserved_markers_and_blank_target_without_rewriting() {
    for marker in [
        "<<LABEL>>",
        "<<SEP>>",
        "<s>",
        "</s>",
        "<pad>",
        "<unk>",
        "<mask>",
    ] {
        for index in 0..3 {
            let mut segments = source();
            segments[index].text.push_str(marker);
            let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
            assert!(
                prepared.question(1, &[word("budget")]).is_err(),
                "marker accepted"
            );
            // Sans demande de modèle, le passage original reste consultable.
            assert_eq!(
                prepared.question(1, &[]).unwrap().target.text,
                segments[1].text
            );
        }
    }
    let mut segments = source();
    segments[1].text = " \n\t".into();
    assert!(PreparedTopicQuestions::prepare(JOB, &segments)
        .unwrap()
        .question(1, &[word("budget")])
        .is_err());
}

#[test]
fn overlong_labels_are_rejected_instead_of_truncated() {
    let name = "A".repeat(4_080);
    let segments = vec![Segment::new(0, 10, format!("projet {name} budget"))];
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let choice = CandidateChoice::WordInPossibleFolder {
        term: "budget".into(),
        folder: name,
    };
    assert!(prepared.question(0, &[choice]).is_err());
}

#[test]
fn repeated_provenance_is_bounded_for_the_whole_request() {
    let text = (0..40)
        .map(|_| "projet Atlas budget")
        .collect::<Vec<_>>()
        .join(" ");
    let segments = (0..100)
        .map(|i| Segment::new(i * 100, i * 100 + 90, text.clone()))
        .collect::<Vec<_>>();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    // Une preuve par occurrence reste importante : ni dédoublonnage ni troncature.
    let choices = [
        word("budget"),
        word("atlas"),
        CandidateChoice::PossibleFolder {
            name: "Atlas".into(),
        },
        CandidateChoice::WordInPossibleFolder {
            term: "budget".into(),
            folder: "Atlas".into(),
        },
        CandidateChoice::WordInPossibleFolder {
            term: "atlas".into(),
            folder: "Atlas".into(),
        },
    ];
    let at_limit = prepared.question(0, &choices[..4]).unwrap();
    assert_eq!(
        at_limit
            .candidates
            .iter()
            .map(|c| c.word_evidence.len() + c.folder_evidence.len())
            .sum::<usize>(),
        20_000
    );
    assert!(prepared.question(0, &choices).is_err());
}

#[test]
fn request_identity_binds_source_order_target_and_exact_transport() {
    let segments = source();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let choices = [word("budget"), word("calendrier")];
    let a = prepared.question(1, &choices).unwrap();
    let b = prepared.question(1, &choices).unwrap();
    let wire = serde_json::to_value(&a).unwrap();
    assert_eq!(wire, serde_json::to_value(&b).unwrap());
    assert_eq!(a.schema_version, 1);
    assert_eq!(a.producer_revision, 1);
    assert_eq!(a.job_id, JOB);
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join(JOB);
    std::fs::create_dir(&folder).unwrap();
    let mut job = parole_core::Job::new("fiction-question.wav".into(), 3_500, 1_000);
    job.segments = segments.clone();
    std::fs::write(
        folder.join("travail.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    let native = parole_core::topic_access::TopicLibrary::open(root.path())
        .unwrap()
        .load(JOB)
        .unwrap();
    assert_eq!(a.source_revision, native.source_revision);
    assert_eq!(a.request_revision.len(), 64);
    assert!(a
        .request_revision
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    assert_eq!(wire["candidates"][0]["choice"]["kind"], "word");
    assert!(wire.get("review_state").is_none());
    assert!(wire.get("confirmed").is_none());
    assert!(wire["target"].get("translated_text").is_none());
    let reversed = prepared
        .question(1, &[choices[1].clone(), choices[0].clone()])
        .unwrap();
    assert_ne!(a.request_revision, reversed.request_revision);
    assert_eq!(
        a.candidates[0].candidate_id,
        reversed.candidates[1].candidate_id
    );
    assert_ne!(a.candidates[0].candidate_id, a.candidates[1].candidate_id);
    assert_eq!(a.candidates[0].candidate_id.len(), 64);
    assert_ne!(
        a.request_revision,
        prepared.question(0, &choices).unwrap().request_revision
    );
    for changed_field in 0..4 {
        let mut changed = segments.clone();
        match changed_field {
            0 => changed[3].text.push_str(" après"),
            1 => changed[3].start_ms += 1,
            2 => changed[3].speaker_id = Some("autre-voix".into()),
            _ => changed[3].translated_text = Some("autre traduction".into()),
        }
        let changed = PreparedTopicQuestions::prepare(JOB, &changed)
            .unwrap()
            .question(1, &choices)
            .unwrap();
        assert_ne!(a.request_revision, changed.request_revision);
        assert_ne!(
            a.candidates[0].candidate_id,
            changed.candidates[0].candidate_id
        );
    }
    let other = PreparedTopicQuestions::prepare("88888888-8888-4888-8888-888888888888", &segments)
        .unwrap()
        .question(1, &choices)
        .unwrap();
    assert_ne!(a.request_revision, other.request_revision);
    if let Ok(path) = std::env::var("PAROLE_QUESTION_FIXTURE_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&a).unwrap()).unwrap();
    }
}

#[test]
fn unicode_offsets_repeated_words_and_absent_neighbors_stay_exact() {
    let segments = vec![Segment::new(
        0,
        50,
        "🌿 budget budget ; projet Érable ; dossier Érable.".into(),
    )];
    let choice = CandidateChoice::WordInPossibleFolder {
        term: "budget".into(),
        folder: "Érable".into(),
    };
    let q = PreparedTopicQuestions::prepare(JOB, &segments)
        .unwrap()
        .question(0, &[choice])
        .unwrap();
    assert!(q.previous.is_none() && q.next.is_none());
    assert!(!q.candidates[0].lexical_link); // Deux occurrences ne sont pas deux passages.
    assert_eq!(
        q.candidates[0]
            .word_evidence
            .iter()
            .map(|e| (e.byte_start, e.byte_end))
            .collect::<Vec<_>>(),
        vec![(5, 11), (12, 18)]
    );
    assert_eq!(q.candidates[0].folder_evidence.len(), 1); // Aucune normalisation Unicode implicite.
    assert_eq!(q.model_input.unwrap().text, "Contexte précédent : \nPassage à classer : 🌿 budget budget ; projet Érable ; dossier Érable.\nContexte suivant : ");
    let segments = source();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let last = prepared.question(3, &[word("budget")]).unwrap();
    assert_eq!(last.previous.unwrap().segment_index, 2);
    assert!(last.next.is_none());
}

#[test]
fn real_question_ids_cross_the_selection_boundary_without_implying_confirmation() {
    use parole_core::topic_selection::{
        select_topics, CandidateScore, ReviewState, SelectionThresholds,
    };
    let segments = source();
    let q = PreparedTopicQuestions::prepare(JOB, &segments)
        .unwrap()
        .question(1, &[word("budget"), word("calendrier")])
        .unwrap();
    let ids = q
        .candidates
        .iter()
        .map(|c| c.candidate_id.clone())
        .collect::<Vec<_>>();
    let scores = vec![
        CandidateScore {
            candidate_id: ids[0].clone(),
            score: 1.0,
        },
        CandidateScore {
            candidate_id: ids[1].clone(),
            score: 0.6,
        },
    ];
    // Scores artificiels de contrat, jamais présentés comme une sortie de modèle.
    let thresholds = SelectionThresholds {
        uncertain_from: 0.5,
        proposed_from: 0.9,
    };
    let result = select_topics(thresholds, &ids, &scores).unwrap();
    assert_eq!(result.state, ReviewState::Ambiguous);
    assert_eq!(result.assessments.len(), 2);
    assert!(select_topics(thresholds, &ids, &[scores[1].clone(), scores[0].clone()]).is_err());
}

#[test]
fn no_candidates_preserves_the_passage_without_a_model_request() {
    let segments = vec![Segment::new(20, 40, "oui".into())];
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let question = prepared.question(0, &[]).unwrap();
    assert_eq!(question.target.text, "oui");
    assert!(question.previous.is_none());
    assert!(question.next.is_none());
    assert!(question.candidates.is_empty());
    assert!(question.question.is_none());
    assert!(question.model_input.is_none());
    assert!(prepared.question(1, &[]).is_err());
    assert!(prepared.question(usize::MAX, &[]).is_err());
}

#[test]
fn prepares_a_real_lexical_choice_with_original_neighbors() {
    let segments = source();
    let before = serde_json::to_vec(&segments).unwrap();
    let prepared = PreparedTopicQuestions::prepare(JOB, &segments).unwrap();
    let question = prepared.question(1, &[word("budget")]).unwrap();
    assert_eq!(question.target.segment_index, 1);
    assert_eq!(question.target.text, segments[1].text);
    assert_eq!(
        (question.target.start_ms, question.target.end_ms),
        (900, 1_400)
    );
    assert_eq!(
        question.target.speaker_id.as_deref(),
        Some("voix-originale")
    );
    assert_eq!(question.previous.as_ref().unwrap().text, segments[0].text);
    assert_eq!(question.next.as_ref().unwrap().text, segments[2].text);
    assert_eq!(
        question.question.as_deref(),
        Some("Ce passage concerne-t-il « budget » ?")
    );
    let input = question.model_input.as_ref().unwrap();
    assert_eq!(input.labels, vec!["budget"]);
    assert_eq!(input.text, "Contexte précédent : Le projet Atlas prépare le budget.\nPassage à classer : Ce calendrier reste à discuter.\nContexte suivant : Le dossier Luciole conserve les illustrations.");
    let candidate = &question.candidates[0];
    assert_eq!(candidate.choice, word("budget"));
    assert!(candidate.lexical_link);
    assert_eq!(
        candidate
            .word_evidence
            .iter()
            .map(|p| p.segment_index)
            .collect::<Vec<_>>(),
        vec![0, 3]
    );
    for evidence in &candidate.word_evidence {
        let original = &segments[evidence.segment_index];
        assert_eq!(
            &original.text[evidence.byte_start..evidence.byte_end],
            "budget"
        );
        assert_eq!(
            (evidence.start_ms, evidence.end_ms),
            (original.start_ms, original.end_ms)
        );
    }
    assert_eq!(serde_json::to_vec(&segments).unwrap(), before);
}
