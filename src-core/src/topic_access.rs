//! Accès local au cache lexical depuis une identité de travail, sans source fournie par la fenêtre.
use crate::{
    audio_access::MAX_STATE_BYTES,
    audio_access_fs::Directory,
    topic_cache::{source_revision, TopicCache},
    topic_candidates::TopicCandidates,
    Job,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const INVALID: &str = "Travail ou propositions indisponibles ou non vérifiables";
const STALE: &str = "La transcription a changé pendant la consultation des propositions";
const CACHE_FOLDER: &str = "topics-lexical-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicSnapshot {
    pub schema_version: u32,
    pub job_id: String,
    pub source_revision: String,
    pub candidates: Option<TopicCandidates>,
}

/// La racine est choisie par le backend et ses parents sont de confiance.
/// Le namespace des caches doit rester privé et stable pendant les écritures.
pub struct TopicLibrary {
    root: Directory,
    path: PathBuf,
}
impl TopicLibrary {
    pub fn open(root: &Path) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err(INVALID.into());
        }
        Ok(Self {
            root: Directory::open(root).map_err(|_| INVALID)?,
            path: root.to_path_buf(),
        })
    }
    pub fn load(&self, id: &str) -> Result<TopicSnapshot, String> {
        self.snapshot(id, false)
    }
    pub fn prepare(&self, id: &str) -> Result<TopicSnapshot, String> {
        self.snapshot(id, true)
    }
    fn snapshot(&self, id: &str, prepare: bool) -> Result<TopicSnapshot, String> {
        // Valider l'identité avant toute traversée. Même validation que celle de TopicCache.
        source_revision(id, &[])?;
        let folder = self.root.child(id).map_err(|_| INVALID)?;
        let job = read_job(&folder)?;
        let source_rev = source_revision(id, &job.segments)?;
        let cache = TopicCache::new(&self.path.join(id).join(CACHE_FOLDER));
        let candidates = if prepare {
            Some(cache.prepare(id, &job.segments)?)
        } else {
            cache.load(id, &job.segments)?
        };
        let current = read_job(&folder)?;
        if source_revision(id, &current.segments)? != source_rev {
            return Err(STALE.into());
        }
        Ok(TopicSnapshot {
            schema_version: 1,
            job_id: id.to_owned(),
            source_revision: source_rev,
            candidates,
        })
    }
}
fn read_job(folder: &Directory) -> Result<Job, String> {
    let bytes = folder
        .read("travail.json", MAX_STATE_BYTES)
        .map_err(|_| INVALID)?;
    serde_json::from_slice(&bytes).map_err(|_| INVALID.into())
}
