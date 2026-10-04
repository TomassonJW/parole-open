//! Vérification explicite sur des paroles fictives. Aucun média utilisateur.
use parole_core::{
    Segment,
    topic_questions::{CandidateChoice, PreparedTopicQuestions},
    topic_selection::{SelectionThresholds, select_topics},
};
use parole_gliclass::{Gliclass, ModelBundle, Outcome};
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 && !(args.len() == 4 && args[2] == "--preload-uncommitted") {
        return Err("Usage : fictional_smoke <descripteur-local.json>".into());
    }
    use std::io::Read;
    let file =
        std::fs::File::open(&args[1]).map_err(|_| "Le descripteur local est indisponible.")?;
    let mut bytes = Vec::new();
    file.take(16_385)
        .read_to_end(&mut bytes)
        .map_err(|_| "Le descripteur local ne peut pas être lu.")?;
    if bytes.len() > 16_384 {
        return Err("Le descripteur local est trop volumineux.".into());
    }
    let bundle: ModelBundle =
        serde_json::from_slice(&bytes).map_err(|_| "Le descripteur local est invalide.")?;
    if args.len() == 4 {
        use sha2::{Digest, Sha256};
        let file = std::fs::File::open(&args[3])
            .map_err(|_| "La bibliothèque de contre-essai est indisponible.")?;
        if file
            .metadata()
            .map_err(|_| "La bibliothèque de contre-essai est indisponible.")?
            .len()
            > 256 * 1024 * 1024
        {
            return Err("La bibliothèque de contre-essai est trop volumineuse.".into());
        }
        let bytes = std::fs::read(&args[3])
            .map_err(|_| "La bibliothèque de contre-essai ne peut pas être lue.")?;
        if format!("{:x}", Sha256::digest(&bytes)) != bundle.library.sha256 {
            return Err("L'empreinte du contre-essai ne correspond pas.".into());
        }
        let _uncommitted =
            ort::init_from(&args[3]).map_err(|_| "Le contre-essai natif ne peut pas démarrer.")?;
    }
    let mut engine = Gliclass::new(bundle)?;
    let id = "99999999-9999-4999-8999-999999999999";
    let source = vec![
        Segment::new(
            0,
            1_000,
            "Le projet Atlas prépare le budget et le calendrier.".into(),
        ),
        Segment::new(
            1_100,
            2_000,
            "Pour ce budget, gardons une marge avant la prochaine réunion.".into(),
        ),
        Segment::new(
            2_100,
            3_500,
            "Le dossier Luciole attend les illustrations.".into(),
        ),
    ];
    let choices = vec![
        CandidateChoice::WordInPossibleFolder {
            term: "budget".into(),
            folder: "Atlas".into(),
        },
        CandidateChoice::WordInPossibleFolder {
            term: "calendrier".into(),
            folder: "Atlas".into(),
        },
        CandidateChoice::WordInPossibleFolder {
            term: "illustrations".into(),
            folder: "Luciole".into(),
        },
    ];
    let prepared = PreparedTopicQuestions::prepare(id, &source)?;
    assert!(matches!(
        engine.classify(&prepared, 1, &[])?,
        Outcome::NoCandidates { .. }
    ));
    assert!(!engine.is_loaded());
    assert_eq!(engine.inference_count(), 0);
    let mut records = Vec::new();
    for (name, ordered) in [
        ("paired", choices.clone()),
        (
            "reordered",
            vec![choices[2].clone(), choices[0].clone(), choices[1].clone()],
        ),
    ] {
        let Outcome::Scored(result) = engine.classify(&prepared, 1, &ordered)? else {
            return Err("Les scores locaux sont absents.".into());
        };
        assert_eq!(
            serde_json::to_value(result.question()).unwrap(),
            serde_json::to_value(prepared.question(1, &ordered)?).unwrap()
        );
        let ids = result
            .question()
            .candidates
            .iter()
            .map(|c| c.candidate_id.clone())
            .collect::<Vec<_>>();
        let selected = select_topics(
            SelectionThresholds {
                uncertain_from: 0.5,
                proposed_from: 0.9,
            },
            &ids,
            result.scores(),
        )
        .map_err(|e| e.to_string())?;
        records.push(serde_json::json!({"name":name,"question":result.question(),"identity":result.identity(),
            "input_ids":result.input_ids(),"attention_mask":result.attention_mask(),"logits":result.logits(),"selection":selected}));
    }
    assert!(engine.is_loaded());
    assert_eq!(engine.inference_count(), 2);
    assert!(matches!(
        engine.classify(&prepared, 1, &[])?,
        Outcome::NoCandidates { .. }
    ));
    assert_eq!(engine.inference_count(), 2);
    println!(
        "{}",
        serde_json::json!({"cases":records,"inference_count":engine.inference_count(),"thresholds_are_illustrative":true})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
