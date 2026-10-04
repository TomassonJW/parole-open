use parole_core::topic_candidates::prepare_topic_candidates;
use parole_core::Segment;

#[test]
fn empty_source_has_no_suggestions() {
    let result = prepare_topic_candidates(&[]).unwrap();
    assert_eq!(result.schema_version, 1);
    assert!(result.words.is_empty());
    assert!(result.links.is_empty());
    assert!(result.possible_folders.is_empty());
    assert!(result.without_suggestion.is_empty());
}

#[test]
fn a_rare_word_has_an_exact_source_but_banal_speech_abstains() {
    let source = vec![
        segment(10, "Bonjour et merci."),
        segment(30, "La mangrove reverdit."),
    ];
    let before = serde_json::to_value(&source).unwrap();
    let result = prepare_topic_candidates(&source).unwrap();
    assert_eq!(serde_json::to_value(&source).unwrap(), before);
    assert_eq!(result.without_suggestion, vec![0]);
    let word = result.words.iter().find(|w| w.term == "mangrove").unwrap();
    assert!(word.lexical_weight > 0);
    assert_eq!(word.evidence.len(), 1);
    let e = &word.evidence[0];
    assert_eq!((e.segment_index, e.start_ms, e.end_ms), (1, 30, 930));
    assert_eq!(e.citation, source[1].text);
    assert_eq!(&e.citation[e.byte_start..e.byte_end], "mangrove");
}

#[test]
fn lexical_returns_are_discontinuous_and_multiple_without_transitive_merge() {
    let source = vec![
        segment(0, "Projet Nébuleuse : budget et orchidée."),
        segment(1000, "Projet Sillage : budget et calendrier."),
        segment(2000, "Orchidée et calendrier sont évoqués."),
        segment(3000, "Projet Nébuleuse : orchidée et trajectoire."),
    ];
    let result = prepare_topic_candidates(&source).unwrap();
    let orchid = result
        .links
        .iter()
        .find(|l| l.shared_term == "orchidée")
        .unwrap();
    let indices: Vec<_> = orchid.evidence.iter().map(|e| e.segment_index).collect();
    assert_eq!(indices, vec![0, 2, 3]);
    assert_eq!(orchid.evidence[0].citation, source[0].text);
    assert_eq!(orchid.evidence[2].citation, source[3].text);
    let budget = result
        .links
        .iter()
        .find(|l| l.shared_term == "budget")
        .unwrap();
    assert_eq!(
        budget
            .evidence
            .iter()
            .map(|e| e.segment_index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(result
        .words
        .iter()
        .any(|w| w.term == "trajectoire" && w.evidence.len() == 1));
    assert!(result.without_suggestion.is_empty());
}

#[test]
fn only_explicit_affirmative_folder_cues_propose_names() {
    let source = vec![
        segment(0, "Projet Nébuleuse : budget."),
        segment(1000, "Projet Sillage : budget et calendrier."),
        segment(2000, "Paris discute du budget du projet."),
        segment(3000, "Ce n'est pas le projet Orion, juste une hypothèse."),
        segment(4000, "Le dossier R-42 est évoqué."),
    ];
    let result = prepare_topic_candidates(&source).unwrap();
    let names: Vec<_> = result
        .possible_folders
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(names, vec!["Nébuleuse", "R-42", "Sillage"]);
    for folder in &result.possible_folders {
        for e in &folder.evidence {
            assert_eq!(e.citation, source[e.segment_index].text);
            assert_eq!(&e.citation[e.byte_start..e.byte_end], folder.name);
        }
    }
}

#[test]
fn short_folder_names_are_suggestions_even_without_characteristic_words() {
    let source = vec![
        segment(0, "Projet ABC"),
        segment(1000, "Dossier UI"),
        segment(2000, "Projet X1"),
        segment(3000, "projet abc"),
        segment(4000, "Ce n'est pas le projet ABC"),
    ];
    let result = prepare_topic_candidates(&source).unwrap();
    assert!(result.words.is_empty());
    let names: Vec<_> = result
        .possible_folders
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(names, vec!["ABC", "UI", "X1"]);
    assert_eq!(result.without_suggestion, vec![3, 4]);
    for folder in &result.possible_folders {
        for evidence in &folder.evidence {
            assert!(!result.without_suggestion.contains(&evidence.segment_index));
            assert_eq!(evidence.citation, source[evidence.segment_index].text);
            assert_eq!(
                &evidence.citation[evidence.byte_start..evidence.byte_end],
                folder.name
            );
        }
    }
}

#[test]
fn unicode_composites_elisions_and_identifiers_keep_exact_spans() {
    let source = vec![
        segment(
            0,
            "L’orchidée, l'arc-en-ciel et le cafe\u{301} côtoient ZX-42.",
        ),
        segment(1000, "L’orchidée et ZX-43, sans confusion avec ZX-42."),
    ];
    let result = prepare_topic_candidates(&source).unwrap();
    for term in ["orchidée", "arc-en-ciel", "cafe\u{301}", "ZX-42", "ZX-43"] {
        let word = result.words.iter().find(|w| w.term == term).unwrap();
        for e in &word.evidence {
            assert_eq!(e.citation, source[e.segment_index].text);
            assert_eq!(&e.citation[e.byte_start..e.byte_end], term);
        }
    }
    assert_eq!(
        result
            .links
            .iter()
            .find(|l| l.shared_term == "ZX-42")
            .unwrap()
            .evidence
            .len(),
        2
    );
    assert!(result.links.iter().all(|l| l.shared_term != "ZX-43"));
}

#[test]
fn stable_serializable_and_no_silent_truncation() {
    let source: Vec<_> = (0..900)
        .map(|i| segment(i * 1000, &format!("Frelon-{i} et repère commun")))
        .collect();
    let first = prepare_topic_candidates(&source).unwrap();
    assert_eq!(first, prepare_topic_candidates(&source).unwrap());
    let decoded: parole_core::topic_candidates::TopicCandidates =
        serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
    assert_eq!(decoded, first);
    assert_eq!(first.schema_version, 1);
    assert_eq!(
        first
            .words
            .iter()
            .find(|w| w.term == "Frelon-899")
            .unwrap()
            .evidence[0]
            .segment_index,
        899
    );
    let too_long = vec![segment(0, &"x".repeat(4097))];
    assert!(prepare_topic_candidates(&too_long).is_err());
    let too_many = vec![segment(0, "merci"); 2001];
    assert!(prepare_topic_candidates(&too_many).is_err());
    assert!(prepare_topic_candidates(&[segment(0, &"motlong ".repeat(257))]).is_err());
    for item in &first.words {
        for e in &item.evidence {
            assert_eq!(
                (e.start_ms, e.end_ms),
                (
                    source[e.segment_index].start_ms,
                    source[e.segment_index].end_ms
                )
            );
            assert_eq!(e.citation, source[e.segment_index].text);
            assert!(e.citation.get(e.byte_start..e.byte_end).is_some());
        }
    }
}

fn segment(start: u64, text: &str) -> Segment {
    Segment::new(start, start + 900, text.to_owned())
}
