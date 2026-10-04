use parole_core::{topic_candidates::prepare_topic_candidates, Segment};
use serde_json::json;

#[test]
fn real_producer_matches_the_versioned_ui_fixture() {
    let lines = [
        "Projet AB.",
        "Orchidée orchidée et budget 😀 cafe\u{301}.",
        "Budget dans le dossier Nébuleuse.",
        "Bonjour et merci.",
        "L'orchidée revient.",
        "😀 L’orchidée et cafe\u{301}.",
        "Projet AB, budget.",
    ];
    let segments: Vec<Segment> = lines
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let mut s = Segment::new(i as u64 * 1000, (i as u64 + 1) * 1000, (*text).to_owned());
            s.speaker_id = Some(if i % 2 == 0 { "A" } else { "B" }.to_owned());
            s
        })
        .collect();
    let candidates = prepare_topic_candidates(&segments).expect("fiction within bounds");
    assert!(candidates.possible_folders.iter().any(|f| f.name == "AB"));
    assert!(candidates.without_suggestion.contains(&3));
    let empty = prepare_topic_candidates(&[]).expect("empty source supported");
    let output = json!({ "scope": "fiction-transport-v1", "segments": segments, "speaker_names": {"A": "Alice", "B": "Benoît"}, "candidates": candidates, "empty_candidates": empty });
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../ui/tests/fixtures/topicCandidates.generated.json"
    ))
    .expect("versioned producer fixture");
    assert_eq!(output, expected, "Rust producer and UI fixture must agree");
    if let Ok(path) = std::env::var("PAROLE_TOPIC_TRANSPORT_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&output).unwrap()).unwrap();
    }
}
