import type { PlaybackSource } from './playbackTime'

export const AUDIO_PACKET_HEADER = 84
export const MAX_WAV_BYTES = 32 * 1024 * 1024
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/
export type AudioChunk = PlaybackSource & { playableEndMs: number; nextMs: number | null; requestedAtMs: number }
export type DecodedAudio = Omit<AudioChunk, 'src'> & { wav: Uint8Array<ArrayBuffer> }

/** Vue audible du WAV vérifié. Ne modifie ni le paquet ni les fichiers conservés.
 * Borner les échantillons eux-mêmes évite de dépendre de timeupdate, qui peut arriver tard.
 */
export function playableWav(chunk: DecodedAudio): Uint8Array<ArrayBuffer> {
  const invalid = () => { throw new Error('Extrait audio local invalide.') }
  const wav = chunk.wav
  if (wav.byteLength < 44 || wav.byteLength > MAX_WAV_BYTES) return invalid()
  const view = new DataView(wav.buffer, wav.byteOffset, wav.byteLength)
  const tag = (at: number) => String.fromCharCode(...wav.subarray(at, at + 4))
  if (tag(0) !== 'RIFF' || tag(8) !== 'WAVE' || view.getUint32(4, true) !== wav.byteLength - 8) return invalid()
  let format = false, dataStart = -1, dataLength = 0, offset = 12
  while (offset < wav.byteLength) {
    if (offset + 8 > wav.byteLength) return invalid()
    const size = view.getUint32(offset + 4, true), start = offset + 8
    const next = start + size + (size % 2)
    if (next > wav.byteLength) return invalid()
    if (tag(offset) === 'fmt ') {
      if (format || size < 16 || view.getUint16(start, true) !== 1 || view.getUint16(start + 2, true) !== 1 ||
          view.getUint32(start + 4, true) !== 16000 || view.getUint32(start + 8, true) !== 32000 ||
          view.getUint16(start + 12, true) !== 2 || view.getUint16(start + 14, true) !== 16) return invalid()
      format = true
    } else if (tag(offset) === 'data') {
      if (dataStart !== -1 || size % 2 !== 0) return invalid()
      dataStart = start; dataLength = size
    }
    offset = next
  }
  const milliseconds = chunk.playableEndMs - chunk.timelineOffsetMs
  const keep = milliseconds * 32
  if (!format || dataStart === -1 || !safeMs(milliseconds) || milliseconds === 0 ||
      !Number.isSafeInteger(keep) || keep > dataLength || Math.floor(dataLength / 32) !== chunk.durationMs) return invalid()
  if (keep === dataLength) return wav
  // Conteneur PCM16 mono 16 kHz canonique ; tous les échantillons conservés sont copiés à l'identique.
  const bounded = new Uint8Array(44 + keep), header = new DataView(bounded.buffer)
  bounded.set(new TextEncoder().encode('RIFF'), 0); header.setUint32(4, bounded.byteLength - 8, true)
  bounded.set(new TextEncoder().encode('WAVEfmt '), 8); header.setUint32(16, 16, true)
  header.setUint16(20, 1, true); header.setUint16(22, 1, true); header.setUint32(24, 16000, true)
  header.setUint32(28, 32000, true); header.setUint16(32, 2, true); header.setUint16(34, 16, true)
  bounded.set(new TextEncoder().encode('data'), 36); header.setUint32(40, keep, true)
  bounded.set(wav.subarray(dataStart, dataStart + keep), 44)
  return bounded
}

export function safeMs(value: number): boolean {
  return Number.isSafeInteger(value) && value >= 0
}

export function decodeAudioPacket(raw: unknown, id: string, atMs: number): DecodedAudio {
  const invalid = () => { throw new Error('Réponse audio locale invalide.') }
  if (!UUID.test(id) || !safeMs(atMs) || !(raw instanceof ArrayBuffer) || raw.byteLength < AUDIO_PACKET_HEADER || raw.byteLength > AUDIO_PACKET_HEADER + MAX_WAV_BYTES) return invalid()
  const view = new DataView(raw)
  if (String.fromCharCode(...new Uint8Array(raw, 0, 4)) !== 'PAU1') return invalid()
  if (String.fromCharCode(...new Uint8Array(raw, 4, 36)) !== id) return invalid()
  const read = (offset: number): number => {
    const number = view.getBigUint64(offset, true)
    if (number > BigInt(Number.MAX_SAFE_INTEGER)) return invalid()
    return Number(number)
  }
  const requestedAtMs = read(40), timelineOffsetMs = read(48), durationMs = read(56), playableEndMs = read(64)
  const rawNext = view.getBigUint64(72, true)
  const nextMs = rawNext === BigInt('18446744073709551615') ? null : read(72)
  const length = view.getUint32(80, true)
  if (length < 44 || length > MAX_WAV_BYTES || raw.byteLength !== AUDIO_PACKET_HEADER + length ||
    requestedAtMs !== atMs || !safeMs(timelineOffsetMs) || !safeMs(durationMs) || durationMs === 0 ||
    !safeMs(playableEndMs) || playableEndMs <= timelineOffsetMs ||
    !Number.isSafeInteger(timelineOffsetMs + durationMs) ||
    playableEndMs > timelineOffsetMs + durationMs ||
    atMs < timelineOffsetMs || atMs >= playableEndMs ||
    (nextMs !== null && (!safeMs(nextMs) || nextMs <= timelineOffsetMs || playableEndMs > nextMs))) return invalid()
  const wav = new Uint8Array(raw, AUDIO_PACKET_HEADER, length)
  if (String.fromCharCode(...wav.subarray(0, 4)) !== 'RIFF' || String.fromCharCode(...wav.subarray(8, 12)) !== 'WAVE') return invalid()
  // Copy out of the IPC buffer: no mutable payload may change after validation.
  return { requestedAtMs, timelineOffsetMs, durationMs, playableEndMs, nextMs, wav: new Uint8Array(wav) }
}
