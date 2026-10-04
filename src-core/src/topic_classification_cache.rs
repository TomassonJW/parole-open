//! Conservation locale dérivée : ne produit jamais de confirmation humaine.
use crate::{
    topic_questions::{CandidateChoice, PreparedTopicQuestions, TopicQuestion},
    topic_selection::{CandidateScore, SelectionThresholds, TopicSelection},
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
const POLICY_REVISION: u32 = 1;
const INVALID: &str = "Classement enregistré indisponible ou non vérifiable.";
pub const MAX_CLASSIFICATION_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineIdentity {
    pub model_id: String,
    pub model_revision: String,
    pub weights_sha256: String,
    pub tokenizer_sha256: String,
    pub runtime_sha256: String,
    pub engine_sha256: String,
    pub options_sha256: String,
}

impl EngineIdentity {
    /// Empreinte canonique des métadonnées validées ; ne lit aucun artefact.
    pub fn revision(&self) -> Result<String, String> {
        validate_engine(self)?;
        digest(self)
    }
}

pub struct ClassificationPlan {
    question: TopicQuestion,
    engine: EngineIdentity,
    engine_revision: String,
    thresholds: SelectionThresholds,
    cache_key: String,
}
impl ClassificationPlan {
    pub fn new(
        prepared: &PreparedTopicQuestions<'_>,
        target: usize,
        choices: &[CandidateChoice],
        engine: EngineIdentity,
        thresholds: SelectionThresholds,
    ) -> Result<Self, String> {
        let engine_revision = engine.revision()?;
        crate::topic_selection::select_topics(thresholds, &[], &[]).map_err(|e| e.to_string())?;
        let question = prepared.question(target, choices)?;
        let cache_key = digest(&(
            "parole-topic-classification-v1",
            POLICY_REVISION,
            &question,
            &engine,
            [
                thresholds.uncertain_from.to_bits(),
                thresholds.proposed_from.to_bits(),
            ],
        ))?;
        Ok(Self {
            question,
            engine,
            engine_revision,
            thresholds,
            cache_key,
        })
    }
    pub fn question(&self) -> &TopicQuestion {
        &self.question
    }
    pub fn engine_revision(&self) -> &str {
        &self.engine_revision
    }
    pub fn cache_key(&self) -> &str {
        &self.cache_key
    }
}

pub struct ClassificationResponse {
    pub question_revision: String,
    pub engine_revision: String,
    pub scores: Vec<CandidateScore>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ClassificationRecord {
    cache_key: String,
    question: TopicQuestion,
    engine: EngineIdentity,
    selection: TopicSelection,
}
impl ClassificationRecord {
    pub fn question(&self) -> &TopicQuestion {
        &self.question
    }
    pub fn selection(&self) -> &TopicSelection {
        &self.selection
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredScore {
    candidate_id: String,
    score_bits: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    cache_key: String,
    question: serde_json::Value,
    engine: EngineIdentity,
    threshold_bits: [u64; 2],
    scores: Vec<StoredScore>,
    summary: serde_json::Value,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    schema_version: u32,
    policy_revision: u32,
    payload: Payload,
    payload_sha256: String,
}
pub struct ClassificationCache {
    directory: PathBuf,
}
impl ClassificationCache {
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    fn validate_path(&self) -> Result<(), String> {
        if !self.directory.is_absolute()
            || self
                .directory
                .file_name()
                .and_then(|s| s.to_str())
                .is_none()
            || self
                .directory
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(INVALID.into());
        }
        Ok(())
    }
    fn file(&self, plan: &ClassificationPlan) -> PathBuf {
        self.directory
            .join(format!("classification-{}.json", plan.cache_key))
    }
    fn open(&self) -> Result<crate::audio_access_fs::Directory, String> {
        if !self.directory.is_absolute() {
            return Err(INVALID.into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if fs::symlink_metadata(&self.directory)
                .map_err(|_| INVALID)?
                .permissions()
                .mode()
                & 0o022
                != 0
            {
                return Err(INVALID.into());
            }
        }
        let parent = self.directory.parent().ok_or(INVALID)?;
        let name = self
            .directory
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(INVALID)?;
        crate::audio_access_fs::Directory::open(parent)
            .and_then(|p| p.child(name))
            .map_err(|_| INVALID.into())
    }
    pub fn load(&self, plan: &ClassificationPlan) -> Result<Option<ClassificationRecord>, String> {
        self.validate_path()?;
        let _parent = match crate::audio_access_fs::Directory::open(
            self.directory.parent().ok_or(INVALID)?,
        ) {
            Ok(parent) => parent,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(INVALID.into()),
        };
        match fs::symlink_metadata(&self.directory) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(INVALID.into()),
            Ok(_) => (),
        }
        let folder = self.open()?;
        let file = self.file(plan);
        match fs::symlink_metadata(&file) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(INVALID.into()),
            Ok(_) => (),
        }
        let bytes = folder
            .read(
                file.file_name().and_then(|s| s.to_str()).ok_or(INVALID)?,
                MAX_CLASSIFICATION_BYTES,
            )
            .map_err(|_| INVALID)?;
        let saved: Saved = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
        if saved.schema_version != 1
            || saved.policy_revision != POLICY_REVISION
            || saved.payload.cache_key != plan.cache_key
            || saved.payload.question
                != serde_json::to_value(&plan.question).map_err(|_| INVALID)?
            || saved.payload.engine != plan.engine
            || saved.payload.threshold_bits
                != [
                    plan.thresholds.uncertain_from.to_bits(),
                    plan.thresholds.proposed_from.to_bits(),
                ]
            || saved.payload_sha256 != digest(&saved.payload)?
            || canonical_bytes(&saved)? != bytes
        {
            return Err(INVALID.into());
        }
        let scores = saved
            .payload
            .scores
            .iter()
            .map(|s| CandidateScore {
                candidate_id: s.candidate_id.clone(),
                score: f64::from_bits(s.score_bits),
            })
            .collect::<Vec<_>>();
        let record = make_record(plan, &scores)?;
        if selection_summary(&record.selection)? != saved.payload.summary {
            return Err(INVALID.into());
        }
        Ok(Some(record))
    }
    pub fn save(
        &self,
        plan: &ClassificationPlan,
        response: &ClassificationResponse,
    ) -> Result<ClassificationRecord, String> {
        self.save_with(plan, response, |file, bytes| {
            crate::audio_durability::atomic_write(file, bytes)
        })
    }
    fn save_with(
        &self,
        plan: &ClassificationPlan,
        response: &ClassificationResponse,
        write: impl FnOnce(&Path, &[u8]) -> io::Result<()>,
    ) -> Result<ClassificationRecord, String> {
        self.validate_path()?;
        if response.question_revision != plan.question.request_revision
            || response.engine_revision != plan.engine_revision
        {
            return Err("La réponse ne correspond pas à la question et au moteur demandés.".into());
        }
        let record = make_record(plan, &response.scores)?;
        if let Some(existing) = self.load(plan)? {
            if existing
                .selection
                .assessments
                .iter()
                .zip(&response.scores)
                .all(|(a, b)| a.score.to_bits() == b.score.to_bits())
            {
                return Ok(existing);
            }
            return Err(
                "Un classement différent est déjà conservé pour cette même demande.".into(),
            );
        }
        let payload = Payload {
            cache_key: plan.cache_key.clone(),
            question: serde_json::to_value(&plan.question).map_err(|_| INVALID)?,
            engine: plan.engine.clone(),
            threshold_bits: [
                plan.thresholds.uncertain_from.to_bits(),
                plan.thresholds.proposed_from.to_bits(),
            ],
            scores: response
                .scores
                .iter()
                .map(|s| StoredScore {
                    candidate_id: s.candidate_id.clone(),
                    score_bits: s.score.to_bits(),
                })
                .collect(),
            summary: selection_summary(&record.selection)?,
        };
        let payload_sha256 = digest(&payload)?;
        let bytes = canonical_bytes(&Saved {
            schema_version: 1,
            policy_revision: POLICY_REVISION,
            payload,
            payload_sha256,
        })?;
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&self.directory) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(_) => return Err(INVALID.into()),
        }
        let _folder = self.open()?;
        write(&self.file(plan), &bytes).map_err(|_| INVALID)?;
        Ok(record)
    }
}
fn make_record(
    plan: &ClassificationPlan,
    scores: &[CandidateScore],
) -> Result<ClassificationRecord, String> {
    let ids = plan
        .question
        .candidates
        .iter()
        .map(|c| c.candidate_id.clone())
        .collect::<Vec<_>>();
    let selection = crate::topic_selection::select_topics(plan.thresholds, &ids, scores)
        .map_err(|e| e.to_string())?;
    Ok(ClassificationRecord {
        cache_key: plan.cache_key.clone(),
        question: plan.question.clone(),
        engine: plan.engine.clone(),
        selection,
    })
}
fn selection_summary(selection: &TopicSelection) -> Result<serde_json::Value, String> {
    serde_json::to_value((
        selection.state,
        selection
            .assessments
            .iter()
            .map(|a| (&a.candidate_id, a.band))
            .collect::<Vec<_>>(),
    ))
    .map_err(|_| INVALID.into())
}
fn validate_engine(engine: &EngineIdentity) -> Result<(), String> {
    let text_ok = |s: &str| {
        !s.is_empty() && s.len() <= 256 && s.trim() == s && !s.chars().any(char::is_control)
    };
    let hash_ok = |s: &str| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if !text_ok(&engine.model_id)
        || !text_ok(&engine.model_revision)
        || [
            &engine.weights_sha256,
            &engine.tokenizer_sha256,
            &engine.runtime_sha256,
            &engine.engine_sha256,
            &engine.options_sha256,
        ]
        .iter()
        .any(|s| !hash_ok(s))
    {
        return Err("L'identité du moteur de classement est invalide.".into());
    }
    Ok(())
}
fn digest<T: Serialize>(value: &T) -> Result<String, String> {
    Ok(crate::language::sha256_hex(&canonical_bytes(value)?))
}
fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    struct Bounded(Vec<u8>);
    impl io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > MAX_CLASSIFICATION_BYTES.saturating_sub(self.0.len()) {
                return Err(io::Error::other("Classement trop volumineux."));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded(Vec::new());
    serde_json::to_writer(
        &mut output,
        &serde_json::to_value(value).map_err(|_| INVALID)?,
    )
    .map_err(|_| INVALID)?;
    Ok(output.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_failed_publication_does_not_replace_an_older_valid_plan() {
        use crate::Segment;
        let source = vec![Segment::new(0, 500, "Le budget reste à discuter.".into())];
        let prepared =
            PreparedTopicQuestions::prepare("abababab-abab-4bab-8bab-abababababab", &source)
                .unwrap();
        let engine = EngineIdentity {
            model_id: "fixture/model".into(),
            model_revision: "fixture-v1".into(),
            weights_sha256: "a".repeat(64),
            tokenizer_sha256: "b".repeat(64),
            runtime_sha256: "c".repeat(64),
            engine_sha256: "d".repeat(64),
            options_sha256: "e".repeat(64),
        };
        let choices = [CandidateChoice::Word {
            term: "budget".into(),
        }];
        let old = ClassificationPlan::new(
            &prepared,
            0,
            &choices,
            engine.clone(),
            SelectionThresholds {
                uncertain_from: 0.5,
                proposed_from: 0.9,
            },
        )
        .unwrap();
        let new = ClassificationPlan::new(
            &prepared,
            0,
            &choices,
            engine,
            SelectionThresholds {
                uncertain_from: 0.4,
                proposed_from: 0.8,
            },
        )
        .unwrap();
        let reply = |p: &ClassificationPlan| ClassificationResponse {
            question_revision: p.question.request_revision.clone(),
            engine_revision: p.engine_revision.clone(),
            scores: vec![CandidateScore {
                candidate_id: p.question.candidates[0].candidate_id.clone(),
                score: 1.0,
            }],
        };
        let temp = tempfile::tempdir().unwrap();
        let cache = ClassificationCache::new(&temp.path().join("cache"));
        cache.save(&old, &reply(&old)).unwrap();
        let before = fs::read(cache.file(&old)).unwrap();
        let called = std::cell::Cell::new(false);
        let result = cache.save_with(&new, &reply(&new), |file, bytes| {
            called.set(true);
            assert_eq!(file, cache.file(&new));
            assert!(!bytes.is_empty());
            Err(io::Error::other(
                "Échec d'écriture injecté avant publication.",
            ))
        });
        assert!(called.get());
        assert!(result.is_err());
        assert_eq!(fs::read(cache.file(&old)).unwrap(), before);
        assert!(cache.load(&old).unwrap().is_some());
        assert!(cache.load(&new).unwrap().is_none());
        // Une reprise explicite peut ensuite publier le nouveau plan sans toucher à l'ancien.
        cache.save(&new, &reply(&new)).unwrap();
        assert!(cache.load(&new).unwrap().is_some());
        assert_eq!(fs::read(cache.file(&old)).unwrap(), before);
    }
    #[test]
    fn canonical_serialization_is_bounded_on_encoded_bytes_including_escapes() {
        assert_eq!(
            canonical_bytes(&"x".repeat(MAX_CLASSIFICATION_BYTES - 2))
                .unwrap()
                .len(),
            MAX_CLASSIFICATION_BYTES
        );
        assert!(canonical_bytes(&"x".repeat(MAX_CLASSIFICATION_BYTES - 1)).is_err());
        assert!(canonical_bytes(&"\n".repeat(MAX_CLASSIFICATION_BYTES / 2)).is_err());
    }
}
