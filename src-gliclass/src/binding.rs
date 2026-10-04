//! Liaison du résultat natif à la capture réellement préparée, sans écriture ici.
use crate::{Gliclass, Outcome, RuntimeIdentity, ScoredTopics};
use parole_core::{
    topic_classification_access::PreparedClassification,
    topic_classification_cache::{ClassificationResponse, EngineIdentity},
};
const WRONG_ENGINE: &str = "Le moteur local ne correspond pas au classement préparé.";
const WRONG_QUESTION: &str = "Le résultat local ne correspond pas aux passages et sujets préparés.";
impl RuntimeIdentity {
    pub fn cache_identity(&self) -> EngineIdentity {
        EngineIdentity {
            model_id: self.model_id.clone(),
            model_revision: self.model_revision.clone(),
            weights_sha256: self.weights_sha256.clone(),
            tokenizer_sha256: self.tokenizer_sha256.clone(),
            runtime_sha256: self.runtime_sha256.clone(),
            engine_sha256: self.engine_sha256.clone(),
            options_sha256: self.options_sha256.clone(),
        }
    }
}
impl Gliclass {
    /// Identité ATTENDUE du processus courant : lit son exécutable, pas les modèles.
    /// Ne garantit ni présence ni intégrité des artefacts avant le vrai calcul.
    /// Ne remplace pas l'identité d'un autre exécutable de worker.
    pub fn expected_cache_identity(&self) -> Result<EngineIdentity, String> {
        Ok(RuntimeIdentity::expected(&self.bundle)?.cache_identity())
    }
    pub fn classify_prepared(
        &mut self,
        work: &PreparedClassification,
    ) -> Result<ClassificationResponse, String> {
        if self.expected_cache_identity()?.revision()? != work.plan().engine_revision() {
            return Err(WRONG_ENGINE.into());
        }
        let prepared = work.prepared_questions()?;
        self.classify(&prepared, work.target(), work.choices())?
            .response_for(work)
    }
}
impl ScoredTopics {
    /// Conserve les scores du résultat privé natif, sans les recalculer ni confirmer.
    pub fn response_for(
        &self,
        work: &PreparedClassification,
    ) -> Result<ClassificationResponse, String> {
        if self.question.candidates.is_empty() || &self.question != work.plan().question() {
            return Err(WRONG_QUESTION.into());
        }
        let engine_revision = self.identity.cache_identity().revision()?;
        if engine_revision != work.plan().engine_revision() {
            return Err(WRONG_ENGINE.into());
        }
        Ok(ClassificationResponse {
            question_revision: self.question.request_revision.clone(),
            engine_revision,
            scores: self.scores.clone(),
        })
    }
}
impl Outcome {
    pub fn response_for(
        &self,
        work: &PreparedClassification,
    ) -> Result<ClassificationResponse, String> {
        match self {
            Self::Scored(scored) => scored.response_for(work),
            Self::NoCandidates { question } => {
                if !work.plan().question().candidates.is_empty()
                    || question.as_ref() != work.plan().question()
                {
                    return Err(WRONG_QUESTION.into());
                }
                Ok(ClassificationResponse {
                    question_revision: question.request_revision.clone(),
                    engine_revision: work.plan().engine_revision().into(),
                    scores: Vec::new(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parole_core::{
        Job, Segment,
        topic_classification_access::{ClassificationLibrary, ClassificationRequest},
        topic_questions::{CandidateChoice, PreparedTopicQuestions},
        topic_selection::{ReviewState, SelectionThresholds},
    };
    use std::{fs, path::Path};
    const ID: &str = "ffffffff-ffff-4fff-8fff-ffffffffffff";
    fn fixture(root: &Path) -> (Job, ClassificationRequest, RuntimeIdentity) {
        let mut job = Job::new("fictif.wav".into(), 3000, 3000);
        job.segments = vec![
            Segment::new(0, 1000, "Le projet Atlas prépare le budget.".into()),
            Segment::new(
                1000,
                2000,
                "Le calendrier et le budget restent à discuter.".into(),
            ),
            Segment::new(
                2000,
                3000,
                "Le dossier Luciole attend les illustrations.".into(),
            ),
        ];
        fs::create_dir(root.join(ID)).unwrap();
        fs::write(
            root.join(ID).join("travail.json"),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
        let identity = RuntimeIdentity {
            model_id: "fixture/multilang".into(),
            model_revision: "fixture-v1".into(),
            weights_sha256: "1".repeat(64),
            tokenizer_sha256: "2".repeat(64),
            runtime_sha256: "3".repeat(64),
            engine_sha256: "4".repeat(64),
            options_sha256: "5".repeat(64),
        };
        let request = ClassificationRequest {
            job_id: ID.into(),
            source_revision: PreparedTopicQuestions::prepare(ID, &job.segments)
                .unwrap()
                .question(1, &[])
                .unwrap()
                .source_revision,
            target: 1,
            choices: vec![
                CandidateChoice::Word {
                    term: "budget".into(),
                },
                CandidateChoice::Word {
                    term: "calendrier".into(),
                },
            ],
            engine: identity.cache_identity(),
            thresholds: SelectionThresholds {
                uncertain_from: 0.5,
                proposed_from: 0.9,
            },
        };
        (job, request, identity)
    }
    // Stand-in de test uniquement : logits fictifs, pas une inférence native.
    fn scored_fixture(work: &PreparedClassification, identity: RuntimeIdentity) -> ScoredTopics {
        let question = work.plan().question().clone();
        let logits = vec![3.0, 0.3];
        let scores = crate::scores_from_logits(&question, &[1, 2], &logits).unwrap();
        ScoredTopics {
            question,
            identity,
            scores,
            logits,
            input_ids: Vec::new(),
            attention_mask: Vec::new(),
        }
    }
    #[test]
    fn all_model_and_physical_identity_fields_must_match_the_plan() {
        let root = tempfile::tempdir().unwrap();
        let (_, req, identity) = fixture(root.path());
        let library = ClassificationLibrary::open(root.path()).unwrap();
        let work = library.prepare(&req).unwrap();
        let changes: Vec<fn(&mut RuntimeIdentity)> = vec![
            |x| x.model_id.push('2'),
            |x| x.model_revision.push('2'),
            |x| x.weights_sha256 = "a".repeat(64),
            |x| x.tokenizer_sha256 = "b".repeat(64),
            |x| x.runtime_sha256 = "c".repeat(64),
            |x| x.engine_sha256 = "d".repeat(64),
            |x| x.options_sha256 = "e".repeat(64),
        ];
        for change in changes {
            let mut wrong = identity.clone();
            change(&mut wrong);
            assert_eq!(
                scored_fixture(&work, wrong)
                    .response_for(&work)
                    .err()
                    .as_deref(),
                Some(WRONG_ENGINE)
            );
        }
        assert!(!root.path().join(ID).join("topics-classified-v1").exists());
    }
    #[test]
    fn the_entire_question_is_checked_even_when_its_revision_is_left_unchanged() {
        use parole_core::topic_questions::TopicQuestion;
        let root = tempfile::tempdir().unwrap();
        let (_, req, identity) = fixture(root.path());
        let library = ClassificationLibrary::open(root.path()).unwrap();
        let work = library.prepare(&req).unwrap();
        let changes: Vec<fn(&mut TopicQuestion)> = vec![
            |q| q.schema_version += 1,
            |q| q.producer_revision += 1,
            |q| q.job_id = "00000000-0000-4000-8000-000000000000".into(),
            |q| q.source_revision = "0".repeat(64),
            |q| q.target.segment_index += 1,
            |q| q.target.start_ms += 1,
            |q| q.target.end_ms += 1,
            |q| q.target.text.push('!'),
            |q| q.target.speaker_id = Some("autre-voix".into()),
            |q| q.previous.as_mut().unwrap().text.push('!'),
            |q| q.next = None,
            |q| q.question = Some("Une autre question".into()),
            |q| {
                q.candidates[0].choice = CandidateChoice::Word {
                    term: "autre".into(),
                }
            },
            |q| q.candidates[0].label.push('!'),
            |q| q.candidates[0].candidate_id = "faux-id".into(),
            |q| q.candidates[0].lexical_link = !q.candidates[0].lexical_link,
            |q| q.candidates[0].word_evidence[0].byte_end += 1,
            |q| {
                let e = q.candidates[0].word_evidence[0].clone();
                q.candidates[0].folder_evidence.push(e);
            },
            |q| q.candidates.swap(0, 1),
            |q| q.model_input.as_mut().unwrap().text.push('!'),
            |q| q.model_input.as_mut().unwrap().labels[0].push('!'),
        ];
        for change in changes {
            let mut bad = scored_fixture(&work, identity.clone());
            let original_revision = bad.question.request_revision.clone();
            change(&mut bad.question);
            assert_eq!(bad.question.request_revision, original_revision);
            assert_eq!(
                bad.response_for(&work).err().as_deref(),
                Some(WRONG_QUESTION)
            );
        }
        assert!(!root.path().join(ID).join("topics-classified-v1").exists());
    }
    #[test]
    fn empty_outcomes_cannot_disguise_scored_or_different_questions() {
        let root = tempfile::tempdir().unwrap();
        let (_, mut req, identity) = fixture(root.path());
        let library = ClassificationLibrary::open(root.path()).unwrap();
        let nonempty = library.prepare(&req).unwrap();
        let forged = Outcome::NoCandidates {
            question: Box::new(nonempty.plan().question().clone()),
        };
        assert!(forged.response_for(&nonempty).is_err());
        req.choices.clear();
        let empty = library.prepare(&req).unwrap();
        let question = empty.plan().question().clone();
        let fake_scored = ScoredTopics {
            question: question.clone(),
            identity,
            scores: Vec::new(),
            logits: Vec::new(),
            input_ids: Vec::new(),
            attention_mask: Vec::new(),
        };
        assert_eq!(
            fake_scored.response_for(&empty).err().as_deref(),
            Some(WRONG_QUESTION)
        );
        let response = Outcome::NoCandidates {
            question: Box::new(question.clone()),
        }
        .response_for(&empty)
        .unwrap();
        assert!(response.scores.is_empty());
        assert!(library.save(&empty, &response).unwrap().record.is_some());
        let mut wrong = question;
        wrong.target.text.push('!');
        assert!(
            Outcome::NoCandidates {
                question: Box::new(wrong)
            }
            .response_for(&empty)
            .is_err()
        );
    }
    #[test]
    fn candidate_order_and_all_score_bits_survive_without_ranking_or_rounding() {
        let root = tempfile::tempdir().unwrap();
        let (_, mut req, identity) = fixture(root.path());
        req.choices.reverse();
        let library = ClassificationLibrary::open(root.path()).unwrap();
        let work = library.prepare(&req).unwrap();
        let mut scored = scored_fixture(&work, identity);
        scored.logits = vec![0.3, 3.0];
        scored.scores =
            crate::scores_from_logits(&scored.question, &[1, 2], &scored.logits).unwrap();
        assert!(scored.scores[0].score < scored.scores[1].score);
        let outcome = Outcome::Scored(Box::new(scored));
        let response = outcome.response_for(&work).unwrap();
        let Outcome::Scored(original) = &outcome else {
            panic!("fixture")
        };
        assert_eq!(response.scores, original.scores);
        let saved = library.save(&work, &response).unwrap();
        assert_eq!(
            saved
                .record
                .unwrap()
                .selection()
                .assessments
                .iter()
                .map(|x| (x.candidate_id.clone(), x.score.to_bits()))
                .collect::<Vec<_>>(),
            original
                .scores
                .iter()
                .map(|x| (x.candidate_id.clone(), x.score.to_bits()))
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn binding_does_not_hide_later_source_edits_or_rebind_an_old_result() {
        let root = tempfile::tempdir().unwrap();
        let (mut job, mut req, identity) = fixture(root.path());
        let library = ClassificationLibrary::open(root.path()).unwrap();
        let work = library.prepare(&req).unwrap();
        let scored = scored_fixture(&work, identity);
        let response = scored.response_for(&work).unwrap();
        job.segments[0].text.push('!');
        fs::write(
            root.path().join(ID).join("travail.json"),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
        assert!(library.save(&work, &response).is_err());
        req.source_revision = PreparedTopicQuestions::prepare(ID, &job.segments)
            .unwrap()
            .question(1, &[])
            .unwrap()
            .source_revision;
        let next = library.prepare(&req).unwrap();
        assert!(scored.response_for(&next).is_err());
        assert!(library.load(&req).unwrap().record.is_none());
        assert!(!root.path().join(ID).join("topics-classified-v1").exists());
    }
    #[test]
    fn the_same_raw_scores_can_be_assessed_under_explicit_different_thresholds() {
        let root = tempfile::tempdir().unwrap();
        let (_, mut req, identity) = fixture(root.path());
        let library = ClassificationLibrary::open(root.path()).unwrap();
        let first = library.prepare(&req).unwrap();
        let scored = scored_fixture(&first, identity);
        let first_response = scored.response_for(&first).unwrap();
        library.save(&first, &first_response).unwrap();
        req.thresholds = SelectionThresholds {
            uncertain_from: 0.1,
            proposed_from: 0.2,
        };
        let second = library.prepare(&req).unwrap();
        assert_ne!(first.plan().cache_key(), second.plan().cache_key());
        let second_response = scored.response_for(&second).unwrap();
        assert_eq!(first_response.scores, second_response.scores);
        let second_record = library
            .save(&second, &second_response)
            .unwrap()
            .record
            .unwrap();
        assert_eq!(second_record.selection().state, ReviewState::Proposed);
        assert_eq!(second_record.selection().assessments.len(), 2);
        assert_eq!(
            fs::read_dir(root.path().join(ID).join("topics-classified-v1"))
                .unwrap()
                .count(),
            2
        );
    }
    #[test]
    fn genuine_question_and_scores_are_preserved_through_binding_save_and_reload() {
        let root = tempfile::tempdir().unwrap();
        let (job, req, identity) = fixture(root.path());
        let library = ClassificationLibrary::open(root.path()).unwrap();
        let work = library.prepare(&req).unwrap();
        let scored = scored_fixture(&work, identity.clone());
        let original_bits: Vec<_> = scored.scores().iter().map(|x| x.score.to_bits()).collect();
        let response = scored.response_for(&work).unwrap();
        assert_eq!(
            response.question_revision,
            scored.question().request_revision
        );
        assert_eq!(
            response.engine_revision,
            identity.cache_identity().revision().unwrap()
        );
        assert_eq!(
            response
                .scores
                .iter()
                .map(|x| x.score.to_bits())
                .collect::<Vec<_>>(),
            original_bits
        );
        assert_eq!(
            response
                .scores
                .iter()
                .map(|x| &x.candidate_id)
                .collect::<Vec<_>>(),
            scored
                .scores()
                .iter()
                .map(|x| &x.candidate_id)
                .collect::<Vec<_>>()
        );
        let saved = library.save(&work, &response).unwrap();
        let record = saved.record.as_ref().unwrap();
        assert_eq!(record.selection().state, ReviewState::Ambiguous);
        assert_eq!(record.question().target.text, job.segments[1].text);
        assert_eq!(
            record
                .selection()
                .assessments
                .iter()
                .map(|x| x.score.to_bits())
                .collect::<Vec<_>>(),
            original_bits
        );
        let reopened = ClassificationLibrary::open(root.path())
            .unwrap()
            .load(&req)
            .unwrap();
        assert_eq!(
            serde_json::to_value(saved).unwrap(),
            serde_json::to_value(reopened).unwrap()
        );
    }
}
