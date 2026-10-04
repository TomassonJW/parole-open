import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useTranscriptSearch } from '../src/hooks/useTranscriptSearch'
import { createTranscriptIndex, searchTranscriptIndex } from '../src/lib/transcriptSearch'
import { makeJob } from './fakeBackend'

class ControlledWorker {
  static instances: ControlledWorker[] = []
  onmessage: ((event: MessageEvent) => void) | null = null
  onerror: (() => void) | null = null
  messages: any[] = []
  terminated = false
  constructor() { ControlledWorker.instances.push(this) }
  postMessage(message: any) { this.messages.push(message) }
  terminate() { this.terminated = true }
  respond(message: any) { this.onmessage?.({ data: message } as MessageEvent) }
}
const passage = (text: string) => ({ text, start_ms: 0, end_ms: 1000, speaker_id: 'A', translated_text: 'bonjour' })
afterEach(() => { vi.unstubAllGlobals(); ControlledWorker.instances = [] })
describe('transport de recherche', () => {
  it('ignore la réponse A après B et invalide au changement de langue et de travail', async () => {
    vi.stubGlobal('Worker', ControlledWorker)
    const job = makeJob({ segments: [passage('alpha beta')] })
    const { result, rerender, unmount } = renderHook(({ q, translated, id }) => useTranscriptSearch({ ...job, id }, translated, q, true), { initialProps: { q: 'alpha', translated: false, id: 'a' } })
    const worker = ControlledWorker.instances[0]
    await waitFor(() => expect(worker.messages.some(m => m.type === 'search')).toBe(true))
    const a = worker.messages.find(m => m.type === 'search')
    rerender({ q: 'beta', translated: false, id: 'a' })
    await waitFor(() => expect(worker.messages.filter(m => m.type === 'search')).toHaveLength(2))
    const b = worker.messages.filter(m => m.type === 'search').at(-1)
    const index = createTranscriptIndex(job.segments)
    act(() => worker.respond({ type: 'results', revision: b.revision, request: b.request, matches: searchTranscriptIndex(index, 'beta') }))
    expect(result.current.matches[0].spans[0].start).toBe(6)
    act(() => worker.respond({ type: 'results', revision: a.revision, request: a.request, matches: searchTranscriptIndex(index, 'alpha') }))
    expect(result.current.matches[0].spans[0].start).toBe(6)
    rerender({ q: 'bonjour', translated: true, id: 'a' })
    expect(worker.terminated).toBe(true)
    const translatedWorker = ControlledWorker.instances.at(-1)!
    expect(translatedWorker.messages[0].translated).toBe(true)
    rerender({ q: 'bonjour', translated: true, id: 'b' })
    expect(translatedWorker.terminated).toBe(true)
    unmount()
    expect(ControlledWorker.instances.at(-1)!.terminated).toBe(true)
  })
  it('affiche une erreur française sans Worker au lieu de résultats fictifs', async () => {
    vi.stubGlobal('Worker', undefined)
    const job = makeJob({ segments: [passage('mot')] })
    const { result } = renderHook(() => useTranscriptSearch(job, false, 'mot', true))
    await waitFor(() => expect(result.current.error).toMatch(/recherche.*indisponible/i))
    expect(result.current.matches).toEqual([])
  })
})
