import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { AUDIO_PACKET_HEADER, decodeAudioPacket, MAX_WAV_BYTES, playableWav } from '../src/lib/audioPacket'

const path = process.env.PAROLE_AUDIO_FIXTURE_OUTPUT
const producer = path ? readFileSync(path) : null
const ID = '11111111-1111-4111-8111-111111111111'
const copy = () => Uint8Array.from(producer!).buffer

describe.skipIf(!producer)('contrat de l’enveloppe produite par Rust depuis le reçu et le WAV', () => {
  it('conserve exactement les octets vérifiés de la fixture du producteur', () => {
    const packet = copy()
    const decoded = decodeAudioPacket(packet, ID, 1250)
    expect(decoded).toMatchObject({ timelineOffsetMs: 1000, durationMs: 1000, playableEndMs: 2000, nextMs: 2000, requestedAtMs: 1250 })
    expect(Buffer.from(decoded.wav)).toEqual(producer!.subarray(AUDIO_PACKET_HEADER))
  })
  it('borne la dernière tranche sans changer un seul échantillon ni le paquet source', () => {
    const raw = Uint8Array.from(readFileSync(`${path}.overlap-last`)).buffer
    const before = raw.slice(0), decoded = decodeAudioPacket(raw, ID, 900)
    const bounded = playableWav(decoded)
    expect(new DataView(bounded.buffer).getUint32(40, true)).toBe(900 * 32)
    expect(bounded.subarray(44)).toEqual(decoded.wav.subarray(44, 44 + 900 * 32))
    expect(raw).toEqual(before)
  })
  it('conserve le WAV entier si la borne et les échantillons sont déjà identiques', () => {
    const decoded = decodeAudioPacket(copy(), ID, 1250)
    expect(playableWav(decoded)).toBe(decoded.wav)
  })
  it('lit les blocs auxiliaires réels du producteur avant et après les échantillons', () => {
    const raw = Uint8Array.from(readFileSync(`${path}.auxiliary`)).buffer
    const before = raw.slice(0), decoded = decodeAudioPacket(raw, ID, 900)
    const bounded = playableWav(decoded)
    expect(new DataView(bounded.buffer).getUint32(40, true)).toBe(900 * 32)
    expect(bounded.subarray(44)).toEqual(decoded.wav.subarray(56, 56 + 900 * 32))
    expect(raw).toEqual(before)
  })
  it('refuse les mutations de format, taille ou borne au lieu de produire un faux WAV', () => {
    const broken = (mutate: (v: DataView) => void) => {
      const decoded = decodeAudioPacket(copy(), ID, 1250)
      mutate(new DataView(decoded.wav.buffer))
      expect(() => playableWav(decoded)).toThrow('Extrait audio local invalide.')
    }
    broken(v => v.setUint16(20, 3, true)) // pas PCM
    broken(v => v.setUint16(22, 2, true)) // pas mono
    broken(v => v.setUint32(24, 8000, true))
    broken(v => v.setUint32(40, 32001, true))
    broken(v => v.setUint32(4, 0, true))
    const decoded = decodeAudioPacket(copy(), ID, 1250)
    expect(() => playableWav({ ...decoded, playableEndMs: 2001 })).toThrow()
    expect(() => playableWav({ ...decoded, playableEndMs: 1999.5 })).toThrow()
  })
  it('rejette version, travail, instant, taille, troncature et entiers hors précision' , () => {
    const broken = (edit: (bytes: Uint8Array, view: DataView) => void) => {
      const raw = copy(); edit(new Uint8Array(raw), new DataView(raw)); expect(() => decodeAudioPacket(raw, ID, 1250)).toThrow('Réponse audio locale invalide.')
    }
    broken((bytes) => { bytes[3] = 50 })
    broken((bytes) => { bytes[4] = 48 })
    broken((_bytes, view) => { view.setBigUint64(40, 1200n, true) })
    broken((_bytes, view) => { view.setUint32(80, MAX_WAV_BYTES + 1, true) })
    broken((_bytes, view) => { view.setBigUint64(56, BigInt(Number.MAX_SAFE_INTEGER) + 1n, true) })
    broken((_bytes, view) => { view.setBigUint64(64, 3000n, true) })
    broken((_bytes, view) => { view.setBigUint64(72, 1500n, true) })
    expect(() => decodeAudioPacket(copy().slice(0, -1), ID, 1250)).toThrow()
    expect(() => decodeAudioPacket(new ArrayBuffer(AUDIO_PACKET_HEADER + MAX_WAV_BYTES + 1), ID, 1250)).toThrow()
    expect(() => decodeAudioPacket(Array.from(producer!), ID, 1250)).toThrow()
  })
})
