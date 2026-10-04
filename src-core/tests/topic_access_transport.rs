use parole_core::{topic_access::TopicLibrary, Job, Segment};
use serde_json::{json, Value};
use std::fs;
const ID: &str = "44444444-4444-4444-8444-444444444444";

#[test]
fn transport_est_la_sortie_reelle_du_moteur() {
    let temp = tempfile::tempdir().unwrap();
    let mut job = Job::new("fiction-unicode.wav".into(), 4_000, 4_000);
    let mut composed = Segment::new(0, 1_000, "Le dossier Étoile étudie le budget 🌟.".into());
    composed.speaker_id = Some("voix-é".into());
    composed.translated_text = Some("The star budget 🌟".into());
    let mut decomposed = Segment::new(
        1_000,
        2_000,
        "Le dossier E\u{301}toile reprend le budget.".into(),
    );
    decomposed.speaker_id = Some("voix-é".into());
    job.segments = vec![
        composed,
        decomposed,
        Segment::new(2_000, 3_000, "oui 🌿".into()),
    ];
    job.speaker_names
        .insert("voix-é".into(), "Prénom affiché".into());
    let folder = temp.path().join(ID);
    fs::create_dir(&folder).unwrap();
    fs::write(
        folder.join("travail.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    let library = TopicLibrary::open(temp.path()).unwrap();
    let missing = library.load(ID).unwrap();
    assert!(missing.candidates.is_none());
    let prepared = library.prepare(ID).unwrap();
    drop(library);
    let loaded = TopicLibrary::open(temp.path()).unwrap().load(ID).unwrap();
    assert_eq!(prepared, loaded);
    let actual = json!({
        "job_id": ID,
        "segments": job.segments,
        "missing_snapshot": missing,
        "prepared_snapshot": prepared,
        "loaded_snapshot": loaded,
    });
    // Export uniquement sur demande explicite en test, depuis le vrai chemin de production.
    if let Some(path) = std::env::var_os("PAROLE_EXPORT_TOPIC_SNAPSHOT") {
        fs::write(path, serde_json::to_vec_pretty(&actual).unwrap()).unwrap();
    }
    let fixture: Value = serde_json::from_str(include_str!(
        "../../ui/tests/fixtures/topicSnapshot.generated.json"
    ))
    .unwrap();
    assert_eq!(actual, fixture);
}
