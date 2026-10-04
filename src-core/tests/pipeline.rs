use parole_core::{
    format_timestamp, process_chunks, recover_interrupted, render_srt, render_txt, render_vtt, Job,
    Segment, Stage,
};
use std::fs;

#[test]
fn reprise_saute_les_tranches_deja_terminees() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("travail.json");
    let mut job = Job::new("enregistrement.wav".into(), 125_000, 60_000);
    let mut appels = Vec::new();
    let interrupted = process_chunks(&mut job, &path, |n, _start, _duration| {
        appels.push(n);
        if n == 1 {
            return Err("interrompu".into());
        }
        Ok(vec![Segment::new(1_000, 2_000, "Bonjour".into())])
    });
    assert!(interrupted.is_err());
    assert_eq!(appels, vec![0, 1]);
    let disk: Job = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(disk.completed_chunks, 1);
    assert_eq!(disk.stage, Stage::Interrupted);
    job = disk;
    appels.clear();
    process_chunks(&mut job, &path, |n, _, _| {
        appels.push(n);
        Ok(vec![Segment::new(1_000, 2_000, "Suite".into())])
    })
    .unwrap();
    assert_eq!(appels, vec![1, 2]);
    assert_eq!(job.segments.len(), 3);
    assert_eq!(job.segments[1].start_ms, 61_000);
    assert_eq!(job.segments[2].start_ms, 121_000);
    assert_eq!(job.stage, Stage::Transcribed);
}

#[test]
fn mesure_locale_survit_a_la_reprise_sans_dupliquer_les_tranches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("travail.json");
    let mut job = Job::new("long.wav".into(), 65_000, 30_000);
    job.timing.active_since_ms = parole_core::epoch_ms().saturating_sub(2_000);
    let _ = process_chunks(&mut job, &path, |index, _, _| {
        if index == 1 {
            return Err("arrêt".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
        Ok(vec![])
    });
    let mut resumed: Job = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(resumed.timing.active_chunk_since_ms, 0);
    assert!(resumed.timing.preparation_ms >= 1_800);
    assert_eq!(resumed.timing.platform, std::env::consts::OS);
    assert_eq!(resumed.timing.chunk_ms.len(), 1);
    assert!(resumed.timing.chunk_ms[0] > 0);
    process_chunks(&mut resumed, &path, |_, _, _| Ok(vec![])).unwrap();
    assert_eq!(resumed.timing.chunk_ms.len(), 3);
    assert_eq!(
        resumed.timing.transcription_ms,
        resumed.timing.chunk_ms.iter().sum::<u64>()
    );
    let saved: Job = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        saved.timing.transcription_ms,
        resumed.timing.transcription_ms
    );
}

#[test]
fn ouverture_apres_arret_renvoie_une_tache_reprenables() {
    let path = tempfile::tempdir().unwrap();
    let state = path.path().join("travail.json");
    let mut job = Job::new("a.wav".into(), 90_000, 30_000);
    job.stage = Stage::Transcribing;
    job.completed_chunks = 2;
    assert!(recover_interrupted(&mut job, &state).unwrap());
    assert_eq!(job.stage, Stage::Interrupted);
    let from_disk: Job = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    assert_eq!(from_disk.completed_chunks, 2);
    assert!(!recover_interrupted(&mut job, &state).unwrap());
}

#[test]
fn longue_duree_ne_charge_que_la_tranche_restante() {
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join("travail.json");
    let mut job = Job::new("long.mp4".into(), 12 * 3_600_000 + 10_001, 30_000);
    let last = job.chunks() - 1;
    job.completed_chunks = last;
    let mut calls = 0;
    process_chunks(&mut job, &state, |index, start, duration| {
        calls += 1;
        assert_eq!(index, last);
        assert_eq!(start, last as u64 * 30_000);
        assert_eq!(duration, 10_001);
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(job.stage, Stage::Transcribed);
}

#[test]
fn sous_titres_preservent_temps_et_nom() {
    let mut job = Job::new("reunion.wav".into(), 4_000, 30_000);
    let mut s = Segment::new(1_234, 2_456, "Bonjour".into());
    s.speaker_id = Some("speaker-1".into());
    job.speaker_names.insert("speaker-1".into(), "Marie".into());
    job.segments.push(s);
    assert!(render_srt(&job).contains("00:00:01,234 --> 00:00:02,456\nMarie : Bonjour"));
    assert!(render_vtt(&job).contains("00:00:01.234 --> 00:00:02.456\nMarie : Bonjour"));
}

#[test]
fn export_utilise_le_nouveau_nom_pour_toutes_les_occurrences() {
    let mut job = Job::new("a.wav".into(), 10_000, 60_000);
    let mut first = Segment::new(0, 1000, "Oui".into());
    first.speaker_id = Some("speaker-1".into());
    let mut second = Segment::new(2000, 3000, "Non".into());
    second.speaker_id = Some("speaker-1".into());
    job.segments = vec![first, second];
    job.speaker_names.insert("speaker-1".into(), "Marie".into());
    let txt = render_txt(&job);
    assert_eq!(txt.matches("Marie").count(), 2);
    assert_eq!(format_timestamp(3_661_234), "01:01:01,234");
}

#[test]
fn reprise_pendant_traduction_et_compte_rendu() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("travail.json");
    let mut job = Job::new("r.wav".into(), 30_000, 30_000);
    job.stage = Stage::Translating;
    job.target_language = Some("fr".into());
    assert!(recover_interrupted(&mut job, &path).unwrap());
    job.stage = Stage::Reporting;
    assert!(recover_interrupted(&mut job, &path).unwrap());
}

#[test]
fn export_complet_contient_les_trois_resultats() {
    let mut job = Job::new("r.wav".into(), 2_000, 30_000);
    let mut segment = Segment::new(0, 1_000, "Bonjour".into());
    segment.translated_text = Some("Hello".into());
    job.segments.push(segment);
    job.target_language = Some("en".into());
    job.report = Some("## Summary\n\n> [00:00] Bonjour".into());
    let text = parole_core::render_complete_txt(&job);
    let md = parole_core::render_complete_markdown(&job);
    for result in [&text, &md] {
        for expected in ["Bonjour", "Hello", "Summary"] {
            assert!(result.contains(expected));
        }
    }
}
