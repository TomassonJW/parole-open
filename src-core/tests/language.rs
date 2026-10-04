//! Tests du module `language`.
//!
//! Les tests de logique utilisent un générateur scripté (double de test) : ils vérifient le
//! découpage, la reprise, la persistance et les garde-fous, pas la qualité linguistique.
//! Les tests `moteur_reel_*` exécutent le vrai llama.cpp sur CPU ; ils ne s'exécutent que si
//! `PAROLE_LLAMA_BIN` (llama-completion) et `PAROLE_LLM_MODEL` (GGUF épinglé) sont définis,
//! sinon ils l'annoncent et s'arrêtent sans rien affirmer.
use parole_core::language::{
    build_report, dedupe_sentences, guess_language, load_report_state,
    may_build_source_report_after_translation_error, plan_sections, render_report_markdown,
    render_translated_txt, report_language, report_options, sha256_hex, translate_job,
    validate_report_input, verify_model_file, GenerationRequest, Language, LlamaCppEngine,
    ModelSpec, ReportOptions, TextGenerator, TranslationOptions, MODELE_TEXTE_RECOMMANDE,
};
use parole_core::verified_report::render_synthesis_with_sources;
use parole_core::{save_job, Job, Segment};
use serde_json::Value;
use std::{env, fs, path::PathBuf};

fn job_with(texts: &[&str], language: &str) -> Job {
    let mut job = Job::new("reunion.wav".into(), 600_000, 60_000);
    job.source_language = language.into();
    for (i, text) in texts.iter().enumerate() {
        let start = i as u64 * 5_000;
        job.segments
            .push(Segment::new(start, start + 4_000, (*text).into()));
    }
    job
}

#[test]
fn cent_mille_mots_synthetiques_couvrent_les_indices_des_passages() {
    let dir = tempfile::tempdir().unwrap();
    let text = vec!["budget"; 100].join(" ");
    let texts = vec![text.as_str(); 1_000];
    let mut job = job_with(&texts, "fr");
    job.duration_ms = 5_000_000;
    let options = ReportOptions::new(Language::French);
    let mut engine = Scripted::new();
    let report = build_report(
        &job,
        &dir.path().join("long.json"),
        &mut engine,
        &options,
        |_, _| Ok(()),
    )
    .unwrap();
    assert!(report.sections.len() > 100);
    assert_eq!(report.sections.first().unwrap().first_segment, 0);
    assert_eq!(report.sections.last().unwrap().last_segment, 999);
    for pair in report.sections.windows(2) {
        assert_eq!(pair[0].last_segment + 1, pair[1].first_segment);
    }
    assert!(
        engine.calls.iter().all(|request| {
            request.system.len() + request.user.len() + request.max_tokens as usize + 320 < 8_192
        }),
        "une invite dépasse la fenêtre du petit modèle"
    );
}

#[test]
fn langue_du_rapport_independante_de_la_traduction_et_compatible_reprise() {
    let mut job = job_with(&["Bonjour, nous validons le projet."], "fr");
    job.target_language = Some("en".into());
    assert_eq!(report_language(&job).unwrap(), Language::English);
    assert!(report_options(&job).unwrap().use_translation);
    job.report_language = Some("fr".into());
    assert_eq!(report_language(&job).unwrap(), Language::French);
    assert!(!report_options(&job).unwrap().use_translation);
    let restored: Job = serde_json::from_slice(&serde_json::to_vec(&job).unwrap()).unwrap();
    assert_eq!(report_language(&restored).unwrap(), Language::French);
    job.report_language = Some("de".into());
    assert!(report_language(&job).is_err());
    job.report_language = None;
    job.target_language = None;
    assert_eq!(report_language(&job).unwrap(), Language::French);
}

#[test]
fn langue_du_rapport_absente_reste_compatible_avec_les_anciens_travaux() {
    let job = job_with(&["The team will review the project tomorrow."], "en");
    let mut raw = serde_json::to_value(&job).unwrap();
    raw.as_object_mut().unwrap().remove("report_language");
    let restored: Job = serde_json::from_value(raw).unwrap();
    assert_eq!(restored.report_language, None);
    assert_eq!(report_language(&restored).unwrap(), Language::English);
}

