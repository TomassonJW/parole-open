use parole_core::{topic_access::TopicLibrary, Job, Segment};
use serde_json::{json, Value};
use std::fs;
const ID: &str = "11111111-1111-4111-8111-111111111111";

#[test]
fn lecteur_et_pistes_partagent_une_source_reelle() {
    let temp = tempfile::tempdir().unwrap();
    let mut job = Job::new("fiction-audio.wav".into(), 4_000, 1_000);
    job.segments = vec![Segment::new(100, 800, "premier".into())];
    for i in 0..100 {
        job.segments
            .push(Segment::new(800 + i * 5, 805 + i * 5, "interlude".into()));
    }
    job.segments
        .push(Segment::new(3_250, 3_800, "second".into()));
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
    let actual = json!({"job_id": ID, "segments": job.segments, "missing_snapshot": missing, "prepared_snapshot": prepared, "loaded_snapshot": loaded});
    if let Some(path) = std::env::var_os("PAROLE_EXPORT_TOPIC_READER") {
        fs::write(path, serde_json::to_vec_pretty(&actual).unwrap()).unwrap();
    }
    let fixture: Value = serde_json::from_str(include_str!(
        "../../ui/tests/fixtures/topicReader.generated.json"
    ))
    .unwrap();
    assert_eq!(actual, fixture);
}
