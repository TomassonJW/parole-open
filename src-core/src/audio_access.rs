//! Acquisition locale des seuls extraits transcrits et vérifiés.
use crate::audio_access_fs as access_fs;
use crate::{retained_audio::VerifiedAudio, Job};
use access_fs::Directory;
use std::path::Path;
/// Borne de lecture du travail sauvegardé ; aucune découpe silencieuse.
pub const MAX_STATE_BYTES: usize = 64 * 1024 * 1024;
const INVALID: &str = "Extrait audio indisponible ou non vérifiable";
pub struct AudioLibrary {
    root: Directory,
}
/// Bornes issues de la même lecture persistante que les octets vérifiés.
pub struct PlaybackPlan {
    pub audio: VerifiedAudio,
    /// Fin effectivement audible, bornée par le début prévu de la tranche suivante.
    pub playable_end_ms: u64,
    /// Position planifiée, non preuve de l'existence du prochain WAV.
    pub next_ms: Option<u64>,
}
impl AudioLibrary {
    /// Racine choisie par le backend de confiance, jamais un chemin issu du client.
    /// Ses parents appartiennent à ce périmètre de confiance ; la racine finale est épinglée.
    pub fn open(root: &Path) -> Result<Self, String> {
        Ok(Self {
            root: Directory::open(root).map_err(|_| INVALID)?,
        })
    }
    /// Relit un instant depuis l'état disque, puis rend les octets vérifiés eux-mêmes.
    /// Opération bloquante : l'adaptateur doit borner la concurrence et rester hors du fil UI.
    pub fn load_at(&self, id: &str, at_ms: u64) -> Result<VerifiedAudio, String> {
        self.read_at(id, at_ms).map(|(audio, _, _)| audio)
    }
    pub fn plan_at(&self, id: &str, at_ms: u64) -> Result<PlaybackPlan, String> {
        let (audio, planned_end, duration) = self.read_at(id, at_ms)?;
        let actual_end = audio
            .timeline_offset_ms()
            .checked_add(audio.duration_ms())
            .ok_or(INVALID)?;
        let playable_end_ms = actual_end.min(planned_end);
        if at_ms >= playable_end_ms {
            return Err(INVALID.into());
        }
        let next_ms = (planned_end < duration).then_some(planned_end);
        Ok(PlaybackPlan {
            audio,
            playable_end_ms,
            next_ms,
        })
    }
    fn read_at(&self, id: &str, at_ms: u64) -> Result<(VerifiedAudio, u64, u64), String> {
        if !canonical_id(id) {
            return Err(INVALID.into());
        }
        let folder = self.root.child(id).map_err(|_| INVALID)?;
        let state = folder
            .read("travail.json", MAX_STATE_BYTES)
            .map_err(|_| INVALID)?;
        let job: Job = serde_json::from_slice(&state).map_err(|_| INVALID)?;
        drop(state);
        if job.chunk_ms == 0 || at_ms >= job.duration_ms {
            return Err(INVALID.into());
        }
        let index = usize::try_from(at_ms / job.chunk_ms).map_err(|_| INVALID)?;
        let receipt = folder
            .read(
                &format!("tranche-{index:08}.audio.json"),
                crate::retained_audio::MAX_RECEIPT_BYTES,
            )
            .map_err(|_| INVALID)?;
        let bytes = folder
            .read(
                &format!("tranche-{index:08}.wav"),
                crate::retained_audio::MAX_AUDIO_BYTES,
            )
            .map_err(|_| INVALID)?;
        let audio = crate::retained_audio::verify_chunk(&job, id, index, &receipt, bytes)
            .map_err(|_| INVALID)?;
        if at_ms
            .checked_sub(audio.timeline_offset_ms())
            .is_none_or(|relative| relative >= audio.duration_ms())
        {
            return Err(INVALID.into());
        }
        let planned_end = audio
            .timeline_offset_ms()
            .saturating_add(job.chunk_ms)
            .min(job.duration_ms);
        Ok((audio, planned_end, job.duration_ms))
    }
}
fn canonical_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
#[cfg(test)]
#[path = "audio_access_tests.rs"]
mod tests;
