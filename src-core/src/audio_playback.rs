//! Enveloppe binaire atomique du seul WAV vérifié, sans chemin ni JSON d'octets.
use crate::{audio_access::AudioLibrary, retained_audio::MAX_AUDIO_BYTES};

const MAX_SAFE_JS: u64 = (1 << 53) - 1;
const INVALID: &str = "Extrait audio indisponible ou non vérifiable";
/// PAU1 + UUID ASCII canonique + cinq u64 LE + longueur u32 LE + WAV.
pub const HEADER_BYTES: usize = 84;

pub fn encode(library: &AudioLibrary, id: &str, at_ms: u64) -> Result<Vec<u8>, String> {
    if at_ms > MAX_SAFE_JS {
        return Err(INVALID.into());
    }
    let plan = library.plan_at(id, at_ms)?;
    let offset = plan.audio.timeline_offset_ms();
    let duration = plan.audio.duration_ms();
    if [
        offset,
        duration,
        plan.playable_end_ms,
        plan.next_ms.unwrap_or(0),
    ]
    .iter()
    .any(|value| *value > MAX_SAFE_JS)
        || id.len() != 36
        || plan.audio.bytes().len() > MAX_AUDIO_BYTES
    {
        return Err(INVALID.into());
    }
    let bytes = plan.audio.into_bytes();
    let mut result = Vec::with_capacity(HEADER_BYTES + bytes.len());
    result.extend_from_slice(b"PAU1");
    result.extend_from_slice(id.as_bytes());
    for value in [
        at_ms,
        offset,
        duration,
        plan.playable_end_ms,
        plan.next_ms.unwrap_or(u64::MAX),
    ] {
        result.extend_from_slice(&value.to_le_bytes());
    }
    result.extend_from_slice(
        &u32::try_from(bytes.len())
            .map_err(|_| INVALID)?
            .to_le_bytes(),
    );
    result.extend_from_slice(&bytes);
    Ok(result)
}
