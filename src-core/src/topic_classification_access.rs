//! Classements rattachés à la source réellement enregistrée, sans capacité d’inférence.
use crate::{
    audio_access::MAX_STATE_BYTES, audio_access_fs::Directory,
    topic_classification_cache::ClassificationCache, Job,
};
use crate::{
    topic_classification_cache::{
        ClassificationPlan, ClassificationRecord, ClassificationResponse, EngineIdentity,
    },
    topic_questions::{CandidateChoice, PreparedTopicQuestions},
    topic_selection::SelectionThresholds,
    Segment,
};
use serde::Serialize;
use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};
/// Requête construite par le backend ; moteur et seuils ne proviennent pas de la fenêtre.
pub struct ClassificationRequest {
    pub job_id: String,
    pub source_revision: String,
    pub target: usize,
    pub choices: Vec<CandidateChoice>,
    pub engine: EngineIdentity,
    pub thresholds: SelectionThresholds,
}
#[derive(Serialize)]
pub struct ClassificationSnapshot {
    pub schema_version: u32,
    pub job_id: String,
    pub source_revision: String,
    pub target: usize,
    pub record: Option<ClassificationRecord>,
}
/// Capture scellée en mémoire, liée à l’instance qui l’a préparée ; jamais désérialisée.
pub struct PreparedClassification {
    plan: ClassificationPlan,
    segments: Vec<Segment>,
    choices: Vec<CandidateChoice>,
    scope: Arc<()>,
}
impl PreparedClassification {
    pub fn plan(&self) -> &ClassificationPlan {
        &self.plan
    }
    pub fn target(&self) -> usize {
        self.plan.question().target.segment_index
    }
    pub fn choices(&self) -> &[CandidateChoice] {
        &self.choices
    }
    pub fn prepared_questions(&self) -> Result<PreparedTopicQuestions<'_>, String> {
        PreparedTopicQuestions::prepare(&self.plan.question().job_id, &self.segments)
    }
}
mod writer;
pub use writer::{
    ClassificationRun, ClassificationStart, ClassificationStartError, ClassificationWriter,
};

