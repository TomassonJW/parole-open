use parole_core::{
    export_format_allowed, finalize_interrupted_job, recover_interrupted, render_complete_txt,
    render_export_json, Job, Segment, Stage,
};

#[test]
fn un_compte_rendu_demande_mais_absent_ne_sort_pas_comme_resultat_complet() {
    let mut job = Job::new("fiction.wav".into(), 30_000, 30_000);
    job.completed_chunks = job.chunks();
    job.stage = Stage::Transcribed;
    job.generate_report = true;
    job.segments.push(Segment::new(0, 1_000, "Bonjour".into()));
    for format in ["json", "srt", "vtt"] {
        assert!(!export_format_allowed(&job, format), "{format}");
    }
    for format in ["txt", "md", "docx"] {
        assert!(export_format_allowed(&job, format), "{format}");
    }
    assert!(render_complete_txt(&job).contains("Résultat incomplet - vérification nécessaire"));
    job.report = Some("Compte rendu".into());
    assert!(export_format_allowed(&job, "json"));
    assert!(!render_complete_txt(&job).contains("Résultat incomplet - vérification nécessaire"));
}

#[test]
fn le_json_partage_omet_le_chemin_prive_sans_modifier_le_travail() {
    let mut job = Job::new("fiction.wav".into(), 30_000, 30_000);
    job.stage = Stage::Transcribed;
    job.source_path = Some("/dossier/prive/reunion-fictive.wav".into());
    job.segments.push(Segment::new(0, 1_000, "Bonjour".into()));
    let json = render_export_json(&job).unwrap();
    let output: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(output["source_path"], serde_json::Value::Null);
    assert_eq!(output["segments"][0]["text"], "Bonjour");
    assert!(!String::from_utf8(json).unwrap().contains("/dossier/prive/"));
    assert_eq!(
        job.source_path.as_deref(),
        Some("/dossier/prive/reunion-fictive.wav")
    );
}

#[test]
fn les_etapes_actives_et_les_alertes_ne_quittent_pas_lapplication_sans_avertissement() {
    let mut job = Job::new("fiction.wav".into(), 30_000, 30_000);
    for stage in [
        Stage::Ready,
        Stage::Transcribing,
        Stage::Translating,
        Stage::Reporting,
    ] {
        job.stage = stage;
        for format in ["txt", "md", "docx", "srt", "vtt", "json"] {
            assert!(!export_format_allowed(&job, format));
        }
    }
    job.stage = Stage::Transcribed;
    job.target_language = Some("en".into());
    job.translation_issues.push(0);
    assert!(!export_format_allowed(&job, "json"));
    assert!(!export_format_allowed(&job, "vtt"));
    assert!(export_format_allowed(&job, "txt"));
    assert!(render_complete_txt(&job).contains("Résultat incomplet - vérification nécessaire"));
}

#[test]
fn une_sortie_douteuse_apres_un_rapport_ne_devient_pas_un_succes_au_redemarrage() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("travail.json");
    let mut job = Job::new("fiction.wav".into(), 30_000, 30_000);
    job.completed_chunks = job.chunks();
    job.target_language = Some("en".into());
    job.generate_report = true;
    job.report = Some("Compte rendu source".into());
    job.stage = Stage::Transcribed;
    job.translation_issues.push(0);
    let mut segment = Segment::new(0, 1_000, "Bonjour".into());
    segment.translated_text = Some("Bonjour".into());
    job.segments.push(segment);
    assert!(recover_interrupted(&mut job, &path).unwrap());
    assert_eq!(job.stage, Stage::Interrupted);
    assert!(!finalize_interrupted_job(&mut job, &path).unwrap());
    let persisted: Job = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(persisted.stage, Stage::Interrupted);
    assert_eq!(persisted.translation_issues, vec![0]);
    assert_eq!(persisted.report.as_deref(), Some("Compte rendu source"));
}

#[test]
fn les_formats_sans_avertissement_sont_refuses_pour_un_travail_interrompu() {
    let mut job = Job::new("fiction.wav".into(), 30_000, 30_000);
    job.stage = Stage::Interrupted;
    for format in ["txt", "md", "docx"] {
        assert!(export_format_allowed(&job, format));
    }
    for format in ["json", "srt", "vtt", "inconnu"] {
        assert!(!export_format_allowed(&job, format));
    }
    job.stage = Stage::Transcribed;
    assert!(export_format_allowed(&job, "json"));
    assert!(export_format_allowed(&job, "srt"));
}

#[test]
fn une_erreur_apres_le_dernier_lot_peut_finaliser_letat_persiste() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("travail.json");
    let mut job = Job::new("fiction.wav".into(), 30_000, 30_000);
    job.completed_chunks = job.chunks();
    job.target_language = Some("en".into());
    job.generate_report = true;
    job.report = Some("Rapport fondé sur la source".into());
    let mut segment = Segment::new(0, 1_000, "Bonjour".into());
    segment.translated_text = Some("Hello".into());
    job.segments.push(segment);
    job.stage = Stage::Interrupted;
    job.error = Some("Échec de lecture après sauvegarde".into());
    parole_core::save_job(&job, &path).unwrap();
    assert!(finalize_interrupted_job(&mut job, &path).unwrap());
    assert_eq!(job.stage, Stage::Transcribed);
    assert!(job.error.is_none());
    let persisted: Job = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(persisted.stage, Stage::Transcribed);
    assert_eq!(
        persisted.segments[0].translated_text.as_deref(),
        Some("Hello")
    );
    assert_eq!(
        persisted.report.as_deref(),
        Some("Rapport fondé sur la source")
    );
}

#[test]
fn les_traductions_manquantes_ou_a_relire_ne_sont_pas_finalisees() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("travail.json");
    let mut job = Job::new("fiction.wav".into(), 30_000, 30_000);
    job.completed_chunks = job.chunks();
    job.target_language = Some("en".into());
    job.stage = Stage::Interrupted;
    job.segments.push(Segment::new(0, 1_000, "Bonjour".into()));
    assert!(!finalize_interrupted_job(&mut job, &path).unwrap());
    job.segments[0].translated_text = Some("Bonjour".into());
    job.translation_issues = vec![0];
    assert!(!finalize_interrupted_job(&mut job, &path).unwrap());
    job.translation_issues.clear();
    job.generate_report = true;
    assert!(!finalize_interrupted_job(&mut job, &path).unwrap());
    assert_eq!(job.stage, Stage::Interrupted);
    assert!(!path.exists());
}
