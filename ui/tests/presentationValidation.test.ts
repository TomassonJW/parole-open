import { describe, expect, it } from 'vitest'
import { defaultPreferences, parsePresentationState, validateSnapshot, sourceRevision, validatePreferences } from '../src/lib/transcriptPresentation'
import { makeJob } from './fakeBackend'
import generated from './fixtures/transcriptPresentation.generated.json'
import type { Job } from '../src/lib/types'

describe('contrat de présentation', () => {
  it('refuse les valeurs et couleurs incohérentes sans coercition', () => {
    expect(() => parsePresentationState({ preferences: { ...defaultPreferences(), screen: { mode: 'fluid', pause_ms: 250, show_timestamps: true } }, revision: 0, warning: null, writable: true })).toThrow()
    expect(() => parsePresentationState({ preferences: { ...defaultPreferences(), speaker_colors: { alien: '#aabbcc' } }, revision: 0, warning: null, writable: true }, makeJob())).toThrow()
    expect(() => parsePresentationState({ preferences: defaultPreferences(), revision: 0, warning: null, writable: true })).not.toThrow()
  })
  it('hachage tous les segments, partition complète et source/options exactes', async () => {
    const job = makeJob({ segments: [
      { start_ms: 0, end_ms: 100, text: 'a', speaker_id: 'A', translated_text: null },
      { start_ms: 200, end_ms: 300, text: 'b', speaker_id: 'A', translated_text: 'B' },
    ] })
    const options = defaultPreferences().screen
    const revision = await sourceRevision(job)
    const snapshot = { schema_version: 1, job_id: job.id, source_revision: revision, options, blocks: [{ segment_indices: [0, 1], start_ms: 0, end_ms: 300, speaker_id: 'A' }] }
    expect(validateSnapshot(snapshot, job, revision, options).blocks).toHaveLength(1)
    expect(() => validateSnapshot({ ...snapshot, blocks: [{ ...snapshot.blocks[0], segment_indices: [0] }] }, job, revision, options)).toThrow()
    expect(() => validateSnapshot({ ...snapshot, source_revision: '0'.repeat(64) }, job, revision, options)).toThrow()
    expect(await sourceRevision({ ...job, segments: [...job.segments, { ...job.segments[0], text: 'c' }] })).not.toBe(revision)
  })
  it('rejette couleurs non textuelles et identifiants interdits en octets UTF-8', () => {
    for (const [id, color] of [['A', 1234567], ['   ', '#aabbcc'], ['A\u0001', '#aabbcc'], ['é'.repeat(65), '#aabbcc']]) {
      expect(() => validatePreferences({ ...defaultPreferences(), speaker_colors: { [id]: color } })).toThrow()
    }
    expect(() => validatePreferences({ ...defaultPreferences(), speaker_colors: { ['é'.repeat(64)]: '#AaBbCc' } })).not.toThrow()
  })
  it('consomme la projection et les octets du producteur Rust, puis refuse les mutations', async () => {
    const job = generated.job as unknown as Job
    const projection = generated.projection
    const revision = await sourceRevision(job)
    expect(revision).toBe(projection.source_revision)
    const preferences = validatePreferences(generated.preferences, job)
    expect(preferences).toEqual(generated.preferences)
    expect(validateSnapshot(projection, job, revision, preferences.screen).blocks.map(b => b.segment_indices)).toEqual([[0, 1], [2], [3]])
    const rejected = (bad: unknown, target = job) => expect(() => validateSnapshot(bad, target, revision, preferences.screen)).toThrow()
    rejected({ ...projection, blocks: projection.blocks.slice(0, -1) })
    rejected({ ...projection, blocks: [{ ...projection.blocks[0], segment_indices: [0, 0] }, ...projection.blocks.slice(1)] })
    rejected({ ...projection, blocks: [{ segment_indices: [0], start_ms: 0, end_ms: 1000, speaker_id: 'a' }, { segment_indices: [1, 2], start_ms: 3000, end_ms: 7000, speaker_id: 'a' }, projection.blocks[2]] })
    rejected({ ...projection, blocks: [{ ...projection.blocks[0], speaker_id: 'b' }, ...projection.blocks.slice(1)] })
    rejected({ ...projection, blocks: [{ ...projection.blocks[0], start_ms: 1 }, ...projection.blocks.slice(1)] })
    rejected({ ...projection, options: { ...projection.options, pause_ms: 500 } })
    rejected({ ...projection, job_id: 'autre' })
    const altered = { ...job, segments: job.segments.map((s, i) => i === 0 ? { ...s, text: 'altéré' } : s) }
    const alteredRevision = await sourceRevision(altered)
    expect(() => validateSnapshot(projection, altered, alteredRevision, preferences.screen)).toThrow()
  })
  it('préserve les temps inversés Rust uniquement dans un bloc solo exact', async () => {
    const job = makeJob({ segments: [
      { start_ms: 300, end_ms: 100, text: 'inversé', speaker_id: 'A', translated_text: null },
      { start_ms: 400, end_ms: 500, text: 'suite', speaker_id: 'A', translated_text: null },
    ] })
    const options = defaultPreferences().screen
    const revision = await sourceRevision(job)
    const snapshot = { schema_version: 1, job_id: job.id, source_revision: revision, options, blocks: [
      { segment_indices: [0], start_ms: 300, end_ms: 100, speaker_id: 'A' },
      { segment_indices: [1], start_ms: 400, end_ms: 500, speaker_id: 'A' },
    ] }
    expect(validateSnapshot(snapshot, job, revision, options)).toBe(snapshot)
    expect(() => validateSnapshot({ ...snapshot, blocks: [{ segment_indices: [0, 1], start_ms: 300, end_ms: 500, speaker_id: 'A' }] }, job, revision, options)).toThrow()
  })
})
