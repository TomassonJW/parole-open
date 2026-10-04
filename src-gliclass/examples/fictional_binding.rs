//! Preuve locale sur paroles fictives uniquement, pas une commande produit.
use parole_core::{
    Job, Segment,
    topic_classification_access::{ClassificationLibrary, ClassificationRequest},
    topic_classification_cache::EngineIdentity,
    topic_questions::{CandidateChoice, PreparedTopicQuestions},
    topic_selection::SelectionThresholds,
};
use parole_gliclass::{Gliclass, ModelBundle, Outcome};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path};

const ID: &str = "12121212-1212-4212-8212-121212121212";
const TAG: &str = "parole-fictional-binding-v1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCase {
    snapshot_json: String,
    candidate_ids: Vec<String>,
    score_bits: Vec<u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    tag: String,
    engine: EngineIdentity,
    cases: Vec<StoredCase>,
    job_bytes: Vec<u8>,
}
fn bounded<T: serde::de::DeserializeOwned>(path: &Path, maximum: usize) -> Result<T, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > maximum {
        return Err("Fichier de preuve trop volumineux.".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
fn fictional_job() -> Job {
    let mut job = Job::new("paroles-fictives.wav".into(), 3000, 3000);
    job.segments = vec![
        Segment::new(0, 1000, "Le projet Atlas prépare le budget.".into()),
        Segment::new(
            1000,
            2000,
            "Le calendrier du projet Atlas reste à discuter.".into(),
        ),
        Segment::new(
            2000,
            3000,
            "Le dossier Luciole conserve les illustrations.".into(),
        ),
    ];
    job
}
fn choices(index: usize) -> Vec<CandidateChoice> {
    let mut choices = vec![
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
    if index == 1 {
        choices.reverse();
    }
    if index == 2 {
        choices.clear();
    }
    choices
}
fn request(
    job: &Job,
    engine: EngineIdentity,
    index: usize,
) -> Result<ClassificationRequest, String> {
    Ok(ClassificationRequest {
        job_id: ID.into(),
        source_revision: PreparedTopicQuestions::prepare(ID, &job.segments)?
            .question(1, &[])?
            .source_revision,
        target: 1,
        choices: choices(index),
        engine,
        // Seuils de démonstration seulement, aucune calibration de production.
        thresholds: SelectionThresholds {
            uncertain_from: 0.5,
            proposed_from: 0.9,
        },
    })
}
fn run() -> Result<Value, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 || !matches!(args[0].as_str(), "infer" | "read") {
        return Err(
            "Arguments : infer|read racine-de-preuve paquet-json (ignoré en lecture).".into(),
        );
    }
    let root = Path::new(&args[1]);
    if !root.is_absolute()
        || root
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Racine absolue sans détour requise.".into());
    }
    if args[0] == "read" {
        // Aucun Gliclass construit et aucun paquet modèle ouvert dans ce chemin.
        let state: State = bounded(&root.join("fixture-state.json"), 1024 * 1024)?;
        if state.tag != TAG || state.cases.len() != 3 {
            return Err("Preuve fictive incorrecte.".into());
        }
        let job: Job = serde_json::from_slice(&state.job_bytes).map_err(|e| e.to_string())?;
        let library = ClassificationLibrary::open(root)?;
        let mut snapshots = Vec::new();
        for (index, expected) in state.cases.iter().enumerate() {
            let loaded = library.load(&request(&job, state.engine.clone(), index)?)?;
            let text = serde_json::to_string(&loaded).map_err(|e| e.to_string())?;
            let record = loaded.record.as_ref().ok_or("Classement absent.")?;
            if text != expected.snapshot_json
                || record
                    .selection()
                    .assessments
                    .iter()
                    .map(|s| s.score.to_bits())
                    .collect::<Vec<_>>()
                    != expected.score_bits
                || record
                    .selection()
                    .assessments
                    .iter()
                    .map(|s| s.candidate_id.clone())
                    .collect::<Vec<_>>()
                    != expected.candidate_ids
            {
                return Err("La relecture a modifié un résultat.".into());
            }
            snapshots.push(serde_json::to_value(loaded).map_err(|e| e.to_string())?);
        }
        if fs::read(root.join(ID).join("travail.json")).map_err(|e| e.to_string())?
            != state.job_bytes
        {
            return Err("La consultation a modifié le travail.".into());
        }
        return Ok(
            json!({"mode":"read","tag":TAG,"model_constructed":false,"inferences":0,"snapshots":snapshots}),
        );
    }
    let bundle: ModelBundle = bounded(Path::new(&args[2]), 16_384)?;
    // Une preuve neuve uniquement : ne jamais réutiliser/modifier un dossier utilisateur.
    fs::create_dir(root).map_err(|e| e.to_string())?;
    fs::create_dir(root.join(ID)).map_err(|e| e.to_string())?;
    let job = fictional_job();
    let job_bytes = serde_json::to_vec(&job).map_err(|e| e.to_string())?;
    let job_path = root.join(ID).join("travail.json");
    fs::write(&job_path, &job_bytes).map_err(|e| e.to_string())?;
    fs::write(root.join(ID).join("audio.wav"), b"audio fictif conserve")
        .map_err(|e| e.to_string())?;
    fs::write(
        root.join(ID).join("annotations.json"),
        b"annotation humaine fictive",
    )
    .map_err(|e| e.to_string())?;
    let mut model = Gliclass::new(bundle)?;
    let expected_engine = model.expected_cache_identity()?;
    if model.is_loaded() || model.inference_count() != 0 {
        return Err("Chargement prématuré.".into());
    }
    let library = ClassificationLibrary::open(root)?;
    let mut cases = Vec::new();
    let mut snapshots = Vec::new();
    let mut raw_native = Vec::new();
    for index in 0..3 {
        let req = request(&job, expected_engine.clone(), index)?;
        if library.load(&req)?.record.is_some() {
            return Err("La preuve n'est pas neuve.".into());
        }
        let work = library.prepare(&req)?;
        let response = if index == 0 {
            let prepared = work.prepared_questions()?;
            let outcome = model.classify(&prepared, work.target(), work.choices())?;
            let Outcome::Scored(scored) = &outcome else {
                return Err("Résultat natif absent.".into());
            };
            if scored.identity().cache_identity() != expected_engine {
                return Err("Identité physique inattendue.".into());
            }
            let bound = outcome.response_for(&work)?;
            if bound.scores != scored.scores() {
                return Err("La liaison a modifié les scores natifs.".into());
            }
            raw_native.push(json!({"question":scored.question(),"identity":scored.identity(),"logits":scored.logits(),"input_ids":scored.input_ids(),"attention_mask":scored.attention_mask(),"score_bits":scored.scores().iter().map(|s|s.score.to_bits()).collect::<Vec<_>>()}));
            bound
        } else {
            model.classify_prepared(&work)?
        };
        let expected_count = if index == 0 { 1 } else { 2 };
        if model.inference_count() != expected_count {
            return Err("Nombre d'inférences incorrect.".into());
        }
        let snapshot = library.save(&work, &response)?;
        let record = snapshot.record.as_ref().ok_or("Classement non conservé.")?;
        let score_bits: Vec<_> = response.scores.iter().map(|s| s.score.to_bits()).collect();
        let ids: Vec<_> = response
            .scores
            .iter()
            .map(|s| s.candidate_id.clone())
            .collect();
        if record
            .selection()
            .assessments
            .iter()
            .map(|s| s.score.to_bits())
            .collect::<Vec<_>>()
            != score_bits
            || record
                .selection()
                .assessments
                .iter()
                .map(|s| s.candidate_id.clone())
                .collect::<Vec<_>>()
                != ids
        {
            return Err("La conservation a modifié les scores.".into());
        }
        cases.push(StoredCase {
            snapshot_json: serde_json::to_string(&snapshot).map_err(|e| e.to_string())?,
            candidate_ids: ids,
            score_bits,
        });
        snapshots.push(serde_json::to_value(&snapshot).map_err(|e| e.to_string())?);
        if index == 0 {
            let mut changed = job.clone();
            changed.segments[0].text.push('!');
            fs::write(
                &job_path,
                serde_json::to_vec(&changed).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let stale_rejected = library.save(&work, &response).is_err();
            fs::write(&job_path, &job_bytes).map_err(|e| e.to_string())?;
            if !stale_rejected {
                return Err("Résultat périmé accepté.".into());
            }
        }
    }
    if fs::read(&job_path).map_err(|e| e.to_string())? != job_bytes
        || fs::read(root.join(ID).join("audio.wav")).map_err(|e| e.to_string())?
            != b"audio fictif conserve"
        || fs::read(root.join(ID).join("annotations.json")).map_err(|e| e.to_string())?
            != b"annotation humaine fictive"
    {
        return Err("Un contenu indépendant a été modifié.".into());
    }
    let state = State {
        tag: TAG.into(),
        engine: expected_engine,
        cases,
        job_bytes,
    };
    fs::write(
        root.join("fixture-state.json"),
        serde_json::to_vec(&state).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(
        json!({"mode":"infer","tag":TAG,"inferences":model.inference_count(),"snapshots":snapshots,"raw_native":raw_native,"stale_result_refused":true,"job_audio_annotations_preserved":true}),
    )
}
fn main() {
    match run() {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("JSON fictif")
        ),
        Err(error) => {
            eprintln!("Échec de la preuve fictive : {error}");
            std::process::exit(1);
        }
    }
}
