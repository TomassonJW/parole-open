//! Cache dérivé local ; ne contient aucun choix humain ni décision confirmée.
use crate::{audio_access_fs as access_fs, audio_durability as durability};
use crate::{
    language::sha256_hex,
    topic_candidates::{prepare_topic_candidates, TopicCandidates},
    Segment,
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
const INVALID: &str = "Propositions enregistrées indisponibles ou non vérifiables";
const REVISION: u32 = 1;
const MAX_BYTES: usize = 32 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    schema_version: u32,
    producer_revision: u32,
    source_revision: String,
    candidates_sha256: String,
    candidates: TopicCandidates,
}
/// Le dossier et ses parents sont choisis par le backend et restent de confiance
/// pendant l'écriture. Aucun chemin ni état source fourni par la fenêtre.
pub struct TopicCache {
    directory: PathBuf,
}
impl TopicCache {
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    fn open_directory(&self) -> io::Result<access_fs::Directory> {
        if !self.directory.is_absolute() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let parent = self.directory.parent().ok_or(io::ErrorKind::InvalidInput)?;
        let name = self
            .directory
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or(io::ErrorKind::InvalidInput)?;
        access_fs::Directory::open(parent)?.child(name)
    }
    /// Lecture seule, sans préparation ni modèle. Absence distincte d'une corruption.
    pub fn load(
        &self,
        identity: &str,
        segments: &[Segment],
    ) -> Result<Option<TopicCandidates>, String> {
        let revision = source_revision(identity, segments)?;
        let folder = match self.open_directory() {
            Ok(folder) => folder,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(INVALID.into()),
        };
        let bytes = match folder.read(&file_name(&revision), MAX_BYTES) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(INVALID.into()),
        };
        let saved: Saved = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
        if saved.schema_version != 1
            || saved.producer_revision != REVISION
            || saved.source_revision != revision
            || saved.candidates_sha256 != sha256_hex(&bounded_json(&saved.candidates, MAX_BYTES)?)
        {
            return Err(INVALID.into());
        }
        validate_candidates(&saved.candidates, segments)?;
        Ok(Some(saved.candidates))
    }
    /// Action explicite, réutilisant d'abord le résultat valide. Ne touche ni la
    /// transcription, ni les fichiers annotations-v1, ni les anciennes révisions.
    pub fn prepare(&self, identity: &str, segments: &[Segment]) -> Result<TopicCandidates, String> {
        if let Some(candidates) = self.load(identity, segments)? {
            return Ok(candidates);
        }
        let revision = source_revision(identity, segments)?;
        let candidates = prepare_topic_candidates(segments)?;
        let saved = Saved {
            schema_version: 1,
            producer_revision: REVISION,
            source_revision: revision.clone(),
            candidates_sha256: sha256_hex(&bounded_json(&candidates, MAX_BYTES)?),
            candidates,
        };
        let bytes = bounded_json(&saved, MAX_BYTES)?;
        if bytes.len() > MAX_BYTES {
            return Err("Propositions trop volumineuses pour être enregistrées".into());
        }
        match fs::create_dir(&self.directory) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(_) => return Err(INVALID.into()),
        }
        let _folder = self.open_directory().map_err(|_| INVALID)?;
        durability::atomic_write(&self.directory.join(file_name(&revision)), &bytes)
            .map_err(|_| INVALID)?;
        Ok(saved.candidates)
    }
}
fn validate_candidates(candidates: &TopicCandidates, segments: &[Segment]) -> Result<(), String> {
    if candidates.schema_version != 1
        || !candidates.words.windows(2).all(|p| p[0].term < p[1].term)
        || !candidates
            .possible_folders
            .windows(2)
            .all(|p| p[0].name < p[1].name)
    {
        return Err(INVALID.into());
    }
    let mut covered = std::collections::BTreeSet::new();
    for proof in candidates
        .words
        .iter()
        .flat_map(|w| &w.evidence)
        .chain(candidates.possible_folders.iter().flat_map(|f| &f.evidence))
    {
        covered.insert(proof.segment_index);
    }
    let without: Vec<_> = (0..segments.len())
        .filter(|i| !covered.contains(i))
        .collect();
    if candidates.without_suggestion != without {
        return Err(INVALID.into());
    }
    for proof in candidates
        .words
        .iter()
        .flat_map(|w| &w.evidence)
        .chain(candidates.links.iter().flat_map(|l| &l.evidence))
        .chain(candidates.possible_folders.iter().flat_map(|f| &f.evidence))
    {
        let source = segments.get(proof.segment_index).ok_or(INVALID)?;
        if proof.citation != source.text
            || proof.start_ms != source.start_ms
            || proof.end_ms != source.end_ms
            || proof.byte_start >= proof.byte_end
            || source.text.get(proof.byte_start..proof.byte_end).is_none()
        {
            return Err(INVALID.into());
        }
    }
    let mut linked = Vec::new();
    for word in &candidates.words {
        let distinct = word
            .evidence
            .iter()
            .map(|e| e.segment_index)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        if distinct == 0
            || word.lexical_weight != 10_000 / u32::try_from(distinct).map_err(|_| INVALID)?
        {
            return Err(INVALID.into());
        }
        for proof in &word.evidence {
            let text = &proof.citation[proof.byte_start..proof.byte_end];
            let term = if text.chars().any(|c| c.is_ascii_digit()) {
                text.to_owned()
            } else {
                text.to_lowercase()
            };
            if term != word.term {
                return Err(INVALID.into());
            }
        }
        if distinct > 1 {
            linked.push(word);
        }
    }
    if linked.len() != candidates.links.len()
        || linked
            .iter()
            .zip(&candidates.links)
            .any(|(word, link)| word.term != link.shared_term || word.evidence != link.evidence)
    {
        return Err(INVALID.into());
    }
    for folder in &candidates.possible_folders {
        if folder.evidence.is_empty()
            || folder
                .evidence
                .iter()
                .any(|e| e.citation[e.byte_start..e.byte_end] != folder.name)
        {
            return Err(INVALID.into());
        }
    }
    Ok(())
}
pub(crate) fn source_revision(identity: &str, segments: &[Segment]) -> Result<String, String> {
    if identity.len() != 36
        || !identity.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
    {
        return Err("Identité du travail invalide".into());
    }
    if segments.len() > 2_000
        || segments.iter().any(|s| {
            s.text.len() > 4_096
                || s.translated_text.as_ref().is_some_and(|t| t.len() > 16_384)
                || s.speaker_id.as_ref().is_some_and(|id| id.len() > 256)
        })
    {
        return Err(
            "Transcription trop volumineuse pour ces propositions ; aucune fin tronquée".into(),
        );
    }
    let bytes = bounded_json(
        &("parole-topic-source-v1", identity, segments),
        64 * 1024 * 1024,
    )?;
    Ok(sha256_hex(&bytes))
}
fn file_name(revision: &str) -> String {
    format!("topics-lexical-{REVISION}-{revision}.json")
}
fn bounded_json<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, String> {
    struct Buffer {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(io::Error::other("Limite de conservation atteinte"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut buffer, value).map_err(|_| INVALID)?;
    Ok(buffer.bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borne_la_serialisation_avant_de_remplir_la_memoire() {
        assert_eq!(bounded_json(&"é", 4).unwrap(), "\"é\"".as_bytes());
        assert!(bounded_json(&"é", 3).is_err());
        assert!(bounded_json(&"\0".repeat(100), 32).is_err());
    }
}
