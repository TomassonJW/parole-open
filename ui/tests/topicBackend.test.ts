import { beforeEach, describe, expect, it, vi } from 'vitest'
import fixture from './fixtures/topicSnapshot.generated.json'
const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a), isTauri: () => true }))
const { tauriBackend } = await import('../src/lib/backend')
beforeEach(() => invoke.mockReset())
describe('transport Tauri des pistes', () => {
  it('charge absent, prépare et relit avec le seul id, depuis la fixture Rust', async () => {
    invoke.mockResolvedValueOnce(fixture.missing_snapshot).mockResolvedValueOnce(fixture.prepared_snapshot).mockResolvedValueOnce(fixture.loaded_snapshot)
    expect(await tauriBackend.loadTopicCandidates(fixture.job_id)).toEqual(fixture.missing_snapshot)
    expect(await tauriBackend.prepareTopicCandidates(fixture.job_id)).toEqual(fixture.prepared_snapshot)
    expect(await tauriBackend.loadTopicCandidates(fixture.job_id)).toEqual(fixture.loaded_snapshot)
    expect(invoke.mock.calls).toEqual([
      ['load_topic_candidates', { id: fixture.job_id }],
      ['prepare_topic_candidates', { id: fixture.job_id }],
      ['load_topic_candidates', { id: fixture.job_id }],
    ])
  })
  it('refuse une enveloppe illisible et rapporte une commande native absente', async () => {
    invoke.mockResolvedValueOnce({ ...fixture.loaded_snapshot, candidates: undefined })
    await expect(tauriBackend.loadTopicCandidates(fixture.job_id)).rejects.toMatchObject({ kind: 'invalid-response' })
    invoke.mockRejectedValueOnce('Command prepare_topic_candidates not found')
    await expect(tauriBackend.prepareTopicCandidates(fixture.job_id)).rejects.toMatchObject({ kind: 'missing-command' })
  })
})
