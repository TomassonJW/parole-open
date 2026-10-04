//! Comparaison facultative de deux vrais modèles locaux sur la même réunion inventée.
//! Aucune donnée utilisateur, aucun réseau et aucun modèle téléchargé par ce test.
//! PAROLE_COMPARE_BASELINE, PAROLE_COMPARE_CANDIDATE, PAROLE_COMPARE_BIN
//! et PAROLE_COMPARE_OUT doivent être définis explicitement.
use parole_core::language::{build_report, plan_sections, Language, LlamaCppEngine, ReportOptions};
use parole_core::report_models::report_model;
use parole_core::verified_report::render_synthesis_with_sources;
use parole_core::{Job, Segment};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    env, fs,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "Nécessite deux modèles locaux déjà vérifiés et un dossier de sortie explicite"]
fn deux_modeles_locaux_sur_la_meme_transcription_synthetique() {
    let baseline = env::var("PAROLE_COMPARE_BASELINE").expect("PAROLE_COMPARE_BASELINE requis");
    let candidate = env::var("PAROLE_COMPARE_CANDIDATE").expect("PAROLE_COMPARE_CANDIDATE requis");
    let binary = env::var("PAROLE_COMPARE_BIN").expect("PAROLE_COMPARE_BIN requis");
    let parent = PathBuf::from(env::var("PAROLE_COMPARE_OUT").expect("PAROLE_COMPARE_OUT requis"));
    fs::create_dir_all(&parent).unwrap();
    // Ne jamais considérer un rapport mis en cache comme une nouvelle inférence.
    let run = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output = parent.join(format!("comparaison-{run}-{}", std::process::id()));
    fs::create_dir(&output).expect("dossier de comparaison neuf requis");

    // Dialogue entièrement fictif : proposition initiale, décision ultérieure,
    // tâche datée et question ouverte. Même entrée et mêmes options pour les deux modèles.
    let turns = [
        ("S1", "Bonjour. Pour le site Atlas, je propose un lancement en janvier, mais aucune date n'est validée pour l'instant."),
        ("S2", "Les tests de sécurité ne sont pas terminés. Je ne peux pas approuver janvier."),
        ("S3", "Le budget actuel est de douze mille euros. Une augmentation n'a pas été approuvée."),
        ("S1", "Après discussion, nous décidons de reporter le lancement du site Atlas au mois de mars."),
        ("S2", "Je prendrai en charge la vérification de sécurité avant le quinze février."),
        ("S3", "Je préparerai le budget actualisé et l'enverrai à Marie vendredi."),
        ("S1", "Paul informera le client du nouveau calendrier mardi prochain."),
        ("S2", "Le recrutement d'un deuxième testeur reste une question ouverte, personne ne l'a décidé."),
        ("S3", "Nous avons confirmé le report à mars, pas une augmentation du budget."),
    ];
    let mut job = Job::new(
        "reunion-fictive.wav".into(),
        turns.len() as u64 * 10_000,
        30_000,
    );
    job.source_language = "fr".into();
    for (id, name) in [("S1", "Marie"), ("S2", "Paul"), ("S3", "Claire")] {
        job.speaker_names.insert(id.into(), name.into());
    }
    for (index, (speaker, text)) in turns.iter().enumerate() {
        let mut segment = Segment::new(
            index as u64 * 10_000,
            (index as u64 + 1) * 10_000,
            (*text).into(),
        );
        segment.speaker_id = Some((*speaker).into());
        job.segments.push(segment);
    }
    let options = ReportOptions::new(Language::French);

    for (id, path) in [
        ("baseline", baseline),
        ("qwen3-4b-instruct-2507-q4_k_m", candidate),
    ] {
        let model = report_model(id).unwrap();
        let mut engine =
            LlamaCppEngine::new(PathBuf::from(&binary), PathBuf::from(path), output.clone());
        engine
            .check(&model.spec())
            .expect("taille et empreinte du modèle épinglé");
        let mut trial = job.clone();
        trial.report_model_id = id.into();
        let started = Instant::now();
        let report = build_report(
            &trial,
            &output.join(format!("state-{id}.json")),
            &mut engine,
            &options,
            |_, _| Ok(()),
        )
        .expect("compte rendu par le moteur local");
        let text = render_synthesis_with_sources(&trial, &report);
        let result = output.join(format!("rapport-{id}.md"));
        fs::write(&result, &text).unwrap();
        eprintln!(
            "{id}: {:.1}s, rapport {}",
            started.elapsed().as_secs_f64(),
            result.display()
        );
        assert!(text.contains("## Passages sources\n\n"));
        assert!(!report.sections.is_empty());
        assert!(!report.synthese.trim().is_empty());
    }
}

#[derive(Deserialize)]
struct ComplexTurn {
    speaker: String,
    text: String,
}

#[derive(Deserialize)]
struct ComplexFixture {
    title: String,
    source_language: String,
    speakers: BTreeMap<String, String>,
    expected: BTreeMap<String, String>,
    turns: Vec<ComplexTurn>,
}

