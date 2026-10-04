import { describe, expect, it } from 'vitest'
import { chunkCount, formatClock, formatDuration, progressRatio, speakerLabel, isSupportedMedia, baseName } from '../src/lib/format'

describe('format', () => {
  it('calcule les tranches comme le moteur Rust', () => {
    expect(chunkCount({ duration_ms: 65_000, chunk_ms: 30_000 })).toBe(3)
    expect(chunkCount({ duration_ms: 60_000, chunk_ms: 0 })).toBe(0)
    expect(progressRatio({ duration_ms: 60_000, chunk_ms: 30_000, completed_chunks: 1 })).toBe(0.5)
    expect(progressRatio({ duration_ms: 60_000, chunk_ms: 30_000, completed_chunks: 9 })).toBe(1)
  })
  it('formate les durées en français', () => {
    expect(formatClock(65_000)).toBe('01:05')
    expect(formatClock(3_725_000)).toBe('01:02:05')
    expect(formatDuration(3_900_000)).toBe('1 h 05 min')
    expect(formatDuration(45_000)).toBe('45 s')
  })
  it('n’invente pas de locuteur', () => {
    expect(speakerLabel({ speaker_names: {} }, { speaker_id: null })).toBe('Locuteur non attribué')
    expect(speakerLabel({ speaker_names: { S1: 'Alice' } }, { speaker_id: 'S1' })).toBe('Alice')
  })
  it.each(['__proto__', 'constructor', 'toString'])('affiche une clé réservée sans inventer de nom : %s', speakerId => {
    expect(speakerLabel({ speaker_names: {} }, { speaker_id: speakerId })).toBe(speakerId)
  })
  it.each(['__proto__', 'constructor', 'toString'])('préserve un vrai nom explicitement associé à %s', speakerId => {
    const names = Object.fromEntries([[speakerId, ' Camille ']])
    const before = JSON.stringify(names)
    expect(speakerLabel({ speaker_names: names }, { speaker_id: speakerId })).toBe('Camille')
    expect(JSON.stringify(names)).toBe(before)
  })
  it('ignore un nom hérité même quand il est une chaîne', () => {
    const names: Record<string, string> = Object.create({ A: 'Nom non enregistré' })
    expect(speakerLabel({ speaker_names: names }, { speaker_id: 'A' })).toBe('A')
  })
  it('reconnaît les médias et les chemins Windows', () => {
    expect(isSupportedMedia('C:\\audio\\Réunion.M4A')).toBe(true)
    expect(isSupportedMedia('/x/notes.pdf')).toBe(false)
    expect(baseName('C:\\audio\\a.wav')).toBe('a.wav')
  })
})