/// Traduit « mécaniquement » en préfixant, et consigne chaque requête.
struct Scripted {
    calls: Vec<GenerationRequest>,
    fail_on_call: Option<usize>,
    untranslated_on_call: Option<usize>,
    clean_translation: bool,
    wrong_count_on_batches: bool,
}
impl Scripted {
    fn new() -> Self {
        Self {
            calls: vec![],
            fail_on_call: None,
            untranslated_on_call: None,
            clean_translation: false,
            wrong_count_on_batches: false,
        }
    }
}
fn numbered_lines(user: &str) -> Vec<String> {
    user.lines()
        .filter_map(|l| l.split_once("] ").map(|(_, t)| t.to_string()))
        .collect()
}
impl TextGenerator for Scripted {
    fn generate(&mut self, request: &GenerationRequest) -> Result<String, String> {
        self.calls.push(request.clone());
        if self.fail_on_call == Some(self.calls.len()) {
            return Err("Échec du moteur de langue local".into());
        }
        let schema: Value = serde_json::from_str(request.json_schema.as_deref().unwrap()).unwrap();
        let props = &schema["properties"];
        if props.get("translations").is_some() {
            let mut lines: Vec<String> = numbered_lines(&request.user)
                .into_iter()
                .map(|t| {
                    if self.untranslated_on_call == Some(self.calls.len()) {
                        t
                    } else if self.clean_translation && t.starts_with("Nous avons décidé") {
                        "The budget will be reviewed at our next meeting.".to_string()
                    } else if self.clean_translation {
                        "Thank you for your attendance.".to_string()
                    } else {
                        format!("EN:{t}")
                    }
                })
                .collect();
            if self.wrong_count_on_batches && lines.len() > 1 {
                lines.pop();
            }
            return Ok(serde_json::json!({ "translations": lines }).to_string());
        }
        if props.get("points").is_some() {
            let first = request.user.lines().nth(1).unwrap_or("").to_string();
            return Ok(serde_json::json!({
                "titre": "Point budget",
                "resume": format!("Le passage traite du budget. {first}"),
                "points": ["Le budget est discuté"],
                "decisions": ["Le lancement est reporté en mars"],
                "actions": [
                    {"responsable": "Marie", "tache": "Envoyer le budget", "echeance": "vendredi"},
                    {"responsable": "Gérard", "tache": "Appeler le client", "echeance": ""}
                ],
                "questions": []
            })
            .to_string());
        }
        Ok(serde_json::json!({
            "titre": "Réunion budget",
            "synthese": format!("Synthèse de {} caractères de résumés.", request.user.chars().count())
        })
        .to_string())
    }
}

