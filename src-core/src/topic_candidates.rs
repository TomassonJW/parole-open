//! Pistes lexicales locales, jamais des sujets confirmés.
use crate::Segment;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub segment_index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub citation: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordCandidate {
    pub term: String,
    /// Indice lexical entier, pas une probabilité.
    pub lexical_weight: u32,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LexicalLink {
    pub shared_term: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PossibleFolder {
    pub name: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicCandidates {
    pub schema_version: u32,
    pub words: Vec<WordCandidate>,
    pub links: Vec<LexicalLink>,
    pub possible_folders: Vec<PossibleFolder>,
    pub without_suggestion: Vec<usize>,
}

/// Chaque entrée est une piste attestée et non une attribution de thème.
/// Refuse les entrées hors bornes plutôt que de tronquer une fin silencieusement.
pub fn prepare_topic_candidates(segments: &[Segment]) -> Result<TopicCandidates, String> {
    use std::collections::{BTreeMap, BTreeSet};
    if segments.len() > 2_000 || segments.iter().any(|s| s.text.len() > 4_096) {
        return Err("Parole trop longue pour la préparation locale ; aucune fin tronquée.".into());
    }
    let mut terms: BTreeMap<String, Vec<Evidence>> = BTreeMap::new();
    let mut folders: BTreeMap<String, Vec<Evidence>> = BTreeMap::new();
    let mut without_suggestion = Vec::new();
    let mut total_tokens = 0usize;
    for (index, segment) in segments.iter().enumerate() {
        let mut found = false;
        let spans = tokens(&segment.text);
        total_tokens += spans.len();
        if spans.len() > 256 || total_tokens > 20_000 {
            return Err(
                "Trop de mots pour la préparation locale ; aucune proposition partielle.".into(),
            );
        }
        for (position, &(start, end)) in spans.iter().enumerate() {
            let original = &segment.text[start..end];
            let key = if original.chars().any(|c| c.is_ascii_digit()) {
                original.to_owned() // Codes : identité exacte, même la casse.
            } else {
                original.to_lowercase()
            };
            if position > 0 {
                let cue = &segment.text[spans[position - 1].0..spans[position - 1].1];
                let negated =
                    spans[position.saturating_sub(4)..position - 1]
                        .iter()
                        .any(|&(a, b)| {
                            matches!(
                                segment.text[a..b].to_lowercase().as_str(),
                                "pas" | "non" | "jamais" | "ni"
                            )
                        });
                let looks_like_name = original.chars().next().is_some_and(char::is_uppercase)
                    || original.chars().any(|c| c.is_ascii_digit());
                if matches!(cue.to_lowercase().as_str(), "projet" | "dossier")
                    && looks_like_name
                    && !negated
                {
                    folders
                        .entry(original.to_owned())
                        .or_default()
                        .push(evidence(segment, index, start, end));
                    found = true;
                }
            }
            if key.chars().count() < 4
                || is_banal(&key)
                || matches!(key.as_str(), "projet" | "dossier")
            {
                continue;
            }
            found = true;
            terms
                .entry(key)
                .or_default()
                .push(evidence(segment, index, start, end));
        }
        if !found {
            without_suggestion.push(index);
        }
    }
    let mut words = Vec::new();
    let mut links = Vec::new();
    for (term, evidence) in terms {
        let document_frequency = evidence
            .iter()
            .map(|e| e.segment_index)
            .collect::<BTreeSet<_>>()
            .len();
        let lexical_weight = 10_000 / u32::try_from(document_frequency).unwrap_or(u32::MAX);
        if document_frequency > 1 {
            links.push(LexicalLink {
                shared_term: term.clone(),
                evidence: evidence.clone(),
            });
        }
        words.push(WordCandidate {
            term,
            lexical_weight,
            evidence,
        });
    }
    Ok(TopicCandidates {
        schema_version: 1,
        words,
        links,
        possible_folders: folders
            .into_iter()
            .map(|(name, evidence)| PossibleFolder { name, evidence })
            .collect(),
        without_suggestion,
    })
}

fn evidence(segment: &Segment, index: usize, byte_start: usize, byte_end: usize) -> Evidence {
    Evidence {
        segment_index: index,
        start_ms: segment.start_ms,
        end_ms: segment.end_ms,
        citation: segment.text.clone(),
        byte_start,
        byte_end,
    }
}

fn is_banal(word: &str) -> bool {
    matches!(
        word,
        "bonjour"
            | "merci"
            | "avec"
            | "pour"
            | "dans"
            | "vous"
            | "nous"
            | "elles"
            | "ils"
            | "elle"
            | "leur"
            | "leurs"
            | "cela"
            | "cette"
            | "c'est"
            | "donc"
            | "mais"
            | "alors"
            | "voilà"
            | "parce"
            | "comme"
            | "être"
            | "avoir"
            | "sont"
            | "tout"
            | "tous"
            | "bien"
            | "déjà"
            | "encore"
            | "aussi"
            | "sans"
            | "plus"
            | "moins"
            | "quel"
            | "quelle"
    )
}

fn tokens(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<_> = text.char_indices().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].1.is_alphanumeric() {
            i += 1;
            continue;
        }
        let start = chars[i].0;
        i += 1;
        while i < chars.len() {
            let ch = chars[i].1;
            let mark = matches!(ch, '\u{0300}'..='\u{036f}' | '\u{1ab0}'..='\u{1aff}' | '\u{1dc0}'..='\u{1dff}' | '\u{20d0}'..='\u{20ff}' | '\u{fe20}'..='\u{fe2f}');
            let prefix = &text[start..chars[i].0];
            let elision = matches!(ch, '\'' | '’')
                && matches!(
                    prefix.to_lowercase().as_str(),
                    "l" | "d" | "j" | "c" | "s" | "n" | "m" | "t" | "qu"
                );
            let joined = matches!(ch, '-' | '\'' | '’')
                && !elision
                && chars
                    .get(i + 1)
                    .is_some_and(|(_, next)| next.is_alphanumeric());
            if ch.is_alphanumeric() || mark || joined {
                i += 1;
            } else {
                break;
            }
        }
        let end = chars.get(i).map_or(text.len(), |(offset, _)| *offset);
        result.push((start, end));
    }
    result
}
