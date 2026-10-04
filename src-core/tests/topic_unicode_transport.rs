use parole_core::{topic_access::TopicLibrary, Job, Segment};
use serde_json::json;
use std::fs;
const ID: &str = "66666666-6666-4666-8666-666666666666";
#[test]
fn transport_unicode_est_la_sortie_reelle_du_moteur() {
    let temp = tempfile::tempdir().unwrap();
    let bmp = "AＡＢＣＤ";
    let supplementary = "A𝐀𝐁𝐂𝐃";
    let mut job = Job::new("fiction-ordre-unicode.wav".into(), 3_000, 3_000);
    job.segments = vec![
        Segment::new(
            0,
            1_000,
            format!("Le dossier {bmp} compare {bmp} et {supplementary}."),
        ),
        Segment::new(
            1_000,
            2_000,
            format!("Le dossier {supplementary} compare {bmp} et {supplementary}."),
        ),
        Segment::new(2_000, 3_000, "oui 🌿".into()),
    ];
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
    let candidates = loaded.candidates.as_ref().unwrap();
    assert_eq!(
        candidates
            .possible_folders
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec![bmp, supplementary]
    );
    assert_eq!(candidates.without_suggestion, vec![2]);
    assert!(candidates.links.len() >= 2);
    let actual = json!({ "job_id": ID, "segments": job.segments, "missing_snapshot": missing, "prepared_snapshot": prepared, "loaded_snapshot": loaded });
    if let Some(path) = std::env::var_os("PAROLE_EXPORT_UNICODE_TOPIC_SNAPSHOT") {
        fs::write(path, serde_json::to_vec_pretty(&actual).unwrap()).unwrap();
    }
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../ui/tests/fixtures/topicSnapshot.unicode.generated.json"
    ))
    .unwrap();
    assert_eq!(actual, fixture);
}
