//! Preuve locale de correspondance entre une tranche WAV et les paroles conservées.
//!
//! Le consommateur valide les octets qu'il rend, jamais un chemin à rouvrir.
//! L'adaptateur natif devra acquérir ces octets sous sa racine privée, sans liens,
//! avec les limites ci-dessous. Ce module n'expose ni fichier ni serveur réseau.
use crate::{language::sha256_hex, Job, Segment};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

pub const MAX_AUDIO_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RECEIPT_BYTES: usize = 4096;
const TIMING_TOLERANCE_MS: u64 = 200;
const INVALID: &str = "Extrait audio absent, altéré ou incompatible avec cette transcription";
use crate::audio_durability as durability;

#[cfg(test)]
#[path = "retained_audio_tests.rs"]
mod tests;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u8,
    chunk_index: usize,
    timeline_offset_ms: u64,
    requested_duration_ms: u64,
    duration_ms: u64,
    audio_bytes: usize,
    audio_sha256: String,
    job_binding: String,
    first_segment: usize,
    segment_count: usize,
    transcript_sha256: String,
}

/// Octets et repères validés ensemble. Aucun chemin n'est renvoyé.
#[derive(Debug)]
pub struct VerifiedAudio {
    bytes: Vec<u8>,
    timeline_offset_ms: u64,
    duration_ms: u64,
}
impl VerifiedAudio {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
    pub fn timeline_offset_ms(&self) -> u64 {
        self.timeline_offset_ms
    }
    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }
}

/// Refuse les anciens extraits sans reçu, les tranches non confirmées et toute
/// différence de paroles, d'horodatage ou d'octets. Pas de repli sur le média source.
/// Le Job doit être relu par le backend depuis son état persistant ; il ne doit
/// jamais provenir du client ni d'un événement de progression non sauvegardé.
pub fn verify_chunk(
    job: &Job,
    work_identity: &str,
    index: usize,
    receipt_bytes: &[u8],
    wav_bytes: Vec<u8>,
) -> Result<VerifiedAudio, String> {
    let reject = || INVALID.to_string();
    if work_identity.is_empty()
        || work_identity.len() > 255
        || receipt_bytes.len() > MAX_RECEIPT_BYTES
        || wav_bytes.len() > MAX_AUDIO_BYTES
        || job.chunk_ms == 0
        || job.duration_ms == 0
        || index >= job.completed_chunks
        || job.completed_chunks as u64 > job.duration_ms.div_ceil(job.chunk_ms)
    {
        return Err(reject());
    }
    let receipt: Receipt = serde_json::from_slice(receipt_bytes).map_err(|_| reject())?;
    let start = (index as u64)
        .checked_mul(job.chunk_ms)
        .ok_or_else(reject)?;
    let requested = job
        .duration_ms
        .checked_sub(start)
        .ok_or_else(reject)?
        .min(job.chunk_ms);
    let end = receipt
        .first_segment
        .checked_add(receipt.segment_count)
        .ok_or_else(reject)?;
    let segments = job
        .segments
        .get(receipt.first_segment..end)
        .ok_or_else(reject)?;
    if receipt.schema_version != 1
        || receipt.chunk_index != index
        || receipt.timeline_offset_ms != start
        || requested == 0
        || receipt.requested_duration_ms != requested
        || receipt.job_binding != job_binding(job, work_identity)
        || receipt.audio_bytes != wav_bytes.len()
        || receipt.audio_sha256.len() != 64
        || receipt.transcript_sha256.len() != 64
        || receipt.audio_sha256 != sha256_hex(&wav_bytes)
        || receipt.transcript_sha256 != transcript_hash(segments, 0)?
    {
        return Err(reject());
    }
    let duration = wav_duration_ms(&wav_bytes)?;
    if duration != receipt.duration_ms || duration.abs_diff(requested) > TIMING_TOLERANCE_MS {
        return Err(reject());
    }
    validate_times(segments, start, requested, duration)?;
    Ok(VerifiedAudio {
        bytes: wav_bytes,
        timeline_offset_ms: start,
        duration_ms: duration,
    })
}

pub(crate) struct PreparedAudio {
    bytes: Vec<u8>,
    sha256: String,
    len: usize,
    duration_ms: u64,
}

pub(crate) fn job_binding(job: &Job, work_identity: &str) -> String {
    // Ces propriétés restent stables lors d'une traduction, d'un renommage de
    // locuteur, d'un déplacement du média source ou de la génération du rapport.
    sha256_hex(
        &serde_json::to_vec(&(
            work_identity,
            &job.media_name,
            job.duration_ms,
            job.chunk_ms,
        ))
        .unwrap(),
    )
}