const INVALID: &str = "Travail ou classement indisponible ou non vérifiable.";
const STALE: &str = "La transcription a changé ; prépare à nouveau le classement.";
const CACHE_FOLDER: &str = "topics-classified-v1";
/// Racine et parents de confiance, namespace privé/stable, un seul écrivain par cache.
/// Aucun verrou interprocessus, moteur ou ordonnanceur n’est fourni par cette couche.
pub struct ClassificationLibrary {
    root: Directory,
    path: PathBuf,
    scope: Arc<()>,
}
impl ClassificationLibrary {
    pub fn open(root: &Path) -> Result<Self, String> {
        if !root.is_absolute() || root.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(INVALID.into());
        }
        Ok(Self {
            root: Directory::open(root).map_err(|_| INVALID)?,
            path: root.to_path_buf(),
            scope: Arc::new(()),
        })
    }
    fn job(&self, id: &str) -> Result<Job, String> {
        crate::topic_cache::source_revision(id, &[])?;
        let folder = self.root.child(id).map_err(|_| INVALID)?;
        let bytes = folder
            .read("travail.json", MAX_STATE_BYTES)
            .map_err(|_| INVALID)?;
        serde_json::from_slice(&bytes).map_err(|_| INVALID.into())
    }
    pub fn prepare(
        &self,
        request: &ClassificationRequest,
    ) -> Result<PreparedClassification, String> {
        self.prepare_after(request, || {})
    }
    // Les trois fonctions *_after sont les chemins employés par l'API publique.
    // Le callback privé est vide en production et permet une mutation déterministe
    // du fichier source aux coutures réelles dans les tests, sans nouveau validateur.
    fn prepare_after(
        &self,
        request: &ClassificationRequest,
        after: impl FnOnce(),
    ) -> Result<PreparedClassification, String> {
        let job = self.job(&request.job_id)?;
        if crate::topic_cache::source_revision(&request.job_id, &job.segments)?
            != request.source_revision
        {
            return Err(STALE.into());
        }
        let prepared = PreparedTopicQuestions::prepare(&request.job_id, &job.segments)?;
        let plan = ClassificationPlan::new(
            &prepared,
            request.target,
            &request.choices,
            request.engine.clone(),
            request.thresholds,
        )?;
        let work = PreparedClassification {
            plan,
            segments: job.segments,
            choices: request.choices.clone(),
            scope: self.scope.clone(),
        };
        after();
        self.current(&work)?;
        Ok(work)
    }
    fn cache(&self, work: &PreparedClassification) -> ClassificationCache {
        ClassificationCache::new(
            &self
                .path
                .join(&work.plan.question().job_id)
                .join(CACHE_FOLDER),
        )
    }
    fn current(&self, work: &PreparedClassification) -> Result<(), String> {
        if !Arc::ptr_eq(&self.scope, &work.scope) {
            return Err("Cette préparation appartient à une autre bibliothèque de travaux.".into());
        }
        let current = self.job(&work.plan.question().job_id)?;
        if crate::topic_cache::source_revision(&work.plan.question().job_id, &current.segments)?
            != work.plan.question().source_revision
        {
            return Err(STALE.into());
        }
        Ok(())
    }
    fn checked<T>(
        &self,
        work: &PreparedClassification,
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        self.current(work)?;
        let result = operation()?;
        self.current(work)?;
        Ok(result)
    }
    pub fn load(&self, request: &ClassificationRequest) -> Result<ClassificationSnapshot, String> {
        self.load_after(request, || {})
    }
    fn load_after(
        &self,
        request: &ClassificationRequest,
        after: impl FnOnce(),
    ) -> Result<ClassificationSnapshot, String> {
        let work = self.prepare(request)?;
        self.load_work_after(&work, after)
    }
    fn load_work_after(
        &self,
        work: &PreparedClassification,
        after: impl FnOnce(),
    ) -> Result<ClassificationSnapshot, String> {
        let record = self.checked(work, || {
            let record = self.cache(work).load(work.plan())?;
            after();
            Ok(record)
        })?;
        Ok(snapshot(work, record))
    }
    pub fn save(
        &self,
        work: &PreparedClassification,
        response: &ClassificationResponse,
    ) -> Result<ClassificationSnapshot, String> {
        self.save_after(work, response, || {})
    }
    fn save_after(
        &self,
        work: &PreparedClassification,
        response: &ClassificationResponse,
        after: impl FnOnce(),
    ) -> Result<ClassificationSnapshot, String> {
        let record = self.checked(work, || {
            let record = self.cache(work).save(work.plan(), response)?;
            after();
            Ok(record)
        })?;
        Ok(snapshot(work, Some(record)))
    }
}
fn snapshot(
    work: &PreparedClassification,
    record: Option<ClassificationRecord>,
) -> ClassificationSnapshot {
    ClassificationSnapshot {
        schema_version: 1,
        job_id: work.plan.question().job_id.clone(),
        source_revision: work.plan.question().source_revision.clone(),
        target: work.target(),
        record,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topic_selection::CandidateScore;
    use std::fs;
    pub(super) fn fixture(root: &Path) -> (Job, ClassificationRequest) {
        let id = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
        let mut job = Job::new("fictif.wav".into(), 1_000, 1_000);
        job.segments = vec![Segment::new(
            0,
            1_000,
            "Le budget du projet Atlas reste à discuter.".into(),
        )];
        fs::create_dir(root.join(id)).unwrap();
        fs::write(
            root.join(id).join("travail.json"),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
        let request = ClassificationRequest {
            job_id: id.into(),
            source_revision: crate::topic_cache::source_revision(id, &job.segments).unwrap(),
            target: 0,
            choices: vec![CandidateChoice::Word {
                term: "budget".into(),
            }],
            engine: EngineIdentity {
                model_id: "fixture/local".into(),
                model_revision: "v1".into(),
                weights_sha256: "1".repeat(64),
                tokenizer_sha256: "2".repeat(64),
                runtime_sha256: "3".repeat(64),
                engine_sha256: "4".repeat(64),
                options_sha256: "5".repeat(64),
            },
            thresholds: SelectionThresholds {
                uncertain_from: 0.5,
                proposed_from: 0.9,
            },
        };
        (job, request)
    }
    fn changed(root: &Path, request: &ClassificationRequest, job: &Job) {
        let mut next = job.clone();
        next.segments[0].text.push('!');
        fs::write(
            root.join(&request.job_id).join("travail.json"),
            serde_json::to_vec(&next).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn changed_during_preparation_never_returns_a_current_work() {
        let t = tempfile::tempdir().unwrap();
        let (job, req) = fixture(t.path());
        let library = ClassificationLibrary::open(t.path()).unwrap();
        assert!(library
            .prepare_after(&req, || changed(t.path(), &req, &job))
            .is_err());
        assert!(!t.path().join(&req.job_id).join(CACHE_FOLDER).exists());
    }
    #[test]
    fn changed_during_missing_cache_read_never_returns_a_current_snapshot() {
        let t = tempfile::tempdir().unwrap();
        let (job, req) = fixture(t.path());
        let library = ClassificationLibrary::open(t.path()).unwrap();
        assert!(library
            .load_after(&req, || changed(t.path(), &req, &job))
            .is_err());
        assert!(!t.path().join(&req.job_id).join(CACHE_FOLDER).exists());
    }
    #[test]
    fn changed_during_existing_cache_read_never_returns_old_record() {
        let t = tempfile::tempdir().unwrap();
        let (job, req) = fixture(t.path());
        let library = ClassificationLibrary::open(t.path()).unwrap();
        let work = library.prepare(&req).unwrap();
        let response = ClassificationResponse {
            question_revision: work.plan.question().request_revision.clone(),
            engine_revision: work.plan.engine_revision().into(),
            scores: work
                .plan
                .question()
                .candidates
                .iter()
                .map(|c| CandidateScore {
                    candidate_id: c.candidate_id.clone(),
                    score: 0.95,
                })
                .collect(),
        };
        library.save(&work, &response).unwrap();
        assert!(library
            .load_after(&req, || changed(t.path(), &req, &job))
            .is_err());
        assert!(library.cache(&work).load(work.plan()).unwrap().is_some());
    }
    #[test]
    fn changed_after_publication_keeps_old_record_but_never_returns_it_as_current() {
        let t = tempfile::tempdir().unwrap();
        let (job, req) = fixture(t.path());
        let library = ClassificationLibrary::open(t.path()).unwrap();
        let work = library.prepare(&req).unwrap();
        let response = ClassificationResponse {
            question_revision: work.plan.question().request_revision.clone(),
            engine_revision: work.plan.engine_revision().into(),
            scores: work
                .plan
                .question()
                .candidates
                .iter()
                .map(|c| CandidateScore {
                    candidate_id: c.candidate_id.clone(),
                    score: 0.95,
                })
                .collect(),
        };
        assert!(library
            .save_after(&work, &response, || changed(t.path(), &req, &job))
            .is_err());
        assert!(library.cache(&work).load(work.plan()).unwrap().is_some());
        let mut new_req = req;
        let mut next = job.clone();
        next.segments[0].text.push('!');
        new_req.source_revision =
            crate::topic_cache::source_revision(&new_req.job_id, &next.segments).unwrap();
        assert!(library.load(&new_req).unwrap().record.is_none());
    }
}
