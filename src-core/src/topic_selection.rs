//! Propositions techniques uniquement : aucun score ne confirme un rattachement.

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct SelectionThresholds {
    pub uncertain_from: f64,
    pub proposed_from: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidateScore {
    pub candidate_id: String,
    pub score: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreBand {
    BelowThreshold,
    Uncertain,
    Proposed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    NoCandidates,
    NoSuggestion,
    Ambiguous,
    Proposed,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct CandidateAssessment {
    pub candidate_id: String,
    pub score: f64,
    pub band: ScoreBand,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct TopicSelection {
    pub thresholds: SelectionThresholds,
    #[serde(rename = "review_state")]
    pub state: ReviewState,
    pub assessments: Vec<CandidateAssessment>,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum SelectionError {
    #[error("Les seuils du classement sont invalides.")]
    InvalidThresholds,
    #[error("La liste des sujets proposés est invalide.")]
    InvalidCandidates,
    #[error("Les scores ne correspondent pas aux sujets proposés.")]
    CandidateMismatch,
    #[error("Un score de classement est invalide.")]
    InvalidScore,
}

pub fn select_topics(
    thresholds: SelectionThresholds,
    candidate_ids: &[String],
    scores: &[CandidateScore],
) -> Result<TopicSelection, SelectionError> {
    if !thresholds.uncertain_from.is_finite()
        || !thresholds.proposed_from.is_finite()
        || !(0.0..=1.0).contains(&thresholds.uncertain_from)
        || !(0.0..=1.0).contains(&thresholds.proposed_from)
        || thresholds.uncertain_from >= thresholds.proposed_from
    {
        return Err(SelectionError::InvalidThresholds);
    }
    let mut unique = std::collections::HashSet::new();
    for id in candidate_ids {
        if id.is_empty()
            || id.len() > 256
            || id.trim() != id
            || id.chars().any(char::is_control)
            || !unique.insert(id.as_str())
        {
            return Err(SelectionError::InvalidCandidates);
        }
    }
    if candidate_ids.len() != scores.len()
        || candidate_ids
            .iter()
            .zip(scores)
            .any(|(id, entry)| id != &entry.candidate_id)
    {
        return Err(SelectionError::CandidateMismatch);
    }
    if scores
        .iter()
        .any(|entry| !entry.score.is_finite() || !(0.0..=1.0).contains(&entry.score))
    {
        return Err(SelectionError::InvalidScore);
    }
    let assessments: Vec<_> = scores
        .iter()
        .map(|entry| CandidateAssessment {
            candidate_id: entry.candidate_id.clone(),
            score: entry.score,
            band: if entry.score >= thresholds.proposed_from {
                ScoreBand::Proposed
            } else if entry.score >= thresholds.uncertain_from {
                ScoreBand::Uncertain
            } else {
                ScoreBand::BelowThreshold
            },
        })
        .collect();
    let state = if assessments.is_empty() {
        ReviewState::NoCandidates
    } else if assessments
        .iter()
        .any(|entry| entry.band == ScoreBand::Uncertain)
    {
        ReviewState::Ambiguous
    } else if assessments
        .iter()
        .any(|entry| entry.band == ScoreBand::Proposed)
    {
        ReviewState::Proposed
    } else {
        ReviewState::NoSuggestion
    };
    Ok(TopicSelection {
        thresholds,
        state,
        assessments,
    })
}