pub(crate) fn prepare(wav: &Path, requested_duration: u64) -> Result<PreparedAudio, String> {
    let bytes = read_bounded(wav, MAX_AUDIO_BYTES)?;
    let duration_ms = wav_duration_ms(&bytes)?;
    if duration_ms.abs_diff(requested_duration) > TIMING_TOLERANCE_MS {
        return Err(INVALID.into());
    }
    Ok(PreparedAudio {
        sha256: sha256_hex(&bytes),
        len: bytes.len(),
        duration_ms,
        bytes,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn retain_receipt(
    workspace: &Path,
    index: usize,
    start: u64,
    requested: u64,
    binding: &str,
    first_segment: usize,
    segments: &[Segment],
    prepared: PreparedAudio,
) -> Result<(), String> {
    let current = prepare(
        &workspace.join(format!("tranche-{index:08}.wav")),
        requested,
    )?;
    if prepared.sha256 != current.sha256 || prepared.len != current.len {
        return Err("L'extrait audio a changé pendant la transcription".into());
    }
    validate_times(segments, 0, requested, prepared.duration_ms)?;
    let receipt = Receipt {
        schema_version: 1,
        chunk_index: index,
        timeline_offset_ms: start,
        requested_duration_ms: requested,
        duration_ms: prepared.duration_ms,
        audio_bytes: prepared.len,
        audio_sha256: prepared.sha256,
        job_binding: binding.to_owned(),
        first_segment,
        segment_count: segments.len(),
        transcript_sha256: transcript_hash(segments, start)?,
    };
    let body = serde_json::to_vec(&receipt).map_err(|_| INVALID.to_string())?;
    let destination = workspace.join(format!("tranche-{index:08}.audio.json"));
    // Publier durablement la copie exacte validée, puis le reçu, AVANT la
    // confirmation dans travail.json. Aucun réencodage ni relecture du chemin.
    durability::atomic_write(
        &workspace.join(format!("tranche-{index:08}.wav")),
        &current.bytes,
    )
    .map_err(|_| "Conservation durable de l'extrait audio impossible".to_string())?;
    // Un reçu orphelin n'autorise rien : verify_chunk exige completed_chunks.
    durability::atomic_write(&destination, &body)
        .map_err(|_| "Conservation durable de la preuve audio impossible".to_string())
}

fn validate_times(
    segments: &[Segment],
    origin: u64,
    requested: u64,
    audio: u64,
) -> Result<(), String> {
    for segment in segments {
        let start = segment.start_ms.checked_sub(origin).ok_or(INVALID)?;
        let end = segment.end_ms.checked_sub(origin).ok_or(INVALID)?;
        // Une fin peut dépasser de peu à cause de la précision de Whisper,
        // mais le début du passage doit exister dans le WAV réellement conservé.
        if end < start
            || start >= audio
            || end > audio.saturating_add(TIMING_TOLERANCE_MS)
            || end > requested.saturating_add(TIMING_TOLERANCE_MS)
        {
            return Err("Horodatages incompatibles avec la durée audio".into());
        }
    }
    Ok(())
}

fn transcript_hash(segments: &[Segment], offset: u64) -> Result<String, String> {
    let mut records = Vec::with_capacity(segments.len());
    for segment in segments {
        let start = segment.start_ms.checked_add(offset).ok_or(INVALID)?;
        let end = segment.end_ms.checked_add(offset).ok_or(INVALID)?;
        records.push((start, end, &segment.text));
    }
    Ok(sha256_hex(
        &serde_json::to_vec(&records).map_err(|_| INVALID)?,
    ))
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| INVALID)?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(INVALID.into());
    }
    let file = File::open(path).map_err(|_| INVALID)?;
    if !file.metadata().map_err(|_| INVALID)?.is_file() {
        return Err(INVALID.into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| INVALID)?;
    if bytes.len() > limit {
        return Err(INVALID.into());
    }
    Ok(bytes)
}

// FFmpeg émet un RIFF/WAVE PCM16 mono 16 kHz ; des blocs LIST/JUNK sont licites.
// Toute taille vient des octets bornés, jamais d'une durée déclarative du média.
fn wav_duration_ms(bytes: &[u8]) -> Result<u64, String> {
    if bytes.len() < 44
        || bytes.len() > MAX_AUDIO_BYTES
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
    {
        return Err(INVALID.into());
    }
    let number =
        |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    if number(4).checked_add(8) != Some(bytes.len()) {
        return Err(INVALID.into());
    }
    let mut position = 12usize;
    let mut format = false;
    let mut data = None;
    while position < bytes.len() {
        let begin = position.checked_add(8).ok_or(INVALID)?;
        if begin > bytes.len() {
            return Err(INVALID.into());
        }
        let length = number(position + 4);
        let end = begin.checked_add(length).ok_or(INVALID)?;
        let block = bytes.get(begin..end).ok_or(INVALID)?;
        match &bytes[position..position + 4] {
            b"fmt " => {
                if format
                    || block.len() < 16
                    || block[..16] != [1, 0, 1, 0, 128, 62, 0, 0, 0, 125, 0, 0, 2, 0, 16, 0]
                {
                    return Err(INVALID.into());
                }
                format = true;
            }
            b"data" => {
                if data.is_some() || length == 0 || length % 2 != 0 {
                    return Err(INVALID.into());
                }
                data = Some(length);
            }
            _ => (),
        }
        position = end.checked_add(length % 2).ok_or(INVALID)?;
    }
    if !format || position != bytes.len() {
        return Err(INVALID.into());
    }
    let duration = data.ok_or(INVALID)? as u64 * 1000 / 32_000;
    if duration == 0 {
        return Err(INVALID.into());
    }
    Ok(duration)
}
