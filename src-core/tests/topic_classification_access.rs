use parole_core::{
    topic_classification_access::{ClassificationLibrary, ClassificationRequest},
    topic_classification_cache::{ClassificationResponse, EngineIdentity},
    topic_questions::CandidateChoice,
    topic_questions::PreparedTopicQuestions,
    topic_selection::{CandidateScore, SelectionThresholds},
    Job, Segment,
};
use std::{
    fs,
    path::Path,
    time::{Duration, UNIX_EPOCH},
};
const ID: &str = "abababab-abab-4bab-8bab-abababababab";
fn job() -> Job {
    let mut j = Job::new("fictif.wav".into(), 3_000, 3_000);
    j.segments = vec![
        Segment::new(0, 1_000, "Le projet Atlas prépare le budget.".into()),
        Segment::new(
            1_000,
            2_000,
            "Le calendrier et le budget restent à discuter.".into(),
        ),
        Segment::new(
            2_000,
            3_000,
            "Le dossier Luciole conserve les illustrations.".into(),
        ),
    ];
    j.segments[1].speaker_id = Some("voix-1".into());
    j
}
fn install(root: &Path, j: &Job) {
    fs::create_dir(root.join(ID)).unwrap();
    rewrite(root, j);
}
fn rewrite(root: &Path, j: &Job) {
    fs::write(
        root.join(ID).join("travail.json"),
        serde_json::to_vec(j).unwrap(),
    )
    .unwrap();
}
fn request(j: &Job) -> ClassificationRequest {
    ClassificationRequest {
        job_id: ID.into(),
        source_revision: PreparedTopicQuestions::prepare(ID, &j.segments)
            .unwrap()
            .question(1, &[])
            .unwrap()
            .source_revision,
        target: 1,
        choices: vec![
            CandidateChoice::Word {
                term: "budget".into(),
            },
            CandidateChoice::Word {
                term: "calendrier".into(),
            },
        ],
        engine: EngineIdentity {
            model_id: "fixture/modele-fictif".into(),
            model_revision: "revision-fictive-v1".into(),
            weights_sha256: "1".repeat(64),
            tokenizer_sha256: "2".repeat(64),
            runtime_sha256: "3".repeat(64),
            engine_sha256: "4".repeat(64),
            options_sha256: "5".repeat(64),
        },
        thresholds: SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: 0.9,
        },
    }
}
fn response(
    plan: &parole_core::topic_classification_cache::ClassificationPlan,
) -> ClassificationResponse {
    ClassificationResponse {
        question_revision: plan.question().request_revision.clone(),
        engine_revision: plan.engine_revision().into(),
        scores: plan
            .question()
            .candidates
            .iter()
            .enumerate()
            .map(|(i, c)| CandidateScore {
                candidate_id: c.candidate_id.clone(),
                score: if i == 0 { 0.95 } else { 0.7 },
            })
            .collect(),
    }
}
fn only_record(root: &Path) -> std::path::PathBuf {
    let entries: Vec<_> = fs::read_dir(root.join(ID).join("topics-classified-v1"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1);
    entries[0].clone()
}
#[test]
fn identical_words_in_two_jobs_do_not_share_a_classification() {
    const OTHER: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    let t = tempfile::tempdir().unwrap();
    let j = job();
    install(t.path(), &j);
    fs::create_dir(t.path().join(OTHER)).unwrap();
    fs::write(
        t.path().join(OTHER).join("travail.json"),
        serde_json::to_vec(&j).unwrap(),
    )
    .unwrap();
    let library = ClassificationLibrary::open(t.path()).unwrap();
    let req = request(&j);
    let work = library.prepare(&req).unwrap();
    library.save(&work, &response(work.plan())).unwrap();
    let mut other = request(&j);
    other.job_id = OTHER.into();
    assert!(library.load(&other).is_err());
    other.source_revision = PreparedTopicQuestions::prepare(OTHER, &j.segments)
        .unwrap()
        .question(1, &[])
        .unwrap()
        .source_revision;
    assert!(library.load(&other).unwrap().record.is_none());
    let other_work = library.prepare(&other).unwrap();
    assert_ne!(work.plan().cache_key(), other_work.plan().cache_key());
    assert!(library.save(&other_work, &response(work.plan())).is_err());
    assert!(!t.path().join(OTHER).join("topics-classified-v1").exists());
    let original_path = only_record(t.path());
    let original_bytes = fs::read(&original_path).unwrap();
    let saved = library
        .save(&other_work, &response(other_work.plan()))
        .unwrap();
    assert_eq!(saved.job_id, OTHER);
    assert_eq!(
        fs::read_dir(t.path().join(OTHER).join("topics-classified-v1"))
            .unwrap()
            .count(),
        1
    );
    assert!(library.load(&other).unwrap().record.is_some());
    assert_eq!(fs::read(original_path).unwrap(), original_bytes);
}
#[test]
fn later_request_mutations_do_not_change_the_sealed_work() {
    let t = tempfile::tempdir().unwrap();
    let j = job();
    install(t.path(), &j);
    let library = ClassificationLibrary::open(t.path()).unwrap();
    let mut req = request(&j);
    let work = library.prepare(&req).unwrap();
    let question = serde_json::to_value(work.plan().question()).unwrap();
    req.choices.reverse();
    req.target = 0;
    req.engine.model_revision.push('2');
    req.source_revision.clear();
    assert_eq!(
        serde_json::to_value(
            work.prepared_questions()
                .unwrap()
                .question(work.target(), work.choices())
                .unwrap()
        )
        .unwrap(),
        question
    );
    library.save(&work, &response(work.plan())).unwrap();
    assert!(library.load(&request(&j)).unwrap().record.is_some());
}
#[test]
fn missing_result_and_preparation_do_not_create_a_cache() {
    let t = tempfile::tempdir().unwrap();
    let j = job();
    install(t.path(), &j);
    let library = ClassificationLibrary::open(t.path()).unwrap();
    let mut req = request(&j);
    let lexical = parole_core::topic_access::TopicLibrary::open(t.path())
        .unwrap()
        .load(ID)
        .unwrap();
    req.source_revision = lexical.source_revision;
    assert!(library.load(&req).unwrap().record.is_none());
    library.prepare(&req).unwrap();
    assert_eq!(fs::read_dir(t.path().join(ID)).unwrap().count(), 1);
}
#[test]
fn engine_order_target_thresholds_invalidate_without_replacing_old_results() {
    let t = tempfile::tempdir().unwrap();
    let j = job();
    install(t.path(), &j);
    let library = ClassificationLibrary::open(t.path()).unwrap();
    let req = request(&j);
    let work = library.prepare(&req).unwrap();
    let original = library.save(&work, &response(work.plan())).unwrap();
    assert_eq!(
        original.record.as_ref().unwrap().selection().state,
        parole_core::topic_selection::ReviewState::Ambiguous
    );
    let path = only_record(t.path());
    let bytes = fs::read(&path).unwrap();
    let changes: Vec<fn(&mut ClassificationRequest)> = vec![
        |r| r.choices.reverse(),
        |r| r.choices.pop().map(drop).unwrap(),
        |r| r.target = 0,
        |r| r.engine.model_revision.push('2'),
        |r| r.engine.weights_sha256 = "a".repeat(64),
        |r| r.engine.tokenizer_sha256 = "b".repeat(64),
        |r| r.engine.runtime_sha256 = "c".repeat(64),
        |r| r.engine.engine_sha256 = "d".repeat(64),
        |r| r.engine.options_sha256 = "e".repeat(64),
        |r| r.thresholds.uncertain_from = 0.4,
        |r| r.thresholds.proposed_from = 0.95,
    ];
    for change in changes {
        let mut different = request(&j);
        change(&mut different);
        assert!(library.load(&different).unwrap().record.is_none());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    let mut renamed = j.clone();
    renamed
        .speaker_names
        .insert("voix-1".into(), "Nom humain fictif".into());
    rewrite(t.path(), &renamed);
    assert_eq!(
        serde_json::to_value(library.load(&req).unwrap()).unwrap(),
        serde_json::to_value(original).unwrap()
    );
}
#[test]
fn empty_candidates_are_stored_without_any_model_capability() {
    let t = tempfile::tempdir().unwrap();
    let j = job();
    install(t.path(), &j);
    let library = ClassificationLibrary::open(t.path()).unwrap();
    let mut req = request(&j);
    req.choices.clear();
    let work = library.prepare(&req).unwrap();
    assert!(work.plan().question().model_input.is_none());
    let stored = library.save(&work, &response(work.plan())).unwrap();
    let selection = stored.record.unwrap();
    assert_eq!(
        selection.selection().state,
        parole_core::topic_selection::ReviewState::NoCandidates
    );
    assert!(selection.selection().assessments.is_empty());
    assert!(ClassificationLibrary::open(t.path())
        .unwrap()
        .load(&req)
        .unwrap()
        .record
        .is_some());
}
#[test]
fn invalid_requests_and_malformed_responses_never_publish() {
    let t = tempfile::tempdir().unwrap();
    let j = job();
    install(t.path(), &j);
    let library = ClassificationLibrary::open(t.path()).unwrap();
    let changes: Vec<fn(&mut ClassificationRequest)> = vec![
        |r| r.job_id = "../outside".into(),
        |r| r.job_id = "invalid".into(),
        |r| r.target = usize::MAX,
        |r| r.source_revision.clear(),
        |r| r.choices.push(r.choices[0].clone()),
        |r| {
            r.choices[0] = CandidateChoice::Word {
                term: "inexistant".into(),
            }
        },
        |r| r.engine.weights_sha256.clear(),
        |r| r.thresholds.proposed_from = f64::NAN,
    ];
    for change in changes {
        let mut bad = request(&j);
        change(&mut bad);
        assert!(library.prepare(&bad).is_err());
        assert!(library.load(&bad).is_err());
    }
    let work = library.prepare(&request(&j)).unwrap();
    let changes: Vec<fn(&mut ClassificationResponse)> = vec![
        |r| r.question_revision = "0".repeat(64),
        |r| r.engine_revision = "f".repeat(64),
        |r| r.scores[0].score = f64::NAN,
        |r| r.scores.reverse(),
        |r| {
            r.scores.pop();
        },
    ];
    for change in changes {
        let mut bad = response(work.plan());
        change(&mut bad);
        assert!(library.save(&work, &bad).is_err());
    }
    assert!(!t.path().join(ID).join("topics-classified-v1").exists());
}
#[test]
fn corruption_is_not_repaired_by_read_or_save() {
    let t = tempfile::tempdir().unwrap();
    let j = job();
    install(t.path(), &j);
    let library = ClassificationLibrary::open(t.path()).unwrap();
    let req = request(&j);
    let work = library.prepare(&req).unwrap();
    library.save(&work, &response(work.plan())).unwrap();
    let path = only_record(t.path());
    fs::write(&path, b"{casse").unwrap();
    assert!(library.load(&req).is_err());
    assert!(library.save(&work, &response(work.plan())).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{casse");
    fs::write(t.path().join(ID).join("travail.json"), b"{invalide").unwrap();
    assert!(library.load(&req).is_err());
    assert!(library.save(&work, &response(work.plan())).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{casse");
}
#[test]
fn oversized_source_is_refused_before_any_publication() {
    let t = tempfile::tempdir().unwrap();
    let j = job();
    install(t.path(), &j);
    let library = ClassificationLibrary::open(t.path()).unwrap();
    let req = request(&j);
    let work = library.prepare(&req).unwrap();
    let path = t.path().join(ID).join("travail.json");
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(parole_core::audio_access::MAX_STATE_BYTES as u64 + 1)
        .unwrap();
    assert!(library.prepare(&req).is_err());
    assert!(library.load(&req).is_err());
    assert!(library.save(&work, &response(work.plan())).is_err());
    let mut large = j.clone();
    large.segments[1].text = "x".repeat(4097);
    rewrite(t.path(), &large);
    assert!(library.load(&req).is_err());
    assert!(library.save(&work, &response(work.plan())).is_err());
    assert!(!t.path().join(ID).join("topics-classified-v1").exists());
}
#[test]
#[cfg(unix)]
fn symlinked_job_source_or_cache_and_multilink_source_are_rejected() {
    use std::os::unix::fs::symlink;
    for kind in 0..4 {
        let t = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let j = job();
        install(t.path(), &j);
        let library = ClassificationLibrary::open(t.path()).unwrap();
        let req = request(&j);
        let work = library.prepare(&req).unwrap();
        match kind {
            0 => {
                fs::rename(t.path().join(ID), outside.path().join(ID)).unwrap();
                symlink(outside.path().join(ID), t.path().join(ID)).unwrap();
            }
            1 => {
                let path = t.path().join(ID).join("travail.json");
                fs::rename(&path, outside.path().join("travail.json")).unwrap();
                symlink(outside.path().join("travail.json"), &path).unwrap();
            }
            2 => {
                symlink(
                    outside.path(),
                    t.path().join(ID).join("topics-classified-v1"),
                )
                .unwrap();
            }
            _ => {
                fs::hard_link(
                    t.path().join(ID).join("travail.json"),
                    outside.path().join("copie.json"),
                )
                .unwrap();
            }
        }
        assert!(library.load(&req).is_err());
        assert!(library.save(&work, &response(work.plan())).is_err());
        let expected = if kind == 2 { 0 } else { 1 };
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), expected);
    }
}
#[test]
fn tasks_cannot_cross_library_instances_or_roots() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let j = job();
    install(first.path(), &j);
    install(second.path(), &j);
    let a = ClassificationLibrary::open(first.path()).unwrap();
    let b = ClassificationLibrary::open(second.path()).unwrap();
    let other_handle = ClassificationLibrary::open(first.path()).unwrap();
    let work = a.prepare(&request(&j)).unwrap();
    assert!(b.save(&work, &response(work.plan())).is_err());
    assert!(other_handle.save(&work, &response(work.plan())).is_err());
    assert!(!second.path().join(ID).join("topics-classified-v1").exists());
    assert!(!first.path().join(ID).join("topics-classified-v1").exists());
    a.save(&work, &response(work.plan())).unwrap();
    assert!(other_handle.load(&request(&j)).unwrap().record.is_some());
}
#[test]
fn roots_are_absolute_without_parent_traversal() {
    assert!(ClassificationLibrary::open(Path::new(".")).is_err());
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("child")).unwrap();
    assert!(ClassificationLibrary::open(&temp.path().join("child").join("..")).is_err());
}
#[test]
fn stale_source_is_rejected_before_consultation_or_persistence() {
    let temp = tempfile::tempdir().unwrap();
    let original = job();
    install(temp.path(), &original);
    let library = ClassificationLibrary::open(temp.path()).unwrap();
    let req = request(&original);
    let changes: Vec<fn(&mut Job)> = vec![
        |j| j.segments[0].text.push('!'),
        |j| j.segments[0].start_ms += 1,
        |j| j.segments[0].end_ms += 1,
        |j| j.segments[0].speaker_id = Some("voix-2".into()),
        |j| j.segments[0].translated_text = Some("traduction fictive".into()),
        |j| j.segments.swap(0, 2),
        |j| {
            j.segments
                .push(Segment::new(4_000, 5_000, "Suite hors voisinage".into()))
        },
    ];
    for change in changes {
        rewrite(temp.path(), &original);
        let work = library.prepare(&req).unwrap();
        let mut changed = original.clone();
        change(&mut changed);
        rewrite(temp.path(), &changed);
        assert!(
            library.prepare(&req).is_err(),
            "old source accepted for preparation"
        );
        assert!(
            library.load(&req).is_err(),
            "old source accepted for consultation"
        );
        assert!(
            library.save(&work, &response(work.plan())).is_err(),
            "late result accepted after source changed"
        );
        assert!(!temp.path().join(ID).join("topics-classified-v1").exists());
    }
    rewrite(temp.path(), &original);
    let mut wrong = request(&original);
    wrong.source_revision = "0".repeat(64);
    assert!(library.prepare(&wrong).is_err());
    let work = library.prepare(&req).unwrap();
    library.save(&work, &response(work.plan())).unwrap();
    let path = only_record(temp.path());
    let bytes = fs::read(&path).unwrap();
    let mut changed = original.clone();
    changed.segments[0].text.push('!');
    rewrite(temp.path(), &changed);
    assert!(library.load(&request(&changed)).unwrap().record.is_none());
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn real_job_question_cache_roundtrip_preserves_source_audio_and_human_notes() {
    let temp = tempfile::tempdir().unwrap();
    let j = job();
    install(temp.path(), &j);
    let folder = temp.path().join(ID);
    fs::write(folder.join("audio.wav"), b"audio fictif").unwrap();
    fs::write(folder.join("annotations.json"), b"choix humain fictif").unwrap();
    let before = fs::read(folder.join("travail.json")).unwrap();
    let library = ClassificationLibrary::open(temp.path()).unwrap();
    let req = request(&j);
    let task = library.prepare(&req).unwrap();
    assert_eq!(task.plan().question().target.text, j.segments[1].text);
    let prepared = task.prepared_questions().unwrap();
    assert_eq!(
        serde_json::to_value(prepared.question(task.target(), task.choices()).unwrap()).unwrap(),
        serde_json::to_value(task.plan().question()).unwrap()
    );
    let created = library.save(&task, &response(task.plan())).unwrap();
    assert_eq!(created.schema_version, 1);
    assert_eq!(created.job_id, ID);
    assert_eq!(created.source_revision, req.source_revision);
    assert_eq!(created.target, 1);
    assert_eq!(
        created
            .record
            .as_ref()
            .unwrap()
            .selection()
            .assessments
            .len(),
        2
    );
    let path = only_record(temp.path());
    let bytes = fs::read(&path).unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(60)))
        .unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    library.save(&task, &response(task.plan())).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    let reopened = ClassificationLibrary::open(temp.path()).unwrap();
    assert_eq!(
        serde_json::to_value(reopened.load(&req).unwrap()).unwrap(),
        serde_json::to_value(created).unwrap()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert_eq!(fs::read(folder.join("travail.json")).unwrap(), before);
    assert_eq!(fs::read(folder.join("audio.wav")).unwrap(), b"audio fictif");
    assert_eq!(
        fs::read(folder.join("annotations.json")).unwrap(),
        b"choix humain fictif"
    );
}
