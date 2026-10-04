//! Questions explicites, préparées localement sans inférence ni confirmation.
use crate::{
    topic_candidates::{prepare_topic_candidates, Evidence, TopicCandidates},
    Segment,
};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CandidateChoice {
    Word { term: String },
    PossibleFolder { name: String },
    WordInPossibleFolder { term: String, folder: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Passage {
    pub segment_index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub speaker_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EvidenceRef {
    pub segment_index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub byte_start: usize,
    pub byte_end: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct QuestionCandidate {
    pub candidate_id: String,
    pub choice: CandidateChoice,
    pub label: String,
    pub lexical_link: bool,
    pub word_evidence: Vec<EvidenceRef>,
    pub folder_evidence: Vec<EvidenceRef>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelInput {
    pub text: String,
    pub labels: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TopicQuestion {
    pub schema_version: u32,
    pub producer_revision: u32,
    pub job_id: String,
    pub source_revision: String,
    pub request_revision: String,
    pub target: Passage,
    pub previous: Option<Passage>,
    pub next: Option<Passage>,
    pub question: Option<String>,
    pub candidates: Vec<QuestionCandidate>,
    pub model_input: Option<ModelInput>,
}
pub struct PreparedTopicQuestions<'a> {
    segments: &'a [Segment],
    candidates: TopicCandidates,
    job_id: String,
    source_revision: String,
}
impl<'a> PreparedTopicQuestions<'a> {
    pub fn prepare(job_id: &str, segments: &'a [Segment]) -> Result<Self, String> {
        let source_revision = crate::topic_cache::source_revision(job_id, segments)?;
        if segments
            .iter()
            .any(|s| s.start_ms > s.end_ms || s.end_ms > 9_007_199_254_740_991)
        {
            return Err("Les heures de la transcription sont invalides.".into());
        }
        Ok(Self {
            segments,
            candidates: prepare_topic_candidates(segments)?,
            job_id: job_id.into(),
            source_revision,
        })
    }
    fn bind(&self, mut question: TopicQuestion) -> Result<TopicQuestion, String> {
        question.request_revision = digest(&("parole-topic-question-v1", &question))?;
        Ok(question)
    }
    fn resolve(&self, choice: &CandidateChoice) -> Result<QuestionCandidate, String> {
        let (term, folder) = match choice {
            CandidateChoice::Word { term } => (Some(term), None),
            CandidateChoice::PossibleFolder { name } => (None, Some(name)),
            CandidateChoice::WordInPossibleFolder { term, folder } => (Some(term), Some(folder)),
        };
        let mut words: Vec<&Evidence> = match term {
            Some(term) => self
                .candidates
                .words
                .iter()
                .find(|w| &w.term == term)
                .ok_or("Le mot demandé n'est pas une piste lexicale de cette source.")?
                .evidence
                .iter()
                .collect(),
            None => Vec::new(),
        };
        let mut folders: Vec<&Evidence> = match folder {
            Some(name) => self
                .candidates
                .possible_folders
                .iter()
                .find(|f| &f.name == name)
                .ok_or("Le dossier demandé n'est pas une piste possible de cette source.")?
                .evidence
                .iter()
                .collect(),
            None => Vec::new(),
        };
        let label = match (term, folder) {
            (Some(term), Some(name)) => {
                let word_indices = words
                    .iter()
                    .map(|e| e.segment_index)
                    .collect::<std::collections::BTreeSet<_>>();
                let folder_indices = folders
                    .iter()
                    .map(|e| e.segment_index)
                    .collect::<std::collections::BTreeSet<_>>();
                words.retain(|e| folder_indices.contains(&e.segment_index));
                folders.retain(|e| word_indices.contains(&e.segment_index));
                if words.is_empty() || folders.is_empty() {
                    return Err(
                        "Le mot et le dossier possible ne figurent dans aucun même passage.".into(),
                    );
                }
                format!("{term} (dossier possible : {name})")
            }
            (Some(term), None) => term.clone(),
            (None, Some(name)) => format!("Dossier possible : {name}"),
            (None, None) => unreachable!(),
        };
        let lexical_link = words
            .iter()
            .map(|e| e.segment_index)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 1;
        Ok(QuestionCandidate {
            candidate_id: digest(&("parole-topic-choice-v1", &self.source_revision, choice))?,
            choice: choice.clone(),
            label,
            lexical_link,
            word_evidence: words.into_iter().map(evidence_ref).collect(),
            folder_evidence: folders.into_iter().map(evidence_ref).collect(),
        })
    }
    pub fn question(
        &self,
        target: usize,
        choices: &[CandidateChoice],
    ) -> Result<TopicQuestion, String> {
        let mut unique = std::collections::HashSet::new();
        if choices.len() > 25 || choices.iter().any(|choice| !unique.insert(choice)) {
            return Err("La liste des sujets doit contenir au plus 25 choix distincts.".into());
        }
        let passage = |index| {
            self.segments.get(index).map(|s: &Segment| Passage {
                segment_index: index,
                start_ms: s.start_ms,
                end_ms: s.end_ms,
                text: s.text.clone(),
                speaker_id: s.speaker_id.clone(),
            })
        };
        let target_passage = passage(target).ok_or("Le passage demandé est introuvable.")?;
        let previous = target.checked_sub(1).and_then(passage);
        let next = target.checked_add(1).and_then(passage);
        let mut candidates = Vec::new();
        let mut evidence_count = 0;
        for choice in choices {
            let candidate = self.resolve(choice)?;
            evidence_count += candidate.word_evidence.len() + candidate.folder_evidence.len();
            if evidence_count > 20_000 || candidate.label.len() > 4_096 {
                return Err(
                    "Les sujets et leurs preuves dépassent les limites ; aucune fin tronquée."
                        .into(),
                );
            }
            candidates.push(candidate);
        }
        if candidates.is_empty() {
            return self.bind(TopicQuestion {
                schema_version: 1,
                producer_revision: 1,
                job_id: self.job_id.clone(),
                source_revision: self.source_revision.clone(),
                request_revision: String::new(),
                target: target_passage,
                previous,
                next,
                question: None,
                candidates,
                model_input: None,
            });
        }
        const RESERVED: [&str; 7] = [
            "<<LABEL>>",
            "<<SEP>>",
            "<s>",
            "</s>",
            "<pad>",
            "<unk>",
            "<mask>",
        ];
        if target_passage.text.trim().is_empty()
            || previous
                .iter()
                .chain(std::iter::once(&target_passage))
                .chain(next.iter())
                .any(|p| RESERVED.iter().any(|marker| p.text.contains(marker)))
        {
            return Err(
                "Le passage ou son voisinage ne peut pas être transmis tel quel au classifieur."
                    .into(),
            );
        }
        let labels = candidates
            .iter()
            .map(|c| c.label.clone())
            .collect::<Vec<_>>();
        let question = Some(format!(
            "Ce passage concerne-t-il « {} » ?",
            labels.join(" » ou « ")
        ));
        let text = format!(
            "Contexte précédent : {}\nPassage à classer : {}\nContexte suivant : {}",
            previous.as_ref().map_or("", |p| &p.text),
            target_passage.text,
            next.as_ref().map_or("", |p| &p.text)
        );
        self.bind(TopicQuestion {
            schema_version: 1,
            producer_revision: 1,
            job_id: self.job_id.clone(),
            source_revision: self.source_revision.clone(),
            request_revision: String::new(),
            target: target_passage,
            previous,
            next,
            question,
            candidates,
            model_input: Some(ModelInput { text, labels }),
        })
    }
}
fn digest<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "La question ne peut pas être conservée.")?;
    Ok(crate::language::sha256_hex(&bytes))
}
fn evidence_ref(e: &Evidence) -> EvidenceRef {
    EvidenceRef {
        segment_index: e.segment_index,
        start_ms: e.start_ms,
        end_ms: e.end_ms,
        byte_start: e.byte_start,
        byte_end: e.byte_end,
    }
}