fn complex_fixture() -> (ComplexFixture, Job) {
    let fixture: ComplexFixture =
        serde_json::from_str(include_str!("fixtures/reunion-orion-complexe.json")).unwrap();
    let mut job = Job::new(
        "reunion-orion-fictive.wav".into(),
        fixture.turns.len() as u64 * 10_000,
        30_000,
    );
    job.source_language = fixture.source_language.clone();
    job.speaker_names = fixture.speakers.clone();
    for (index, turn) in fixture.turns.iter().enumerate() {
        let mut segment = Segment::new(
            index as u64 * 10_000,
            (index as u64 + 1) * 10_000,
            turn.text.clone(),
        );
        segment.speaker_id = Some(turn.speaker.clone());
        job.segments.push(segment);
    }
    (fixture, job)
}

#[test]
fn reunion_complexe_contient_des_corrections_et_plusieurs_passages() {
    let (fixture, job) = complex_fixture();
    assert_eq!(fixture.speakers.len(), 3);
    assert_eq!(fixture.expected.len(), 5);
    assert!(fixture.title.contains("Orion"));
    assert!(plan_sections(&job, ReportOptions::new(Language::French).section_chars).len() >= 2);
    let text = fixture
        .turns
        .iter()
        .map(|turn| turn.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("19 avril") && text.contains("26 avril"));
    assert!(text.contains("annulons") && text.contains("question ouverte"));
    assert!(text.contains("seize mille") && text.contains("dix-huit mille"));
}

#[test]
#[ignore = "Nécessite deux modèles locaux déjà vérifiés et un dossier de sortie explicite"]
fn deux_modeles_locaux_sur_une_reunion_complexe_avec_rectifications() {
    let baseline = env::var("PAROLE_COMPARE_BASELINE").expect("PAROLE_COMPARE_BASELINE requis");
    let candidate = env::var("PAROLE_COMPARE_CANDIDATE").expect("PAROLE_COMPARE_CANDIDATE requis");
    let binary = env::var("PAROLE_COMPARE_BIN").expect("PAROLE_COMPARE_BIN requis");
    let stress = match env::var("PAROLE_COMPARE_STRESS") {
        Ok(value) if value == "1" => true,
        Err(env::VarError::NotPresent) => false,
        _ => panic!("PAROLE_COMPARE_STRESS doit valoir 1 ou être absent"),
    };
    let mut options = ReportOptions::new(Language::French);
    if stress {
        // Exerce plusieurs niveaux de réduction, sans prétendre simuler une
        // réunion de cent mille mots ni les réglages par défaut de production.
        options.section_chars = 900;
        options.synthesis_chars = 1_800;
    }
    let label = if stress { "orion-stress" } else { "orion" };
    let parent = PathBuf::from(env::var("PAROLE_COMPARE_OUT").expect("PAROLE_COMPARE_OUT requis"));
    fs::create_dir_all(&parent).unwrap();
    let run = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output = parent.join(format!("{label}-{run}-{}", std::process::id()));
    fs::create_dir(&output).expect("dossier de comparaison neuf requis");
    fs::write(
        output.join("corpus-et-attendus.json"),
        include_str!("fixtures/reunion-orion-complexe.json"),
    )
    .unwrap();
    let (fixture, job) = complex_fixture();
    let section_count = plan_sections(&job, options.section_chars).len();
    eprintln!(
        "Cas : {} - {} prises de parole, {} sections prévues, stress={stress}",
        fixture.title,
        job.segments.len(),
        section_count
    );
    assert!(section_count >= if stress { 4 } else { 2 });
    for (id, path) in [
        ("baseline", baseline),
        ("qwen3-4b-instruct-2507-q4_k_m", candidate),
    ] {
        let model = report_model(id).unwrap();
        let mut engine =
            LlamaCppEngine::new(PathBuf::from(&binary), PathBuf::from(path), output.clone());
        engine
            .check(&model.spec())
            .expect("taille et empreinte du modèle épinglé");
        let mut trial = job.clone();
        trial.report_model_id = id.into();
        let started = Instant::now();
        let report = build_report(
            &trial,
            &output.join(format!("state-{id}.json")),
            &mut engine,
            &options,
            |_, _| Ok(()),
        )
        .expect("compte rendu par le moteur local");
        let text = render_synthesis_with_sources(&trial, &report);
        let result = output.join(format!("rapport-{id}.md"));
        fs::write(&result, &text).unwrap();
        eprintln!(
            "{id}: {:.1}s, {} sections, rapport {}",
            started.elapsed().as_secs_f64(),
            report.sections.len(),
            result.display()
        );
        assert!(
            report.sections.len() >= 2,
            "le test doit réellement traverser deux passages"
        );
        assert!(text.contains("## Passages sources\n\n"));
        assert!(!report.synthese.trim().is_empty());
    }
}
