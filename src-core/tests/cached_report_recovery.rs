//! Reproduit une interruption après l'enregistrement du rapport structuré,
//! avant son rattachement à travail.json. Générateur scripté : pas une mesure de modèle.
use parole_core::{
    language::{build_report, load_report_state, report_options, GenerationRequest, TextGenerator},
    report_lifecycle::prepare_report_execution,
    report_models::report_model_for_generation,
    save_job, Job, Segment, Stage,
};
use std::{fs, path::Path};

struct FixtureGenerator;
impl TextGenerator for FixtureGenerator {
    fn generate(&mut self, _: &GenerationRequest) -> Result<String, String> {
        Ok(r#"{"titre":"Réunion Atlas","resume":"Le budget du projet Atlas sera relu après la réunion.","synthese":"Le budget du projet Atlas sera relu après la réunion.","decisions":[],"actions":[],"questions":[]}"#.into())
    }
}

fn persisted_gap(dir: &Path) -> Job {
    let mut job = Job::new("fiction-atlas.wav".into(), 10_000, 10_000);
    job.source_language = "fr".into();
    job.report_language = Some("fr".into());
    job.report_model_id = "qwen3-4b-instruct-2507-q4_k_m".into();
    job.generate_report = true;
    job.completed_chunks = job.chunks();
    job.stage = Stage::Reporting;
    job.segments.push(Segment::new(
        0,
        8_000,
        "Nous allons relire le budget du projet Atlas après cette réunion.".into(),
    ));
    save_job(&job, &dir.join("travail.json")).unwrap();
    let options = report_options(&job).unwrap();
    let failure = build_report(
        &job,
        &dir.join("compte-rendu-etat.json"),
        &mut FixtureGenerator,
        &options,
        |done, total| {
            if done == total {
                Err("Fermeture après la dernière sauvegarde".into())
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(failure, "Fermeture après la dernière sauvegarde");
    assert!(load_report_state(&dir.join("compte-rendu-etat.json"))
        .unwrap()
        .unwrap()
        .report
        .is_some());
    assert!(job.report.is_none());
    job
}

#[test]
fn retired_completed_report_is_recovered_without_model_or_generator() {
    let dir = tempfile::tempdir().unwrap();
    let mut job = persisted_gap(dir.path());
    assert!(report_model_for_generation(&job.report_model_id).is_err());
    let report_path = dir.path().join("compte-rendu-etat.json");
    let original = fs::read(&report_path).unwrap();
    assert!(prepare_report_execution(&mut job, &dir.path().join("travail.json")).unwrap());
    assert!(job.report.as_deref().unwrap().contains("Atlas"));
    assert_eq!(job.report_format_version, 1);
    assert_eq!(job.stage, Stage::Transcribed);
    assert!(job.error.is_none());
    assert_eq!(fs::read(&report_path).unwrap(), original);
    let saved: Job =
        serde_json::from_slice(&fs::read(dir.path().join("travail.json")).unwrap()).unwrap();
    assert_eq!(saved.report, job.report);
    assert!(!prepare_report_execution(&mut job, &dir.path().join("travail.json")).unwrap());
}

#[test]
fn stale_or_unfinished_cached_reports_do_not_reopen_the_retired_model() {
    for defect in [
        "text",
        "model",
        "language",
        "unfinished",
        "version",
        "translation",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut job = persisted_gap(dir.path());
        let path = dir.path().join("compte-rendu-etat.json");
        let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        match defect {
            "text" => job.segments[0].text = "Cette transcription a changé.".into(),
            "model" => raw["fingerprint"] = "0000000000000000".into(),
            "language" => raw["language"] = "English".into(),
            "unfinished" => raw["report"] = serde_json::Value::Null,
            "version" => raw["version"] = 999.into(),
            "translation" => {
                job.target_language = Some("en".into());
                job.report_language = Some("en".into());
            }
            _ => unreachable!(),
        }
        fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        let before = fs::read(dir.path().join("travail.json")).unwrap();
        assert!(
            prepare_report_execution(&mut job, &dir.path().join("travail.json")).is_err(),
            "{defect}"
        );
        assert!(job.report.is_none(), "{defect}");
        assert_eq!(fs::read(dir.path().join("travail.json")).unwrap(), before);
    }
}

#[test]
fn recovered_source_report_keeps_incomplete_translation_visible() {
    let dir = tempfile::tempdir().unwrap();
    let mut job = persisted_gap(dir.path());
    job.target_language = Some("en".into());
    job.translation_issues = vec![0];
    assert!(prepare_report_execution(&mut job, &dir.path().join("travail.json")).unwrap());
    assert!(job.report.is_some());
    assert_eq!(job.stage, Stage::Interrupted);
    assert!(job.result_incomplete());
    assert_eq!(job.translation_issues, vec![0]);
}

#[test]
fn failed_persistence_does_not_modify_the_in_memory_job_or_cache() {
    let dir = tempfile::tempdir().unwrap();
    let mut job = persisted_gap(dir.path());
    let before = serde_json::to_value(&job).unwrap();
    // save_job remplace l'extension par .tmp : bloquer cette destination exacte.
    fs::create_dir(dir.path().join("travail.tmp")).unwrap();
    assert!(prepare_report_execution(&mut job, &dir.path().join("travail.json")).is_err());
    assert_eq!(serde_json::to_value(&job).unwrap(), before);
    assert!(
        load_report_state(&dir.path().join("compte-rendu-etat.json"))
            .unwrap()
            .unwrap()
            .report
            .is_some()
    );
}

#[test]
fn both_native_entrypoints_prepare_recovery_before_model_access() {
    // Garde de branchement source, pas test d'application Windows installée.
    let source = include_str!("../../src-tauri/src/jobs.rs");
    let resume = source
        .split("pub fn resume_job(")
        .nth(1)
        .unwrap()
        .split("fn run(")
        .next()
        .unwrap();
    let run = source
        .split("fn run_report(")
        .nth(1)
        .unwrap()
        .split("pub fn cancel_job(")
        .next()
        .unwrap();
    for body in [resume, run] {
        let recovery = body
            .find("prepare_report_execution(")
            .expect("récupération absente du point d'entrée");
        let model = body
            .find("models::report_model_path(")
            .expect("garde du modèle absent");
        assert!(
            recovery < model,
            "le modèle est exigé avant la récupération du rapport"
        );
    }
}
