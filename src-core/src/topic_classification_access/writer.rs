use super::{
    ClassificationLibrary, ClassificationRequest, ClassificationResponse, ClassificationSnapshot,
    PreparedClassification,
};
use std::{
    fmt,
    fs::{File, TryLockError},
    path::Path,
};
#[derive(Debug, PartialEq, Eq)]
pub enum ClassificationStartError {
    Busy,
    Unavailable(String),
}
impl fmt::Display for ClassificationStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => {
                f.write_str("Un autre classement est déjà en cours dans cette bibliothèque.")
            }
            Self::Unavailable(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for ClassificationStartError {}
pub struct ClassificationWriter {
    library: ClassificationLibrary,
}
pub enum ClassificationStart<'a> {
    Cached(Box<ClassificationSnapshot>),
    Compute(Box<ClassificationRun<'a>>),
}
pub struct ClassificationRun<'a> {
    library: &'a ClassificationLibrary,
    work: PreparedClassification,
    _lease: File,
}
impl ClassificationWriter {
    pub fn open(root: &Path) -> Result<Self, String> {
        Ok(Self {
            library: ClassificationLibrary::open(root)?,
        })
    }
    pub fn load(&self, request: &ClassificationRequest) -> Result<ClassificationSnapshot, String> {
        self.library.load(request)
    }
    pub fn try_begin(
        &self,
        request: &ClassificationRequest,
    ) -> Result<ClassificationStart<'_>, ClassificationStartError> {
        self.try_begin_after(request, || {})
    }
    fn try_begin_after(
        &self,
        request: &ClassificationRequest,
        after_miss: impl FnOnce(),
    ) -> Result<ClassificationStart<'_>, ClassificationStartError> {
        let work = self
            .library
            .prepare(request)
            .map_err(ClassificationStartError::Unavailable)?;
        let loaded = self
            .library
            .load_work_after(&work, || {})
            .map_err(ClassificationStartError::Unavailable)?;
        if loaded.record.is_some() {
            return Ok(ClassificationStart::Cached(Box::new(loaded)));
        }
        after_miss();
        let lease = self
            .library
            .root
            .classification_writer_file()
            .map_err(|_| {
                ClassificationStartError::Unavailable(
                    "Le verrou du classement est indisponible ou non vérifiable.".into(),
                )
            })?;
        match lease.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(ClassificationStartError::Busy),
            Err(TryLockError::Error(_)) => {
                return Err(ClassificationStartError::Unavailable(
                    "Le verrou du classement est indisponible ou non vérifiable.".into(),
                ))
            }
        }
        let loaded = self
            .library
            .load_work_after(&work, || {})
            .map_err(ClassificationStartError::Unavailable)?;
        if loaded.record.is_some() {
            return Ok(ClassificationStart::Cached(Box::new(loaded)));
        }
        Ok(ClassificationStart::Compute(Box::new(ClassificationRun {
            library: &self.library,
            work,
            _lease: lease,
        })))
    }
}
impl ClassificationRun<'_> {
    pub fn prepared(&self) -> &PreparedClassification {
        &self.work
    }
    pub fn save(self, response: &ClassificationResponse) -> Result<ClassificationSnapshot, String> {
        self.save_after(response, || {})
    }
    fn save_after(
        self,
        response: &ClassificationResponse,
        after: impl FnOnce(),
    ) -> Result<ClassificationSnapshot, String> {
        self.library.save_after(&self.work, response, after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topic_selection::CandidateScore;
    use std::fs;
    fn response(run: &ClassificationRun<'_>) -> ClassificationResponse {
        ClassificationResponse {
            question_revision: run.prepared().plan().question().request_revision.clone(),
            engine_revision: run.prepared().plan().engine_revision().into(),
            scores: run
                .prepared()
                .plan()
                .question()
                .candidates
                .iter()
                .map(|c| CandidateScore {
                    candidate_id: c.candidate_id.clone(),
                    score: 0.7,
                })
                .collect(),
        }
    }
    #[test]
    fn the_lock_remains_held_during_publication_and_final_source_check() {
        let root = tempfile::tempdir().unwrap();
        let (_, req) = super::super::tests::fixture(root.path());
        let writer = ClassificationWriter::open(root.path()).unwrap();
        let other = ClassificationWriter::open(root.path()).unwrap();
        let ClassificationStart::Compute(run) = writer.try_begin(&req).unwrap() else {
            panic!("calcul")
        };
        let data = response(&run);
        let mut different = ClassificationRequest {
            job_id: req.job_id.clone(),
            source_revision: req.source_revision.clone(),
            target: req.target,
            choices: req.choices.clone(),
            engine: req.engine.clone(),
            thresholds: req.thresholds,
        };
        different.engine.model_revision.push('2');
        run.save_after(&data, || {
            assert!(matches!(
                other.try_begin(&different),
                Err(ClassificationStartError::Busy)
            ));
        })
        .unwrap();
        assert!(matches!(
            other.try_begin(&different),
            Ok(ClassificationStart::Compute(_))
        ));
    }
    #[test]
    fn a_result_saved_between_the_first_read_and_the_lock_is_not_recomputed() {
        let root = tempfile::tempdir().unwrap();
        let (_, req) = super::super::tests::fixture(root.path());
        let first = ClassificationWriter::open(root.path()).unwrap();
        let other = ClassificationWriter::open(root.path()).unwrap();
        let result = first
            .try_begin_after(&req, || {
                let ClassificationStart::Compute(run) = other.try_begin(&req).unwrap() else {
                    panic!("calcul attendu")
                };
                let response = response(&run);
                run.save(&response).unwrap();
            })
            .unwrap();
        assert!(matches!(result, ClassificationStart::Cached(_)));
    }
    #[test]
    fn source_changes_during_acquisition_are_refused_and_the_lock_is_released() {
        let root = tempfile::tempdir().unwrap();
        let (mut job, mut req) = super::super::tests::fixture(root.path());
        let writer = ClassificationWriter::open(root.path()).unwrap();
        job.segments[0].text.push('!');
        let result = writer.try_begin_after(&req, || {
            fs::write(
                root.path().join(&req.job_id).join("travail.json"),
                serde_json::to_vec(&job).unwrap(),
            )
            .unwrap();
        });
        assert!(matches!(
            result,
            Err(ClassificationStartError::Unavailable(_))
        ));
        req.source_revision =
            crate::topic_cache::source_revision(&req.job_id, &job.segments).unwrap();
        assert!(matches!(
            writer.try_begin(&req),
            Ok(ClassificationStart::Compute(_))
        ));
    }
}
