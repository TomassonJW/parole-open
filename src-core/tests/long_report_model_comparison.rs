//! Essai local facultatif sur une réunion entièrement fictive, avec oracle figé avant inférence.
//! Aucune donnée utilisateur ni téléchargement ; ce banc n'émet pas de requête réseau,
//! mais le binaire local fourni n'est pas isolé du réseau : l'usage hors ligne reste à tester.
//! Succès = capture technique, PAS fidélité du compte rendu. Deux modèles, même entrée.
//! Réponses pré-parsing conservées si le moteur retourne Ok ; en cas de processus en échec,
//! LlamaCppEngine ne fournit ni stdout ni stderr, seule son erreur générique est sauvegardée.
//! Variables explicites : PAROLE_COMPARE_BASELINE, PAROLE_COMPARE_CANDIDATE,
//! PAROLE_COMPARE_BIN, PAROLE_COMPARE_OUT ; PAROLE_COMPARE_STRESS=1 pour le découpage exigeant.
use parole_core::language::{
    build_report, plan_sections, GenerationRequest, Language, LlamaCppEngine, ReportOptions,
    ReportState, TextGenerator,
};
use parole_core::report_models::report_model;
use parole_core::verified_report::render_synthesis_with_sources;
use parole_core::{
    docx::render_docx, render_complete_markdown, render_complete_txt, Job, Segment, Stage,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const SOURCE: &str = include_str!("fixtures/transcription-atlas-verriere-luciole.json");
const ORACLE: &str = include_str!("fixtures/oracle-atlas-verriere-luciole.json");
const ORACLE_V1: &str = include_str!("fixtures/oracle-atlas-verriere-luciole-v1.json");
const ERRATUM: &str = include_str!("fixtures/oracle-atlas-verriere-luciole-erratum.md");

struct CapturingEngine<G: TextGenerator> {
    inner: G,
    directory: PathBuf,
    next: usize,
}

impl<G: TextGenerator> TextGenerator for CapturingEngine<G> {
    fn generate(&mut self, request: &GenerationRequest) -> Result<String, String> {
        let response = self.inner.generate(request);
        self.next += 1;
        let record = json!({
            "sequence": self.next,
            "request": {
                "system": request.system.as_str(),
                "user": request.user.as_str(),
                "json_schema": request.json_schema.as_deref(),
                "max_tokens": request.max_tokens,
            },
            "response": response.as_ref().ok(),
            "error": response.as_ref().err(),
        });
        fs::write(
            self.directory
                .join(format!("generation-{:04}.json", self.next)),
            serde_json::to_vec_pretty(&record).map_err(|_| "Capture illisible")?,
        )
        .map_err(|_| "Impossible de conserver la réponse du générateur")?;
        response
    }
}

#[derive(Deserialize)]
struct MeetingMeta {
    synthetic: bool,
    source_language: String,
}

#[derive(Deserialize)]
struct MeetingFixture {
    meta: MeetingMeta,
    speakers: BTreeMap<String, String>,
    segments: Vec<Segment>,
}

fn write_trial_sources(output: &Path) {
    fs::write(output.join("transcription.json"), SOURCE).unwrap();
    fs::write(output.join("oracle-avant-inference.json"), ORACLE_V1).unwrap();
    fs::write(output.join("oracle-corrige-v2.json"), ORACLE).unwrap();
    fs::write(output.join("ERRATUM-oracle.md"), ERRATUM).unwrap();
}

fn fixture_and_oracle() -> (Job, Value) {
    let fixture: MeetingFixture = serde_json::from_str(SOURCE).unwrap();
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert!(fixture.meta.synthetic && oracle["meta"]["synthetic"] == true);
    assert_eq!(
        oracle["meta"]["source"],
        "transcription-atlas-verriere-luciole.json"
    );
    assert_eq!(fixture.speakers.len(), 4);
    assert_eq!(fixture.segments.len(), 44);
    let word_count: usize = fixture
        .segments
        .iter()
        .map(|s| s.text.split_whitespace().count())
        .sum();
    assert!((1_800..=2_400).contains(&word_count));
    for (index, segment) in fixture.segments.iter().enumerate() {
        assert!(segment.start_ms < segment.end_ms && segment.translated_text.is_none());
        assert!(fixture
            .speakers
            .contains_key(segment.speaker_id.as_deref().unwrap()));
        if index > 0 {
            assert!(fixture.segments[index - 1].end_ms < segment.start_ms);
        }
    }
    let mut evidence_count = 0;
    for (key, expected) in [
        ("events", 11),
        ("tasks", 7),
        ("open_questions", 3),
        ("forbidden_claims", 10),
    ] {
        let items = oracle[key].as_array().unwrap();
        assert_eq!(items.len(), expected, "{key}");
        for item in items {
            for evidence_key in ["evidence", "contradiction"] {
                if let Some(references) = item[evidence_key].as_array() {
                    for reference in references {
                        let index = reference["segment_index"].as_u64().unwrap() as usize;
                        let segment = &fixture.segments[index];
                        assert_eq!(reference["start_ms"], segment.start_ms);
                        assert_eq!(reference["end_ms"], segment.end_ms);
                        assert!(segment.text.contains(reference["quote"].as_str().unwrap()));
                        evidence_count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(evidence_count, 61);
    let duration = fixture.segments.last().unwrap().end_ms;
    let mut job = Job::new(
        "reunion-atlas-verriere-luciole-fictive.wav".into(),
        duration,
        30_000,
    );
    job.source_language = fixture.meta.source_language;
    job.speaker_names = fixture.speakers;
    job.segments = fixture.segments;
    (job, oracle)
}

#[test]
fn chaque_essai_conserve_les_deux_oracles_et_leur_erratum() {
    let temp = tempfile::tempdir().unwrap();
    write_trial_sources(temp.path());
    assert_eq!(
        fs::read_to_string(temp.path().join("transcription.json")).unwrap(),
        SOURCE
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("oracle-avant-inference.json")).unwrap(),
        ORACLE_V1
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("oracle-corrige-v2.json")).unwrap(),
        ORACLE
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("ERRATUM-oracle.md")).unwrap(),
        ERRATUM
    );
    let original: Value = serde_json::from_str(ORACLE_V1).unwrap();
    let corrected: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(original["tasks"][6]["id"], "légendes");
    assert_eq!(original["tasks"][6]["status"], "active");
    assert_eq!(corrected["tasks"][6]["status"], "achevée");
    assert_eq!(corrected["meta"]["revision"], 2);
}

#[test]
fn la_capture_distingue_erreur_et_json_malforme_sans_inventer_de_stdout() {
    struct Broken;
    impl TextGenerator for Broken {
        fn generate(&mut self, _request: &GenerationRequest) -> Result<String, String> {
            Err("Échec local".into())
        }
    }
    struct Invalid;
    impl TextGenerator for Invalid {
        fn generate(&mut self, _request: &GenerationRequest) -> Result<String, String> {
            Ok("{réponse incomplète".into())
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let request = GenerationRequest {
        system: "test".into(),
        user: "fiction".into(),
        json_schema: None,
        max_tokens: 32,
    };
    let mut broken = CapturingEngine {
        inner: Broken,
        directory: temp.path().into(),
        next: 0,
    };
    assert_eq!(broken.generate(&request).unwrap_err(), "Échec local");
    let failed: Value =
        serde_json::from_slice(&fs::read(temp.path().join("generation-0001.json")).unwrap())
            .unwrap();
    assert!(failed["response"].is_null());
    assert_eq!(failed["error"], "Échec local");
    let mut invalid = CapturingEngine {
        inner: Invalid,
        directory: temp.path().into(),
        next: 1,
    };
    assert_eq!(invalid.generate(&request).unwrap(), "{réponse incomplète");
    let malformed: Value =
        serde_json::from_slice(&fs::read(temp.path().join("generation-0002.json")).unwrap())
            .unwrap();
    assert_eq!(malformed["response"], "{réponse incomplète");
    assert!(malformed["error"].is_null());
}

#[test]
fn la_reponse_du_generateur_est_capturee_avant_le_parseur() {
    struct Fixed;
    impl TextGenerator for Fixed {
        fn generate(&mut self, _request: &GenerationRequest) -> Result<String, String> {
            Ok("{\"fait\":true}".into())
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let mut capturing = CapturingEngine {
        inner: Fixed,
        directory: temp.path().into(),
        next: 0,
    };
    let request = GenerationRequest {
        system: "instruction locale".into(),
        user: "texte fictif".into(),
        json_schema: None,
        max_tokens: 32,
    };
    assert_eq!(capturing.generate(&request).unwrap(), "{\"fait\":true}");
    let saved: Value =
        serde_json::from_slice(&fs::read(temp.path().join("generation-0001.json")).unwrap())
            .unwrap();
    assert_eq!(saved["request"]["user"], "texte fictif");
    assert_eq!(saved["response"], "{\"fait\":true}");
    assert_eq!(saved["sequence"], 1);
}

#[test]
fn reunion_longue_et_oracle_sont_coherents_avant_inference() {
    let (job, oracle) = fixture_and_oracle();
    let normal = plan_sections(&job, ReportOptions::new(Language::French).section_chars).len();
    let stress = plan_sections(&job, 900).len();
    assert!((4..=44).contains(&normal));
    assert!(stress > normal && stress <= 44);
    assert!(oracle["forbidden_claims"].as_array().unwrap().len() >= 10);
}

#[test]
#[ignore = "Rejeu en mémoire de l'état Atlas fictif épinglé ; PAROLE_ATLAS_REPLAY_STATE requis"]
fn rejouer_l_etat_atlas_et_verifier_l_echeance_d_un_autre_dossier() {
    let input = env::var("PAROLE_ATLAS_REPLAY_STATE").expect("état Atlas fictif requis");
    let bytes = fs::read(input).expect("état Atlas fictif illisible");
    // Garde contre la confusion accidentelle avec un travail réel, pas une authentification.
    let digest = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    });
    assert!(
        digest == 0x6143_cdcc_7f19_00b8,
        "état différent de la fixture fictive épinglée : rejeu refusé"
    );
    let state: ReportState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(state.fingerprint, "b9cc844e3055fa0f");
    assert_eq!(state.language, Language::French);
    assert_eq!(state.sections.len(), 7);
    let report = state.report.expect("rapport structuré terminé");
    assert!(report.actions.iter().any(|action| {
        action.responsable == "Élodie"
            && action.tache == "reprendre le tableau à Malik"
            && action.echeance == "11 mai"
    }));
    let (job, _) = fixture_and_oracle();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        rendered.contains("| Élodie | reprendre le tableau à Malik | à confirmer |"),
        "date d'un autre dossier encore propagée au tableau"
    );
    assert!(!rendered.contains("| Élodie | reprendre le tableau à Malik | 11 mai |"));
    assert!(
        rendered.contains("| à confirmer | consolider le tableau des anomalies | à confirmer |")
    );
    assert!(rendered.contains("| 10 mai 2032 |"));
    assert!(rendered.contains("| 24 mai 2032 |"));
    assert!(rendered.contains("Luciole le 11 mai n'est pas annulé"));
}

#[test]
#[ignore = "Nécessite deux modèles locaux épinglés, un moteur local et un dossier de sortie explicite"]
fn deux_modeles_locaux_sur_reunion_longue_et_rectifications() {
    let baseline = env::var("PAROLE_COMPARE_BASELINE").expect("PAROLE_COMPARE_BASELINE requis");
    let candidate = env::var("PAROLE_COMPARE_CANDIDATE").expect("PAROLE_COMPARE_CANDIDATE requis");
    let binary = env::var("PAROLE_COMPARE_BIN").expect("PAROLE_COMPARE_BIN requis");
    let parent = PathBuf::from(env::var("PAROLE_COMPARE_OUT").expect("PAROLE_COMPARE_OUT requis"));
    let stress = match env::var("PAROLE_COMPARE_STRESS") {
        Ok(value) if value == "1" => true,
        Err(env::VarError::NotPresent) => false,
        _ => panic!("PAROLE_COMPARE_STRESS doit valoir 1 ou être absent"),
    };
    let mut options = ReportOptions::new(Language::French);
    if stress {
        options.section_chars = 900;
        options.synthesis_chars = 1_800;
    }
    let (job, _) = fixture_and_oracle();
    let section_count = plan_sections(&job, options.section_chars).len();
    assert!((if stress { 10 } else { 4 }..=44).contains(&section_count));
    fs::create_dir_all(&parent).unwrap();
    let run = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let label = if stress {
        "atlas-stress"
    } else {
        "atlas-normal"
    };
    let output = parent.join(format!("{label}-{run}-{}", std::process::id()));
    fs::create_dir(&output).expect("dossier d'essai neuf requis : aucun état antérieur réutilisé");
    write_trial_sources(&output);
    eprintln!(
        "{label} : {} interventions, {section_count} sections prévues",
        job.segments.len()
    );
    for (id, path) in [
        ("baseline", baseline),
        ("qwen3-4b-instruct-2507-q4_k_m", candidate),
    ] {
        let model = report_model(id).unwrap();
        let engine =
            LlamaCppEngine::new(PathBuf::from(&binary), PathBuf::from(path), output.clone());
        engine
            .check(&model.spec())
            .expect("taille et empreinte du modèle épinglé");
        let capture_dir = output.join(format!("reponses-generateur-{id}"));
        fs::create_dir(&capture_dir).unwrap();
        let mut engine = CapturingEngine {
            inner: engine,
            directory: capture_dir,
            next: 0,
        };
        let mut trial = job.clone();
        trial.report_model_id = id.into();
        let began = Instant::now();
        let report = build_report(
            &trial,
            &output.join(format!("etat-intermediaire-{id}.json")),
            &mut engine,
            &options,
            |done, total| {
                eprintln!("{label}/{id} : {done}/{total}");
                Ok(())
            },
        )
        .expect("inférence du compte rendu sur l'entrée fictive");
        let seconds = began.elapsed().as_secs_f64();
        assert_eq!(report.sections.len(), section_count);
        assert!(!report.synthese.trim().is_empty());
        let rendered = render_synthesis_with_sources(&trial, &report);
        assert!(rendered.contains("## Passages sources\n\n"));
        trial.report = Some(rendered.clone());
        trial.report_format_version = 1;
        trial.generate_report = true;
        trial.completed_chunks = trial.chunks();
        trial.stage = Stage::Transcribed;
        fs::write(output.join(format!("rapport-{id}.md")), &rendered).unwrap();
        fs::write(
            output.join(format!("export-{id}.md")),
            render_complete_markdown(&trial),
        )
        .unwrap();
        fs::write(
            output.join(format!("export-{id}.txt")),
            render_complete_txt(&trial),
        )
        .unwrap();
        fs::write(
            output.join(format!("export-{id}.docx")),
            render_docx(&trial).unwrap(),
        )
        .unwrap();
        fs::write(
            output.join(format!("mesure-{id}.json")),
            serde_json::to_vec_pretty(
                &json!({"mode": label, "model": id, "sections": section_count,
                "section_chars": options.section_chars, "synthesis_chars": options.synthesis_chars,
                "elapsed_seconds": seconds, "synthetic": true}),
            )
            .unwrap(),
        )
        .unwrap();
        eprintln!(
            "{label}/{id} : {section_count} sections, {seconds:.1} secondes, {}",
            output.display()
        );
    }
}
