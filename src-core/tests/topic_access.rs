use parole_core::{topic_access::TopicLibrary, Job, Segment};
use std::{fs, path::Path};
const ID: &str = "11111111-1111-4111-8111-111111111111";
const OTHER: &str = "22222222-2222-4222-8222-222222222222";
fn job() -> Job {
    let mut job = Job::new("fiction.wav".into(), 3_000, 3_000);
    let mut first = Segment::new(0, 1_000, "Le dossier Atlas a un budget.".into());
    first.speaker_id = Some("voix-1".into());
    job.segments = vec![
        first,
        Segment::new(1_000, 2_000, "Le budget est révisé.".into()),
    ];
    job.speaker_names
        .insert("voix-1".into(), "Nom affiché".into());
    job
}
fn save(root: &Path, id: &str, job: &Job) {
    fs::create_dir(root.join(id)).unwrap();
    fs::write(
        root.join(id).join("travail.json"),
        serde_json::to_vec(job).unwrap(),
    )
    .unwrap();
}
#[test]
fn lecture_absente_ne_cree_rien_preparation_et_relecture_ne_reecrivent_rien() {
    let temp = tempfile::tempdir().unwrap();
    save(temp.path(), ID, &job());
    let folder = temp.path().join(ID);
    let audio = folder.join("tranche-00000000.wav");
    let annotation = folder.join("annotations-v1");
    fs::write(&audio, b"fiction-only").unwrap();
    fs::create_dir(&annotation).unwrap();
    fs::write(annotation.join("choix.json"), b"human decision").unwrap();
    let original = fs::read(folder.join("travail.json")).unwrap();
    let library = TopicLibrary::open(temp.path()).unwrap();
    let missing = library.load(ID).unwrap();
    assert_eq!(missing.schema_version, 1);
    assert_eq!(missing.job_id, ID);
    assert!(missing.candidates.is_none());
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 3);
    let prepared = library.prepare(ID).unwrap();
    assert_eq!(prepared.source_revision, missing.source_revision);
    assert!(prepared
        .candidates
        .as_ref()
        .is_some_and(|c| !c.words.is_empty()));
    let cache = folder.join("topics-lexical-v1");
    let path = fs::read_dir(&cache)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let bytes = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let reopened = TopicLibrary::open(temp.path()).unwrap();
    assert_eq!(reopened.load(ID).unwrap(), prepared);
    assert_eq!(reopened.prepare(ID).unwrap(), prepared);
    assert_eq!(fs::read_dir(cache).unwrap().count(), 1);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert_eq!(fs::read(folder.join("travail.json")).unwrap(), original);
    assert_eq!(fs::read(audio).unwrap(), b"fiction-only");
    assert_eq!(
        fs::read(annotation.join("choix.json")).unwrap(),
        b"human decision"
    );
}
#[test]
fn revisions_identites_et_noms_affiches() {
    let temp = tempfile::tempdir().unwrap();
    let original = job();
    save(temp.path(), ID, &original);
    save(temp.path(), OTHER, &original);
    let library = TopicLibrary::open(temp.path()).unwrap();
    let first = library.prepare(ID).unwrap();
    let other = library.load(OTHER).unwrap();
    assert_ne!(first.source_revision, other.source_revision);
    assert!(other.candidates.is_none());
    let changes: Vec<fn(&mut Job)> = vec![
        |j| j.segments[0].text.push('!'),
        |j| j.segments[0].start_ms += 1,
        |j| j.segments[0].end_ms += 1,
        |j| j.segments[0].speaker_id = Some("voix-2".into()),
        |j| j.segments[0].translated_text = Some("Translation".into()),
        |j| j.segments.swap(0, 1),
    ];
    let old_cache = fs::read_dir(temp.path().join(ID).join("topics-lexical-v1"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let old_bytes = fs::read(&old_cache).unwrap();
    for change in changes {
        let mut changed = job();
        change(&mut changed);
        fs::write(
            temp.path().join(ID).join("travail.json"),
            serde_json::to_vec(&changed).unwrap(),
        )
        .unwrap();
        let absent = library.load(ID).unwrap();
        assert_ne!(absent.source_revision, first.source_revision);
        assert!(absent.candidates.is_none());
        assert_eq!(fs::read(&old_cache).unwrap(), old_bytes);
    }
    let mut renamed = original;
    renamed
        .speaker_names
        .insert("voix-1".into(), "Autre nom affiché".into());
    fs::write(
        temp.path().join(ID).join("travail.json"),
        serde_json::to_vec(&renamed).unwrap(),
    )
    .unwrap();
    assert_eq!(library.load(ID).unwrap(), first);
    assert_eq!(library.prepare(ID).unwrap(), first);
    assert_eq!(
        fs::read_dir(temp.path().join(ID).join("topics-lexical-v1"))
            .unwrap()
            .count(),
        1
    );
}
#[test]
fn corruption_ne_se_repare_pas_et_source_trop_grande_ne_cree_rien() {
    let temp = tempfile::tempdir().unwrap();
    save(temp.path(), ID, &job());
    let library = TopicLibrary::open(temp.path()).unwrap();
    library.prepare(ID).unwrap();
    let file = fs::read_dir(temp.path().join(ID).join("topics-lexical-v1"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&file, b"{broken").unwrap();
    for result in [library.load(ID), library.prepare(ID)] {
        assert!(result.is_err());
    }
    assert_eq!(fs::read(file).unwrap(), b"{broken");
    let mut large = job();
    large.segments[0].text = "x".repeat(4_097);
    save(temp.path(), OTHER, &large);
    assert!(library.load(OTHER).is_err());
    assert!(library.prepare(OTHER).is_err());
    assert!(!temp.path().join(OTHER).join("topics-lexical-v1").exists());
    let mut too_many = job();
    too_many.segments = vec![Segment::new(0, 1, "fictif".into()); 2_001];
    fs::write(
        temp.path().join(OTHER).join("travail.json"),
        serde_json::to_vec(&too_many).unwrap(),
    )
    .unwrap();
    assert!(library.load(OTHER).is_err());
    assert!(library.prepare(OTHER).is_err());
    assert!(!temp.path().join(OTHER).join("topics-lexical-v1").exists());
    let huge = temp.path().join(OTHER).join("travail.json");
    fs::OpenOptions::new()
        .write(true)
        .open(&huge)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(library.load(OTHER).is_err());
}
#[test]
fn rejette_identite_et_etat_invalides_avant_ecriture() {
    let temp = tempfile::tempdir().unwrap();
    let library = TopicLibrary::open(temp.path()).unwrap();
    for id in [
        "",
        "../secret",
        "11111111-1111-4111-8111-11111111111A",
        "11111111-1111-4111-8111-111111111111/x",
    ] {
        assert!(library.load(id).is_err(), "{id}");
        assert!(library.prepare(id).is_err(), "{id}");
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    fs::create_dir(temp.path().join(ID)).unwrap();
    fs::write(temp.path().join(ID).join("travail.json"), b"invalid json").unwrap();
    assert!(library.load(ID).is_err());
    assert!(library.prepare(ID).is_err());
    assert_eq!(fs::read_dir(temp.path().join(ID)).unwrap().count(), 1);
}
#[cfg(unix)]
#[test]
fn rejette_liens_de_dossier_et_fichier_et_hardlinks() {
    use std::os::unix::fs::symlink;
    for mode in ["folder", "file", "hardlink", "cache"] {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("source");
        save(temp.path(), "33333333-3333-4333-8333-333333333333", &job());
        fs::write(&target, serde_json::to_vec(&job()).unwrap()).unwrap();
        let folder = temp.path().join(ID);
        match mode {
            "folder" => symlink(
                temp.path().join("33333333-3333-4333-8333-333333333333"),
                &folder,
            )
            .unwrap(),
            "file" | "hardlink" => {
                fs::create_dir(&folder).unwrap();
                if mode == "file" {
                    symlink(&target, folder.join("travail.json")).unwrap();
                } else {
                    fs::hard_link(&target, folder.join("travail.json")).unwrap();
                }
            }
            "cache" => {
                save(temp.path(), ID, &job());
                symlink(
                    temp.path().join("33333333-3333-4333-8333-333333333333"),
                    folder.join("topics-lexical-v1"),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let library = TopicLibrary::open(temp.path()).unwrap();
        assert!(library.load(ID).is_err(), "{mode}");
        assert!(library.prepare(ID).is_err(), "{mode}");
    }
}
