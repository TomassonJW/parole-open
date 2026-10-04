import { afterEach, describe, expect, it, vi } from 'vitest'
import fixture from './fixtures/topicSnapshot.generated.json'
import unicodeFixture from './fixtures/topicSnapshot.unicode.generated.json'
import { sourceRevision, verifyTopicSnapshot } from '../src/lib/topicSnapshot'
import { makeJob } from './fakeBackend'

const job = () => makeJob({ id: fixture.job_id, segments: structuredClone(fixture.segments) })
afterEach(() => vi.unstubAllGlobals())
describe('snapshot émis par Rust', () => {
  it('accepte les mots, liens et dossiers Unicode dans le véritable ordre UTF-8 du moteur', async () => {
    const source = makeJob({ id: unicodeFixture.job_id, segments: structuredClone(unicodeFixture.segments) })
    expect(await sourceRevision(source)).toBe(unicodeFixture.loaded_snapshot.source_revision)
    expect(unicodeFixture.loaded_snapshot.candidates.possible_folders.map(f => f.name)).toEqual(['AＡＢＣＤ', 'A𝐀𝐁𝐂𝐃'])
    for (const raw of [unicodeFixture.prepared_snapshot, unicodeFixture.loaded_snapshot]) {
      const verified = await verifyTopicSnapshot(raw, source)
      expect(verified.candidates).toEqual(raw.candidates)
      expect(verified.entries.find(e => e.kind === 'none')?.passages.map(p => p.index)).toEqual([2])
    }
  })
  it.each(['words', 'folders'] as const)('refuse un ordre %s altéré sans supprimer la validation du tri', async kind => {
    const source = makeJob({ id: unicodeFixture.job_id, segments: structuredClone(unicodeFixture.segments) })
    const altered = structuredClone(unicodeFixture.loaded_snapshot)
    if (kind === 'words') {
      altered.candidates.words.reverse()
      altered.candidates.links.reverse()
    } else altered.candidates.possible_folders.reverse()
    await expect(verifyTopicSnapshot(altered, source)).rejects.toThrow(/non vérifiables/)
  })
  it('calcule le vrai SHA-256 compact et accepte absent, préparé et relu sans perdre les Unicode', async () => {
    expect(await sourceRevision(job())).toBe(fixture.prepared_snapshot.source_revision)
    expect((await verifyTopicSnapshot(fixture.missing_snapshot, job())).candidates).toBeNull()
    for (const raw of [fixture.prepared_snapshot, fixture.loaded_snapshot]) {
      const checked = await verifyTopicSnapshot(raw, job())
      expect(checked.entries.find(e => e.kind === 'none')?.passages.map(p => p.index)).toEqual([2])
      expect(checked.entries.find(e => e.name === 'Étoile')?.passages[0].spans).toEqual([{ start: 11, end: 18 }])
      expect(checked.candidates).toEqual(raw.candidates)
    }
  })
  it('rejette portée étrangère, révision périmée, schémas et preuves altérées', async () => {
    const changed = job(); changed.segments[1].text += '!'
    await expect(verifyTopicSnapshot(fixture.loaded_snapshot, changed)).rejects.toThrow()
    for (const patch of [{ job_id: 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa' }, { source_revision: '0'.repeat(64) }, { schema_version: 2 }, { candidates: { ...fixture.loaded_snapshot.candidates, words: [{ ...fixture.loaded_snapshot.candidates.words[0], evidence: [{ ...fixture.loaded_snapshot.candidates.words[0].evidence[0], byte_end: 35 }] }] } }]) {
      await expect(verifyTopicSnapshot({ ...fixture.loaded_snapshot, ...patch }, job())).rejects.toThrow()
    }
  })
  it('refuse omissions, doublons et passages couverts dans without_suggestion issu du snapshot Rust', async () => {
    for (const without_suggestion of [[], [2, 2], [0, 2], [2, 1]]) {
      const modified = { ...fixture.loaded_snapshot, candidates: { ...fixture.loaded_snapshot.candidates, without_suggestion } }
      await expect(verifyTopicSnapshot(modified, job())).rejects.toThrow()
    }
    expect((await verifyTopicSnapshot(fixture.loaded_snapshot, job())).candidates?.without_suggestion).toEqual([2])
  })
  it('refuse source non sûre, valeurs facultatives mal typées et cryptographie absente', async () => {
    for (const badValue of [-1, 1.5, -0, Number.MAX_SAFE_INTEGER + 1]) {
      const bad = job(); bad.segments[0].start_ms = badValue
      await expect(sourceRevision(bad)).rejects.toThrow()
    }
    const id = job(); id.id = 'ABCDEFAB' + id.id.slice(8)
    await expect(sourceRevision(id)).rejects.toThrow()
    const omitted = job(); delete (omitted.segments[0] as Partial<typeof omitted.segments[0]>).translated_text
    await expect(sourceRevision(omitted)).rejects.toThrow()
    const wrong = job(); wrong.segments[0].speaker_id = 12 as unknown as string
    await expect(sourceRevision(wrong)).rejects.toThrow()
    vi.stubGlobal('crypto', undefined)
    await expect(sourceRevision(job())).rejects.toThrow(/cryptograph|indisponible/i)
  })
  it('mesure le coût local de sérialisation et hash de 1000 passages synthétiques', async () => {
    const large = job(); large.segments = Array.from({ length: 1000 }, (_, i) => ({
      start_ms: i * 1000, end_ms: i * 1000 + 500, text: `Passage synthétique ${i} – ${'mot '.repeat(50)}`,
      speaker_id: null, translated_text: null,
    }))
    const start = performance.now()
    const hash = await sourceRevision(large)
    const elapsed = performance.now() - start
    expect(hash).toMatch(/^[a-f0-9]{64}$/)
    console.info(`topic-source 1000 passages synthétiques (Vitest/jsdom, pas navigateur natif): ${elapsed.toFixed(1)} ms`)
  })
  it('ne confond pas accents composés et décomposés et ignore les noms d’affichage', async () => {
    const a = job(); a.speaker_names = { 'voix-é': 'Autre' }
    expect(await sourceRevision(a)).toBe(fixture.loaded_snapshot.source_revision)
    const b = job(); b.segments[1].text = b.segments[1].text.normalize('NFC')
    expect(await sourceRevision(b)).not.toBe(fixture.loaded_snapshot.source_revision)
  })
})
