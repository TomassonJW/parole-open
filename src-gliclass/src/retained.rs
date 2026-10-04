//! Calcul local et conservation sous la même réservation d’écriture.
use crate::Gliclass;
use parole_core::topic_classification_access::{
    ClassificationRequest, ClassificationSnapshot, ClassificationStart, ClassificationStartError,
    ClassificationWriter,
};
pub enum RetainedClassification {
    Reused(Box<ClassificationSnapshot>),
    Saved(Box<ClassificationSnapshot>),
}
impl RetainedClassification {
    pub fn snapshot(&self) -> &ClassificationSnapshot {
        match self {
            Self::Reused(value) | Self::Saved(value) => value,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum RetainedError {
    Start(ClassificationStartError),
    Classify(String),
    Save(String),
}
impl std::fmt::Display for RetainedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start(error) => write!(f, "{error}"),
            Self::Classify(error) => write!(f, "Classement local interrompu : {error}"),
            Self::Save(error) => write!(f, "Classement non conservé : {error}"),
        }
    }
}
impl std::error::Error for RetainedError {}

impl Gliclass {
    /// Voie synchrone interne : à exécuter hors du fil d’interface, dans le futur worker isolé.
    /// Une relecture valide ne consulte pas le moteur courant, même s’il est indisponible.
    /// Une erreur native reste un échec, pas une suggestion vide ni une confirmation.
    pub fn classify_retained(
        &mut self,
        writer: &ClassificationWriter,
        request: &ClassificationRequest,
    ) -> Result<RetainedClassification, RetainedError> {
        self.classify_retained_after(writer, request, || {})
    }
    fn classify_retained_after(
        &mut self,
        writer: &ClassificationWriter,
        request: &ClassificationRequest,
        after: impl FnOnce(),
    ) -> Result<RetainedClassification, RetainedError> {
        match writer.try_begin(request).map_err(RetainedError::Start)? {
            ClassificationStart::Cached(snapshot) => Ok(RetainedClassification::Reused(snapshot)),
            ClassificationStart::Compute(run) => {
                let response = self
                    .classify_prepared(run.prepared())
                    .map_err(RetainedError::Classify)?;
                after();
                // run conserve la réservation pendant le calcul, l’écriture et le contrôle final.
                let snapshot = run.save(&response).map_err(RetainedError::Save)?;
                Ok(RetainedClassification::Saved(Box::new(snapshot)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Artifact, ModelBundle};
    use parole_core::{
        Job, Segment, topic_questions::PreparedTopicQuestions, topic_selection::SelectionThresholds,
    };
    use std::fs;
    const ID: &str = "78787878-7878-4878-8878-787878787878";
    fn fixture() -> (
        tempfile::TempDir,
        Job,
        Gliclass,
        ClassificationRequest,
        ClassificationWriter,
    ) {
        let root = tempfile::tempdir().unwrap();
        let mut job = Job::new("fictif.wav".into(), 1000, 1000);
        job.segments = vec![Segment::new(
            0,
            1000,
            "Le budget Atlas reste ouvert.".into(),
        )];
        fs::create_dir(root.path().join(ID)).unwrap();
        fs::write(
            root.path().join(ID).join("travail.json"),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
        let artifact = |name: &str, digit: &str| Artifact {
            path: root.path().join(name),
            sha256: digit.repeat(64),
        };
        let engine = Gliclass::new(ModelBundle {
            model_id: "fixture/sans-modele".into(),
            model_revision: "fixture-v1".into(),
            weights: artifact("missing.onnx", "1"),
            tokenizer: artifact("missing.json", "2"),
            library: artifact("missing-library", "3"),
        })
        .unwrap();
        let req = ClassificationRequest {
            job_id: ID.into(),
            source_revision: PreparedTopicQuestions::prepare(ID, &job.segments)
                .unwrap()
                .question(0, &[])
                .unwrap()
                .source_revision,
            target: 0,
            choices: vec![],
            engine: engine.expected_cache_identity().unwrap(),
            thresholds: SelectionThresholds {
                uncertain_from: 0.5,
                proposed_from: 0.9,
            },
        };
        let writer = ClassificationWriter::open(root.path()).unwrap();
        (root, job, engine, req, writer)
    }
    #[test]
    fn reservation_remains_owned_after_calculation_until_the_save_finishes() {
        let (root, _, mut engine, req, writer) = fixture();
        let competitor = ClassificationWriter::open(root.path()).unwrap();
        let reached = std::cell::Cell::new(false);
        let value = engine
            .classify_retained_after(&writer, &req, || {
                reached.set(true);
                assert!(matches!(
                    competitor.try_begin(&req),
                    Err(ClassificationStartError::Busy)
                ));
                assert!(competitor.load(&req).unwrap().record.is_none());
            })
            .unwrap();
        assert!(reached.get());
        assert!(matches!(value, RetainedClassification::Saved(_)));
        assert!(matches!(
            competitor.try_begin(&req).unwrap(),
            ClassificationStart::Cached(_)
        ));
    }
    #[test]
    fn late_source_change_is_not_saved_and_does_not_leave_the_writer_held() {
        let (root, mut job, mut engine, req, writer) = fixture();
        let original = fs::read(root.path().join(ID).join("travail.json")).unwrap();
        let reached = std::cell::Cell::new(false);
        let result = engine.classify_retained_after(&writer, &req, || {
            reached.set(true);
            job.segments[0].text.push('!');
            fs::write(
                root.path().join(ID).join("travail.json"),
                serde_json::to_vec(&job).unwrap(),
            )
            .unwrap();
        });
        assert!(reached.get());
        assert!(matches!(result, Err(RetainedError::Save(_))));
        assert!(!root.path().join(ID).join("topics-classified-v1").exists());
        fs::write(root.path().join(ID).join("travail.json"), original).unwrap();
        assert!(matches!(
            writer.try_begin(&req).unwrap(),
            ClassificationStart::Compute(_)
        ));
        assert!(!engine.is_loaded());
        assert_eq!(engine.inference_count(), 0);
    }
}
