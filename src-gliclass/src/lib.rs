//! Classifieur thématique strictement local.
mod artifacts;
mod binding;
mod encoding;
pub mod retained;
mod worker_protocol;
pub mod worker_service;
use parole_core::{
    topic_questions::{CandidateChoice, PreparedTopicQuestions, TopicQuestion},
    topic_selection::CandidateScore,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelBundle {
    pub model_id: String,
    pub model_revision: String,
    pub weights: Artifact,
    pub tokenizer: Artifact,
    pub library: Artifact,
}
#[derive(Debug)]
pub enum Outcome {
    NoCandidates { question: Box<TopicQuestion> },
    Scored(Box<ScoredTopics>),
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RuntimeIdentity {
    pub model_id: String,
    pub model_revision: String,
    pub weights_sha256: String,
    pub tokenizer_sha256: String,
    pub runtime_sha256: String,
    pub engine_sha256: String,
    pub options_sha256: String,
}
#[derive(Debug)]
pub struct ScoredTopics {
    question: TopicQuestion,
    identity: RuntimeIdentity,
    scores: Vec<CandidateScore>,
    logits: Vec<f32>,
    input_ids: Vec<i64>,
    attention_mask: Vec<i64>,
}
impl ScoredTopics {
    pub fn question(&self) -> &TopicQuestion {
        &self.question
    }
    pub fn identity(&self) -> &RuntimeIdentity {
        &self.identity
    }
    pub fn scores(&self) -> &[CandidateScore] {
        &self.scores
    }
    pub fn logits(&self) -> &[f32] {
        &self.logits
    }
    pub fn input_ids(&self) -> &[i64] {
        &self.input_ids
    }
    pub fn attention_mask(&self) -> &[i64] {
        &self.attention_mask
    }
}
fn scores_from_logits(
    question: &TopicQuestion,
    shape: &[i64],
    logits: &[f32],
) -> Result<Vec<CandidateScore>, String> {
    let count = question.candidates.len();
    if count == 0
        || count > 25
        || (shape != [1, count as i64] && shape != [1, 25])
        || logits.len() != shape[1] as usize
        || logits.iter().any(|v| !v.is_finite())
    {
        return Err("Les sorties du modèle ne correspondent pas aux sujets demandés.".into());
    }
    Ok(question
        .candidates
        .iter()
        .zip(logits)
        .map(|(candidate, &logit)| {
            let value = f64::from(logit);
            let score = if value >= 0.0 {
                1.0 / (1.0 + (-value).exp())
            } else {
                let e = value.exp();
                e / (1.0 + e)
            };
            CandidateScore {
                candidate_id: candidate.candidate_id.clone(),
                score,
            }
        })
        .collect())
}
struct Live {
    session: ort::session::Session,
    identity: RuntimeIdentity,
    _library: std::fs::File,
}
static NATIVE_OWNER: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
pub struct Gliclass {
    bundle: ModelBundle,
    tokenizer: Option<tokenizers::Tokenizer>,
    live: Option<Live>,
    runs: u64,
}
fn execution_profile() -> serde_json::Value {
    let mut features: Vec<&str> = Vec::new();
    #[cfg(target_arch = "x86_64")]
    {
        for (name, present) in [
            ("sse4.1", std::is_x86_feature_detected!("sse4.1")),
            ("avx", std::is_x86_feature_detected!("avx")),
            ("avx2", std::is_x86_feature_detected!("avx2")),
            ("avx512f", std::is_x86_feature_detected!("avx512f")),
            ("fma", std::is_x86_feature_detected!("fma")),
        ] {
            if present {
                features.push(name);
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        for (name, present) in [
            ("neon", std::arch::is_aarch64_feature_detected!("neon")),
            (
                "dotprod",
                std::arch::is_aarch64_feature_detected!("dotprod"),
            ),
            ("i8mm", std::arch::is_aarch64_feature_detected!("i8mm")),
        ] {
            if present {
                features.push(name);
            }
        }
    }
    serde_json::json!({"schema":"parole-gliclass-cpu-v1","provider":"CPU","intra_threads":2,"inter_threads":1,"parallel_execution":false,
        "graph_optimization":"Level3","telemetry":false,"truncation":false,"padding":false,"max_tokens":512,"max_labels":25,
        "activation":"sigmoid-f64-stable-v1","os":std::env::consts::OS,"architecture":std::env::consts::ARCH,"cpu_features":features})
}
impl RuntimeIdentity {
    fn expected(bundle: &ModelBundle) -> Result<Self, String> {
        Ok(Self {
            model_id: bundle.model_id.clone(),
            model_revision: bundle.model_revision.clone(),
            weights_sha256: bundle.weights.sha256.clone(),
            tokenizer_sha256: bundle.tokenizer.sha256.clone(),
            runtime_sha256: bundle.library.sha256.clone(),
            engine_sha256: artifacts::current_executable_sha256()?,
            options_sha256: parole_core::language::sha256_hex(
                &serde_json::to_vec(&execution_profile())
                    .map_err(|_| "Les options du moteur ne peuvent pas être conservées.")?,
            ),
        })
    }
}
impl Live {
    fn load(bundle: &ModelBundle) -> Result<Self, String> {
        use ort::{
            ep::CPUExecutionProvider,
            session::{Session, builder::GraphOptimizationLevel},
        };
        let weights = artifacts::read_verified(&bundle.weights, 2 * 1024 * 1024 * 1024)?;
        let library = artifacts::read_verified(&bundle.library, 256 * 1024 * 1024)?;
        let identity = RuntimeIdentity::expected(bundle)?;
        let mut owner = NATIVE_OWNER
            .lock()
            .map_err(|_| "Le moteur local doit être redémarré.")?;
        if *owner {
            return Err(
                "Une autre instance possède déjà le moteur local dans ce processus.".into(),
            );
        }
        *owner = true;
        #[cfg(target_os = "linux")]
        let library_path = {
            use std::os::fd::AsRawFd;
            std::path::PathBuf::from(format!("/proc/self/fd/{}", library.handle.as_raw_fd()))
        };
        #[cfg(not(target_os = "linux"))]
        let library_path = bundle.library.path.clone();
        let environment = ort::init_from(&library_path)
            .map_err(|_| "La bibliothèque locale du moteur ne peut pas être chargée.")?;
        if !artifacts::loaded_library_matches(
            &library.handle,
            ort::api().CreateEnv as *const std::ffi::c_void,
        )? {
            return Err("La bibliothèque active n'est pas celle qui a été vérifiée.".into());
        }
        if !environment
            .with_telemetry(false)
            .with_execution_providers([CPUExecutionProvider::default().build()])
            .commit()
        {
            return Err(
                "Une autre configuration possède déjà le moteur local dans ce processus.".into(),
            );
        }
        let session = Session::builder()
            .and_then(|b| b.with_execution_providers([CPUExecutionProvider::default().build()]))
            .and_then(|b| b.with_intra_threads(2))
            .and_then(|b| b.with_inter_threads(1))
            .and_then(|b| b.with_parallel_execution(false))
            .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
            .and_then(|b| b.commit_from_memory(&weights.bytes))
            .map_err(|_| "Le modèle local ne peut pas être chargé avec ces options.")?;
        Ok(Self {
            session,
            identity,
            _library: library.handle,
        })
    }
}
impl Gliclass {
    pub fn new(bundle: ModelBundle) -> Result<Self, String> {
        let valid_text = |s: &str| {
            !s.is_empty() && s.len() <= 256 && s.trim() == s && !s.chars().any(char::is_control)
        };
        if !valid_text(&bundle.model_id) || !valid_text(&bundle.model_revision) {
            return Err("L'identité du modèle local est invalide.".into());
        }
        for artifact in [&bundle.weights, &bundle.tokenizer, &bundle.library] {
            if artifact.sha256.len() != 64
                || !artifact
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !artifact.path.is_absolute()
                || artifact
                    .path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(
                    "Le fichier local doit avoir un chemin absolu et une empreinte valide.".into(),
                );
            }
        }
        Ok(Self {
            bundle,
            tokenizer: None,
            live: None,
            runs: 0,
        })
    }
    pub fn is_loaded(&self) -> bool {
        self.live.is_some()
    }
    pub fn inference_count(&self) -> u64 {
        self.runs
    }
    pub fn classify(
        &mut self,
        prepared: &PreparedTopicQuestions<'_>,
        target: usize,
        choices: &[CandidateChoice],
    ) -> Result<Outcome, String> {
        let question = prepared.question(target, choices)?;
        if question.candidates.is_empty() {
            return Ok(Outcome::NoCandidates {
                question: Box::new(question),
            });
        }
        if self.tokenizer.is_none() {
            let file = artifacts::read_verified(&self.bundle.tokenizer, 32 * 1024 * 1024)?;
            self.tokenizer = Some(
                tokenizers::Tokenizer::from_bytes(&file.bytes)
                    .map_err(|_| "Le tokenizer local ne peut pas être chargé.")?,
            );
        }
        let encoded = encoding::encode(
            self.tokenizer
                .as_ref()
                .ok_or("Le tokenizer local est indisponible.")?,
            &question,
        )?;
        if self.live.is_none() {
            self.live = Some(Live::load(&self.bundle)?);
        }
        let live = self
            .live
            .as_mut()
            .ok_or("Le moteur local est indisponible.")?;
        let length = encoded.input_ids.len();
        let ids = ort::value::Tensor::from_array(([1usize, length], encoded.input_ids.clone()))
            .map_err(|_| "Le passage ne peut pas être transmis au modèle local.")?;
        let mask =
            ort::value::Tensor::from_array(([1usize, length], encoded.attention_mask.clone()))
                .map_err(|_| "Le masque du passage est invalide.")?;
        self.runs = self
            .runs
            .checked_add(1)
            .ok_or("Le compteur de calculs est saturé.")?;
        let outputs = live
            .session
            .run(ort::inputs! {"input_ids"=>ids,"attention_mask"=>mask})
            .map_err(|_| "Le calcul local a échoué ; aucun classement partiel n'est conservé.")?;
        let output = outputs
            .get("logits")
            .ok_or("Le modèle local n'a pas fourni les sorties attendues.")?;
        let (shape, values) = output
            .try_extract_tensor::<f32>()
            .map_err(|_| "Les sorties du modèle local ont un format invalide.")?;
        let scores = scores_from_logits(&question, shape, values)?;
        let logits = values[..question.candidates.len()].to_vec();
        Ok(Outcome::Scored(Box::new(ScoredTopics {
            question,
            identity: live.identity.clone(),
            scores,
            logits,
            input_ids: encoded.input_ids,
            attention_mask: encoded.attention_mask,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parole_core::Segment;
    fn question() -> TopicQuestion {
        let source = [Segment::new(
            0,
            1000,
            "Le budget et le calendrier sont à préciser.".into(),
        )];
        PreparedTopicQuestions::prepare("99999999-9999-4999-8999-999999999999", &source)
            .unwrap()
            .question(
                0,
                &[
                    CandidateChoice::Word {
                        term: "budget".into(),
                    },
                    CandidateChoice::Word {
                        term: "calendrier".into(),
                    },
                ],
            )
            .unwrap()
    }
    #[test]
    fn malformed_nonfinite_or_partial_native_outputs_are_rejected_atomically() {
        let q = question();
        for (shape, values) in [
            (vec![1, 1], vec![0.0]),
            (vec![2, 2], vec![0.0; 4]),
            (vec![1, 2], vec![0.0]),
            (vec![], vec![]),
            (vec![1, 2], vec![0.0, f32::NAN]),
            (vec![1, 2], vec![f32::INFINITY, 0.0]),
        ] {
            assert!(scores_from_logits(&q, &shape, &values).is_err());
        }
        let mut values = vec![0.0; 25];
        values[24] = f32::NAN;
        assert!(scores_from_logits(&q, &[1, 25], &values).is_err());
    }
    #[test]
    fn all_scores_are_linked_to_original_ids_without_selection_or_confirmation() {
        let q = question();
        let scores = scores_from_logits(&q, &[1, 2], &[0.0, 1000.0]).unwrap();
        assert_eq!(scores.len(), 2);
        assert_eq!(scores[0].score, 0.5);
        assert_eq!(scores[1].score, 1.0);
        assert_eq!(scores[0].candidate_id, q.candidates[0].candidate_id);
        assert_eq!(scores[1].candidate_id, q.candidates[1].candidate_id);
        let padded = vec![-1000.0; 25];
        let scores = scores_from_logits(&q, &[1, 25], &padded).unwrap();
        assert_eq!(scores.len(), 2);
        assert!(scores.iter().all(|s| s.score == 0.0));
    }
}