#[test]
fn traduction_anglaise_et_rapport_francais_utilisent_les_paroles_originales() {
    let dir = tempfile::tempdir().unwrap();
    let mut job = job_with(&["Nous avons discuté du budget de la réunion."], "fr");
    job.segments[0].translated_text = Some("The budget was discussed in the meeting.".into());
    job.target_language = Some("en".into());
    job.report_language = Some("fr".into());
    job.translation_issues.push(0);
    let options = report_options(&job).unwrap();
    assert!(
        !options.use_translation,
        "une traduction douteuse ne bloque pas le rapport fondé sur l'original"
    );
    assert!(validate_report_input(&job, &options).is_ok());
    job.report_language = Some("en".into());
    assert!(validate_report_input(&job, &report_options(&job).unwrap()).is_err());
    job.report_language = Some("fr".into());
    let mut engine = Scripted::new();
    let report = build_report(
        &job,
        &dir.path().join("rapport.json"),
        &mut engine,
        &options,
        |_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(report.language, Language::French);
    assert!(engine.calls[0]
        .user
        .contains("Nous avons discuté du budget"));
    assert!(!engine.calls[0].user.contains("The budget was discussed"));
}

#[test]
fn echec_total_de_traduction_conserve_original_et_autorise_seulement_rapport_source() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("travail.json");
    let source = "Nous avons discuté du budget de la réunion.";
    let mut job = job_with(&[source], "fr");
    job.generate_report = true;
    job.target_language = Some("en".into());
    job.report_language = Some("fr".into());
    let mut broken = Scripted::new();
    broken.fail_on_call = Some(1);
    assert!(translate_job(
        &mut job,
        &state,
        &mut broken,
        &TranslationOptions::new(Language::English),
        |_, _| Ok(())
    )
    .is_err());
    let disk: Job = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    assert_eq!(disk.segments[0].text, source);
    assert_eq!(disk.segments[0].translated_text, None);
    assert!(may_build_source_report_after_translation_error(
        &disk, false
    ));
    let options = report_options(&disk).unwrap();
    assert!(!options.use_translation);
    let mut engine = Scripted::new();
    let report = build_report(
        &disk,
        &dir.path().join("rapport.json"),
        &mut engine,
        &options,
        |_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(report.language, Language::French);
    assert!(engine.calls[0].user.contains(source));
    let source_report = render_synthesis_with_sources(&disk, &report);
    let mut suspended = disk.clone();
    suspended.report = Some(source_report.clone());
    save_job(&suspended, &state).unwrap();
    let mut resumed: Job = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    let mut translator = Scripted::new();
    translate_job(
        &mut resumed,
        &state,
        &mut translator,
        &TranslationOptions::new(Language::English),
        |_, _| Ok(()),
    )
    .unwrap();
    assert!(resumed.segments[0].translated_text.is_some());
    assert_eq!(resumed.report.as_deref(), Some(source_report.as_str()));

    let mut target_report = disk.clone();
    target_report.report_language = Some("en".into());
    assert!(!may_build_source_report_after_translation_error(
        &target_report,
        false
    ));
    target_report.report_language = None; // défaut historique = langue traduite
    assert!(!may_build_source_report_after_translation_error(
        &target_report,
        false
    ));
    assert!(!may_build_source_report_after_translation_error(
        &disk, true
    ));
    target_report = disk.clone();
    target_report.report = Some("déjà rédigé".into());
    assert!(!may_build_source_report_after_translation_error(
        &target_report,
        false
    ));
}

#[test]
fn choix_du_rapport_sans_texte_dans_la_langue_demandee_refuse_avant_traitement() {
    let mut job = job_with(&["The team discussed the budget."], "en");
    job.target_language = Some("en".into());
    job.report_language = Some("fr".into());
    assert!(report_options(&job)
        .unwrap_err()
        .contains("paroles ou de la traduction"));
    job.source_language = "auto".into();
    assert!(report_options(&job)
        .unwrap_err()
        .contains("précisez la langue parlée"));
    job.source_language = "fr".into();
    assert!(!report_options(&job).unwrap().use_translation);
}

#[test]
fn empreinte_sha256_conforme_aux_vecteurs_officiels() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let long = vec![b'a'; 1_000_000];
    assert_eq!(
        sha256_hex(&long),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn modele_refuse_si_empreinte_ou_taille_differente() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("m.gguf");
    fs::write(&path, b"abc").unwrap();
    let mut spec = ModelSpec {
        size_bytes: 3,
        sha256: "00",
        ..MODELE_TEXTE_RECOMMANDE
    };
    assert_eq!(
        verify_model_file(&path, &spec).unwrap_err(),
        "Empreinte du modèle de langue invalide : fichier refusé"
    );
    spec.sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert!(verify_model_file(&path, &spec).is_ok());
    spec.size_bytes = 4;
    assert!(verify_model_file(&path, &spec)
        .unwrap_err()
        .contains("incomplet"));
    assert!(verify_model_file(&dir.path().join("absent"), &spec)
        .unwrap_err()
        .contains("introuvable"));
}

#[test]
fn phrases_repetees_supprimees() {
    assert_eq!(
        dedupe_sentences("Le budget est validé. Paul appelle. Le budget est validé ! Fin"),
        "Le budget est validé. Paul appelle. Fin"
    );
    assert_eq!(
        dedupe_sentences("Version 2.5 retenue."),
        "Version 2.5 retenue."
    );
}

#[test]
fn avertissement_si_compte_rendu_dans_une_autre_langue_sans_traduction() {
    let dir = tempfile::tempdir().unwrap();
    let job = job_with(
        &["We decided that the budget is approved for the team."],
        "en",
    );
    let mut engine = Scripted::new();
    let report = build_report(
        &job,
        &dir.path().join("cr.json"),
        &mut engine,
        &ReportOptions::new(Language::French),
        |_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(report.avertissements.len(), 1);
    assert!(report.avertissements[0]
        .starts_with("Transcription en anglais résumée directement en français"));
}

#[test]
fn detection_de_langue_prudente() {
    assert_eq!(
        guess_language("Nous avons décidé que le budget est validé"),
        Some(Language::French)
    );
    assert_eq!(
        guess_language("We decided that the budget is approved for the team"),
        Some(Language::English)
    );
    assert_eq!(guess_language("OK"), None);
    assert_eq!(Language::parse("Anglais").unwrap(), Language::English);
    assert!(Language::parse("de")
        .unwrap_err()
        .starts_with("Langue non prise en charge"));
}

#[test]
fn traduction_par_lots_persistee_et_reprise_apres_interruption() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("travail.json");
    let texts: Vec<String> = (0..10).map(|i| format!("Phrase numéro {i}.")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let mut job = job_with(&refs, "fr");
    let mut options = TranslationOptions::new(Language::English);
    options.max_segments_per_batch = 4;

    let mut engine = Scripted::new();
    engine.fail_on_call = Some(2);
    let err = translate_job(&mut job, &state, &mut engine, &options, |_, _| Ok(())).unwrap_err();
    assert_eq!(err, "Échec du moteur de langue local");
    let disk: Job = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    let done = disk
        .segments
        .iter()
        .filter(|s| s.translated_text.is_some())
        .count();
    assert_eq!(done, 4, "le premier lot doit être sur disque");

    let mut job = disk;
    let mut engine = Scripted::new();
    let mut progress = vec![];
    let summary = translate_job(&mut job, &state, &mut engine, &options, |d, t| {
        progress.push((d, t));
        Ok(())
    })
    .unwrap();
    assert_eq!(summary.already_done, 4);
    assert_eq!(summary.translated_now, 6);
    assert_eq!(
        engine.calls.len(),
        2,
        "seuls les lots restants sont demandés"
    );
    assert_eq!(progress, vec![(8, 10), (10, 10)]);
    assert!(engine.calls[0].user.starts_with("[1] Phrase numéro 4"));
    assert!(engine.calls[0].system.contains("from French into English"));
    for (seg, text) in job.segments.iter().zip(&texts) {
        assert_eq!(
            seg.translated_text.as_deref(),
            Some(format!("EN:{text}").as_str())
        );
    }
    let rendered = render_translated_txt(&job);
    assert!(rendered.starts_with("[00:00:00,000] Locuteur non attribué : EN:Phrase numéro 0"));
}

#[test]
fn traduction_suspecte_reprise_apres_panne_sans_perdre_alerte() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("travail.json");
    let french = "Nous avons décidé que le budget sera revu lors de notre prochaine réunion.";
    let mut job = job_with(&[french, "Merci pour votre présence."], "fr");
    let mut options = TranslationOptions::new(Language::English);
    options.max_segments_per_batch = 1;
    let mut broken = Scripted::new();
    broken.untranslated_on_call = Some(1);
    broken.fail_on_call = Some(2);
    assert!(translate_job(&mut job, &state, &mut broken, &options, |_, _| Ok(())).is_err());
    let mut disk: Job = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    assert_eq!(disk.translation_issues, vec![0]);
    assert_eq!(disk.segments[0].translated_text.as_deref(), Some(french));
    let mut resumed = Scripted::new();
    resumed.clean_translation = true;
    let summary = translate_job(&mut disk, &state, &mut resumed, &options, |_, _| Ok(())).unwrap();
    assert_eq!(resumed.calls.len(), 2, "le passage douteux est retraduit");
    assert!(summary.suspect_segments.is_empty());
    assert!(disk.translation_issues.is_empty());
}

#[test]
fn lot_incoherent_retraduit_segment_par_segment() {
    let dir = tempfile::tempdir().unwrap();
    let mut job = job_with(&["Un.", "Deux.", "Trois."], "fr");
    let mut engine = Scripted::new();
    engine.wrong_count_on_batches = true;
    let summary = translate_job(
        &mut job,
        &dir.path().join("t.json"),
        &mut engine,
        &TranslationOptions::new(Language::English),
        |_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(summary.batches_retried, 1);
    assert_eq!(engine.calls.len(), 4);
    assert_eq!(
        job.segments[2].translated_text.as_deref(),
        Some("EN:Trois.")
    );
}

#[test]
fn refus_explicites_en_francais() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("t.json");
    let mut engine = Scripted::new();
    let mut job = job_with(&["Bonjour à tous, nous avons une réunion."], "fr");
    let err = translate_job(
        &mut job,
        &state,
        &mut engine,
        &TranslationOptions::new(Language::French),
        |_, _| Ok(()),
    )
    .unwrap_err();
    assert_eq!(err, "La transcription est déjà en français");
    let mut job = job_with(&["OK"], "auto");
    let err = translate_job(
        &mut job,
        &state,
        &mut engine,
        &TranslationOptions::new(Language::English),
        |_, _| Ok(()),
    )
    .unwrap_err();
    assert!(err.starts_with("Langue de la transcription inconnue"));
    let mut job = job_with(&[], "fr");
    assert_eq!(
        translate_job(
            &mut job,
            &state,
            &mut engine,
            &TranslationOptions::new(Language::English),
            |_, _| Ok(())
        )
        .unwrap_err(),
        "Aucune transcription à traduire"
    );
    let job = job_with(&["Bonjour."], "fr");
    let mut options = ReportOptions::new(Language::French);
    options.use_translation = true;
    assert_eq!(
        build_report(
            &job,
            &dir.path().join("cr.json"),
            &mut engine,
            &options,
            |_, _| Ok(())
        )
        .unwrap_err(),
        "Traduction incomplète : terminez-la avant le compte rendu"
    );
    let engine = LlamaCppEngine::new(
        dir.path().join("absent"),
        dir.path().join("m"),
        dir.path().into(),
    );
    assert!(engine
        .check(&MODELE_TEXTE_RECOMMANDE)
        .unwrap_err()
        .contains("introuvable"));
}

#[test]
fn compte_rendu_hierarchique_repris_et_invalide_si_transcription_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("compte-rendu.json");
    let long =
        "Marie doit envoyer le budget mis à jour avant vendredi, nous en avons parlé longuement.";
    let texts: Vec<&str> = (0..40).map(|_| long).collect();
    let mut job = job_with(&texts, "fr");
    job.segments[0].speaker_id = Some("S1".into());
    job.speaker_names.insert("S1".into(), "Marie".into());
    let mut options = ReportOptions::new(Language::French);
    options.section_chars = 600;
    options.synthesis_chars = 500;
    let plan = plan_sections(&job, options.section_chars);
    assert!(plan.len() >= 5);
    assert_eq!(plan.first().unwrap().0, 0);
    assert_eq!(plan.last().unwrap().1, 39);
    for pair in plan.windows(2) {
        assert_eq!(pair[0].1 + 1, pair[1].0, "passages contigus sans trou");
    }

    // Interruption après le 2e passage.
    let mut engine = Scripted::new();
    let err = build_report(&job, &path, &mut engine, &options, |done, _| {
        if done == 2 {
            Err("Interrompu par l'utilisateur".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(err, "Interrompu par l'utilisateur");
    assert_eq!(load_report_state(&path).unwrap().unwrap().sections.len(), 2);
    assert!(
        engine.calls[0].user.contains("] Marie : "),
        "noms des locuteurs transmis"
    );

    let mut engine = Scripted::new();
    let report = build_report(&job, &path, &mut engine, &options, |_, _| Ok(())).unwrap();
    let section_calls = engine.calls.iter().filter(|c| c.max_tokens == 900).count();
    assert_eq!(
        section_calls,
        plan.len() - 2,
        "les passages faits ne sont pas refaits"
    );
    let synth_calls = engine.calls.len() - section_calls;
    assert!(
        synth_calls >= 2,
        "réduction hiérarchique attendue, {synth_calls} appel(s)"
    );
    assert_eq!(report.sections.len(), plan.len());
    assert_eq!(report.decisions.len(), 1, "décisions dédoublonnées");
    assert_eq!(report.actions.len(), 2);
    assert_eq!(report.actions[0].responsable, "Marie");
    assert_eq!(
        report.actions[1].responsable, "non précisé",
        "Gérard n'est pas cité"
    );
    assert_eq!(report.actions[1].echeance, "non précisé");

    // Déjà terminé : aucun appel.
    let mut engine = Scripted::new();
    assert_eq!(
        build_report(&job, &path, &mut engine, &options, |_, _| Ok(())).unwrap(),
        report
    );
    assert!(engine.calls.is_empty());

    // Un autre modèle ne doit pas réutiliser silencieusement les notes ni le
    // rapport achevés par le précédent, même si la transcription ne change pas.
    job.report_model_id = "qwen3-4b-instruct-2507-q4_k_m".into();
    let mut engine = Scripted::new();
    build_report(&job, &path, &mut engine, &options, |_, _| Ok(())).unwrap();
    assert_eq!(
        engine.calls.iter().filter(|c| c.max_tokens == 900).count(),
        plan.len(),
        "un changement de modèle recalcule toutes les sections"
    );
    job.report_model_id = "baseline".into();

    // Transcription corrigée : tout est refait.
    job.segments[3].text = "Texte corrigé.".into();
    let mut engine = Scripted::new();
    build_report(&job, &path, &mut engine, &options, |_, _| Ok(())).unwrap();
    assert_eq!(
        engine.calls.iter().filter(|c| c.max_tokens == 900).count(),
        plan_sections(&job, 600).len()
    );

    let md = render_report_markdown(&report);
    for heading in [
        "# Réunion budget",
        "## Synthèse",
        "## Décisions",
        "## Actions",
        "## Questions ouvertes",
        "## Déroulé détaillé",
        "à relire avant diffusion",
        "| Marie | Envoyer le budget | vendredi | 00:00:00 |",
    ] {
        assert!(md.contains(heading), "absent : {heading}");
    }
}

// ---------------------------------------------------------------------------
// Moteur réel (llama.cpp CPU)
// ---------------------------------------------------------------------------

fn real_engine(dir: &std::path::Path) -> Option<LlamaCppEngine> {
    let (Some(bin), Some(model)) = (
        env::var_os("PAROLE_LLAMA_BIN"),
        env::var_os("PAROLE_LLM_MODEL"),
    ) else {
        eprintln!("Moteur réel non configuré (PAROLE_LLAMA_BIN, PAROLE_LLM_MODEL) : test ignoré");
        return None;
    };
    let engine = LlamaCppEngine::new(PathBuf::from(bin), PathBuf::from(model), dir.to_path_buf());
    engine
        .check(&MODELE_TEXTE_RECOMMANDE)
        .expect("modèle épinglé valide");
    Some(engine)
}

const REUNION_EN: &[&str] = &[
    "Okay, let's get started, we have a lot to cover today.",
    "First item is the budget for the new website.",
    "Marie, can you send the updated budget by Friday?",
    "Sure, I'll send it by Friday morning.",
    "Second item: the product launch planned for January.",
    "Honestly the team is not ready, testing is behind schedule.",
    "So we decided to postpone the launch to March.",
    "Paul will inform the client about the new date next week.",
    "One open question is whether we hire a second tester.",
    "We will discuss that at the next meeting. Thanks everyone.",
];

#[test]
fn moteur_reel_traduction_integrale_anglais_vers_francais() {
    let dir = tempfile::tempdir().unwrap();
    let Some(mut engine) = real_engine(dir.path()) else {
        return;
    };
    let mut job = job_with(REUNION_EN, "en");
    let state = dir.path().join("travail.json");
    let started = std::time::Instant::now();
    let summary = translate_job(
        &mut job,
        &state,
        &mut engine,
        &TranslationOptions::new(Language::French),
        |_, _| Ok(()),
    )
    .unwrap();
    eprintln!(
        "Traduction réelle en {:?} : {summary:?}\n{}",
        started.elapsed(),
        render_translated_txt(&job)
    );
    assert_eq!(summary.translated_now, REUNION_EN.len());
    let all: Vec<&str> = job
        .segments
        .iter()
        .map(|s| s.translated_text.as_deref().unwrap())
        .collect();
    assert!(all.iter().all(|t| !t.is_empty()));
    assert_eq!(guess_language(&all.join(" ")), Some(Language::French));
    assert!(all[2].contains("Marie") && all[2].to_lowercase().contains("vendredi"));
    assert!(
        all[4].to_lowercase().contains("lancement"),
        "le sens du lancement de produit doit être conservé"
    );
    assert!(all[6].to_lowercase().contains("mars"));
    let leftovers = dir.path().read_dir().unwrap().filter(|e| {
        e.as_ref()
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("llm-")
    });
    assert_eq!(leftovers.count(), 0, "fichiers d'invite nettoyés");
}

#[test]
fn moteur_reel_traduction_francais_vers_anglais() {
    let dir = tempfile::tempdir().unwrap();
    let Some(mut engine) = real_engine(dir.path()) else {
        return;
    };
    let mut job = job_with(
        &[
            "Bonjour à tous, merci d'être venus.",
            "Nous devons valider le budget du projet avant vendredi.",
            "Claire s'occupe de la présentation pour le comité de direction.",
        ],
        "fr",
    );
    translate_job(
        &mut job,
        &dir.path().join("t.json"),
        &mut engine,
        &TranslationOptions::new(Language::English),
        |_, _| Ok(()),
    )
    .unwrap();
    let text = render_translated_txt(&job);
    eprintln!("{text}");
    assert_eq!(guess_language(&text), Some(Language::English));
    assert!(text.contains("Friday") && text.contains("Claire"));
}

#[test]
fn moteur_reel_compte_rendu_direct_depuis_anglais() {
    let dir = tempfile::tempdir().unwrap();
    let Some(mut engine) = real_engine(dir.path()) else {
        return;
    };
    let job = job_with(REUNION_EN, "en");
    let mut options = ReportOptions::new(Language::French);
    options.use_translation = false;
    options.section_chars = 360;
    options.synthesis_chars = 600;
    let report = build_report(
        &job,
        &dir.path().join("direct.json"),
        &mut engine,
        &options,
        |_, _| Ok(()),
    )
    .unwrap();
    let md = parole_core::verified_report::render_synthesis_with_sources(&job, &report);
    assert!(md.contains("## Synthèse\n\n"));
    assert!(md.contains("## Passages sources\n\n"));
    if let Some(out) = env::var_os("PAROLE_REPORT_OUT_DIRECT") {
        fs::write(out, md).unwrap();
    }
}

/// Chemin recommandé : traduction intégrale puis compte rendu depuis la traduction.
#[test]
fn moteur_reel_compte_rendu_francais_depuis_reunion_anglaise() {
    let dir = tempfile::tempdir().unwrap();
    let Some(mut engine) = real_engine(dir.path()) else {
        return;
    };
    let mut job = job_with(REUNION_EN, "en");
    let started = std::time::Instant::now();
    translate_job(
        &mut job,
        &dir.path().join("travail.json"),
        &mut engine,
        &TranslationOptions::new(Language::French),
        |_, _| Ok(()),
    )
    .unwrap();
    job.target_language = Some("fr".into());
    let mut options = ReportOptions::new(Language::French);
    options.use_translation = true;
    options.section_chars = 360; // force plusieurs passages + réduction
    options.synthesis_chars = 600;
    let report = build_report(
        &job,
        &dir.path().join("cr.json"),
        &mut engine,
        &options,
        |_, _| Ok(()),
    )
    .unwrap();
    let md = parole_core::verified_report::render_synthesis_with_sources(&job, &report);
    eprintln!(
        "Traduction + compte rendu synthétique sourcé en {:?}",
        started.elapsed()
    );
    for line in md.lines().filter(|line| line.starts_with("> [")) {
        assert!(job
            .segments
            .iter()
            .any(|s| s.translated_text.as_ref().is_some_and(|t| line.contains(t))));
    }
    assert!(report.sections.len() >= 2);
    assert!(md.contains("## Synthèse\n\n"));
    assert!(md.contains("## Passages sources\n\n"));
    assert!(md.contains("## Questions ouvertes\n\n"));
    let decision = md
        .lines()
        .find(|line| line.starts_with("- [00:00:30]"))
        .expect("décision citée au bon instant");
    assert!(
        decision.to_lowercase().contains("lancement"),
        "décision : {decision}"
    );
    let marie = md
        .lines()
        .find(|line| line.starts_with("| Marie |") && line.contains("00:00:10"))
        .expect("action de Marie citée au bon instant");
    assert!(marie.to_lowercase().contains("budget"), "action : {marie}");
    assert!(md.contains("> [00:00:10.000]"));
    assert!(md.contains("> [00:00:30.000]"));
    assert!(md.contains("- [00:00:40]"));
    assert!(!md.contains("- [00:00:10] Marie, peux-tu envoyer"));
    assert!(!md.contains("Synthèse non étayée"));
    assert!(!report.synthese.is_empty());
    assert_eq!(guess_language(&report.synthese), Some(Language::French));
    assert!(report
        .actions
        .iter()
        .any(|a| a.responsable == "Marie" || a.responsable == "Paul"));
    assert!(report
        .actions
        .iter()
        .all(|a| a.responsable == "non précisé" || REUNION_EN.join(" ").contains(&a.responsable)));
    if let Some(out) = env::var_os("PAROLE_REPORT_OUT") {
        fs::write(out, md).unwrap();
    }
}
