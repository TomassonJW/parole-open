import { StrictMode, Suspense, startTransition, useState } from 'react'
import { act, render, renderHook, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import fixture from './fixtures/topicSnapshot.generated.json'
import unicodeFixture from './fixtures/topicSnapshot.unicode.generated.json'
import { fakeBackend, makeJob } from './fakeBackend'
import { useTopicCandidates } from '../src/hooks/useTopicCandidates'
import type { TopicSnapshot } from '../src/lib/topicSnapshot'
const missing = fixture.missing_snapshot as TopicSnapshot
const prepared = fixture.prepared_snapshot as TopicSnapshot

const job = () => makeJob({ id: fixture.job_id, segments: structuredClone(fixture.segments) })
const deferred = <T,>() => { let resolve!: (v: T) => void; let reject!: (e: Error) => void; const promise = new Promise<T>((ok, bad) => { resolve = ok; reject = bad }); return { promise, resolve, reject } }

describe('pistes sauvegardées bornées à la source', () => {
  it.each([false, true])('prépare la source affichée ; autre rendu suspendu=%s', async suspend => {
    const shown = makeJob({ id: fixture.job_id, segments: structuredClone(fixture.segments) })
    const other = { ...shown, id: '55555555-5555-4555-8555-555555555555' }
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => fixture.loaded_snapshot as TopicSnapshot), prepareTopicCandidates: vi.fn(async () => fixture.prepared_snapshot as TopicSnapshot) })
    const waitForever = new Promise<void>(() => {})
    let suspendedRenders = 0
    function Panel() {
      const [next, setNext] = useState(false)
      const topic = useTopicCandidates(b.backend, next ? other : shown, true)
      if (next) { suspendedRenders++; throw waitForever }
      return <><p data-testid="visible-job">{shown.id}</p><p data-testid="status">{topic.status}</p>
        <button onClick={() => startTransition(() => setNext(true))}>Changer de travail</button>
        <button disabled={topic.status !== 'ready'} onClick={() => { void topic.prepare() }}>Préparer les pistes affichées</button></>
    }
    render(<Suspense fallback={<p>Autre travail en attente</p>}><Panel /></Suspense>)
    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('ready'))
    if (suspend) {
      await userEvent.click(screen.getByRole('button', { name: 'Changer de travail' }))
      await waitFor(() => expect(suspendedRenders).toBeGreaterThan(0))
    }
    expect(screen.getByTestId('visible-job')).toHaveTextContent(shown.id)
    expect(screen.getByTestId('status')).toHaveTextContent('ready')
    expect(screen.queryByText('Autre travail en attente')).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Préparer les pistes affichées' }))
    await waitFor(() => expect(b.backend.prepareTopicCandidates).toHaveBeenCalledExactlyOnceWith(shown.id))
    expect(b.backend.prepareTopicCandidates).not.toHaveBeenCalledWith(other.id)
    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('ready'))
    expect(screen.getByTestId('visible-job')).toHaveTextContent(shown.id)

  })


  it('refuse une ancienne commande après engagement du prochain travail, sans perturber sa portée', async () => {
    const initial = job()
    const next = makeJob({ id: unicodeFixture.job_id, segments: structuredClone(unicodeFixture.segments) })
    const nextPrepared = unicodeFixture.prepared_snapshot as TopicSnapshot
    const b = fakeBackend({
      loadTopicCandidates: vi.fn(async (id: string) => id === initial.id ? prepared : nextPrepared),
      prepareTopicCandidates: vi.fn(async (id: string) => id === initial.id ? prepared : nextPrepared),
    })
    const { result, rerender } = renderHook(({ j }) => useTopicCandidates(b.backend, j, true), { initialProps: { j: initial } })
    await waitFor(() => expect(result.current.status).toBe('ready'))
    const formerPrepare = result.current.prepare
    rerender({ j: next })
    await waitFor(() => expect(result.current.status).toBe('ready'))
    expect(result.current.snapshot?.job_id).toBe(next.id)
    const currentSnapshot = result.current.snapshot
    await act(async () => { await formerPrepare() })
    expect(b.backend.prepareTopicCandidates).not.toHaveBeenCalled()
    expect(result.current.status).toBe('ready')
    expect(result.current.snapshot).toBe(currentSnapshot)
    await act(async () => { await result.current.prepare() })
    expect(b.backend.prepareTopicCandidates).toHaveBeenCalledExactlyOnceWith(next.id)
    expect(result.current.status).toBe('ready')
  })

  it('lit absent, prépare seulement sur action, conserve les preuves réelles, ne recharge pas les noms', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => missing), prepareTopicCandidates: vi.fn(async () => prepared) })
    const initial = job()
    const { result, rerender } = renderHook(({ j }) => useTopicCandidates(b.backend, j, true), { initialProps: { j: initial } })
    await waitFor(() => expect(result.current.status).toBe('absent'))
    expect(b.backend.prepareTopicCandidates).not.toHaveBeenCalled()
    rerender({ j: { ...initial, speaker_names: { voix: 'Luc' } } })
    expect(b.backend.loadTopicCandidates).toHaveBeenCalledTimes(1)
    await act(async () => { await result.current.prepare() })
    expect(result.current.status).toBe('ready')
    expect(result.current.snapshot?.entries.find(e => e.kind === 'none')?.passages.map(p => p.index)).toEqual([2])
    expect(b.backend.prepareTopicCandidates).toHaveBeenCalledTimes(1)
  })
  it('ignore A quand B arrive à id constant avec segments changés, sans rafale native', async () => {
    const a = deferred<typeof missing>()
    const b = fakeBackend({ loadTopicCandidates: vi.fn().mockReturnValueOnce(a.promise).mockResolvedValue(missing) })
    const initial = job()
    const { result, rerender } = renderHook(({ j }) => useTopicCandidates(b.backend, j, true), { initialProps: { j: initial } })
    await waitFor(() => expect(b.backend.loadTopicCandidates).toHaveBeenCalledTimes(1))
    const changed = job(); changed.segments[0].text += '!'
    rerender({ j: changed })
    expect(result.current.snapshot).toBeNull()
    expect(b.backend.loadTopicCandidates).toHaveBeenCalledTimes(1)
    await act(async () => { a.resolve(missing); await a.promise })
    await waitFor(() => expect(b.backend.loadTopicCandidates).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(result.current.status).toBe('stale'))
    expect(result.current.snapshot).toBeNull()
  })
  it('sérialise les demandes et abandonne la préparation différée après désactivation', async () => {
    const a = deferred<typeof missing>()
    const b = fakeBackend({ loadTopicCandidates: vi.fn(() => a.promise), prepareTopicCandidates: vi.fn(async () => prepared) })
    const { result, rerender } = renderHook(({ enabled }) => useTopicCandidates(b.backend, job(), enabled), { initialProps: { enabled: true } })
    await waitFor(() => expect(b.backend.loadTopicCandidates).toHaveBeenCalledTimes(1))
    act(() => { void result.current.prepare(); rerender({ enabled: false }) })
    await act(async () => { a.resolve(missing); await a.promise })
    expect(b.backend.prepareTopicCandidates).not.toHaveBeenCalled()
    expect(result.current.status).toBe('inactive')
  })
  it('relit le cache préparé sans préparer et isole une réponse tardive après changement de backend', async () => {
    const old = deferred<TopicSnapshot>()
    const first = fakeBackend({ loadTopicCandidates: vi.fn(() => old.promise) })
    const next = fakeBackend({ loadTopicCandidates: vi.fn(async () => prepared) })
    const same = job()
    const { result, rerender } = renderHook(({ backend }) => useTopicCandidates(backend, same, true), { initialProps: { backend: first.backend } })
    await waitFor(() => expect(first.backend.loadTopicCandidates).toHaveBeenCalledTimes(1))
    rerender({ backend: next.backend })
    expect(result.current.snapshot).toBeNull()
    expect(next.backend.loadTopicCandidates).not.toHaveBeenCalled()
    await act(async () => { old.resolve(prepared); await old.promise })
    await waitFor(() => expect(result.current.status).toBe('ready'))
    expect(next.backend.loadTopicCandidates).toHaveBeenCalledTimes(1)
    expect(next.backend.prepareTopicCandidates).not.toHaveBeenCalled()
  })
  it('ne rend jamais prêt un cache dont la partition sans piste est incomplète', async () => {
    const malformed = { ...fixture.loaded_snapshot, candidates: { ...fixture.loaded_snapshot.candidates, without_suggestion: [] } } as TopicSnapshot
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => malformed) })
    const { result } = renderHook(() => useTopicCandidates(b.backend, job(), true))
    await waitFor(() => expect(result.current.status).toBe('stale'))
    expect(result.current.snapshot).toBeNull()
    expect(b.backend.prepareTopicCandidates).not.toHaveBeenCalled()
  })
  it('interdit les pistes sans crypto ou pour un identifiant invalide', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => prepared) })
    const bad = job(); bad.id = 'bad'
    const { result, rerender } = renderHook(({ j }) => useTopicCandidates(b.backend, j, true), { initialProps: { j: bad } })
    expect(result.current.status).toBe('stale')
    expect(b.backend.loadTopicCandidates).not.toHaveBeenCalled()
    vi.stubGlobal('crypto', undefined)
    rerender({ j: job() })
    await waitFor(() => expect(result.current.status).toBe('unavailable'))
    expect(result.current.snapshot).toBeNull()
    vi.unstubAllGlobals()
  })
  it('remonte sous StrictMode sans préparation automatique et rejette une réponse après démontage', async () => {
    const late = deferred<TopicSnapshot>()
    const b = fakeBackend({ loadTopicCandidates: vi.fn(() => late.promise) })
    const { unmount } = renderHook(() => useTopicCandidates(b.backend, job(), true), { wrapper: StrictMode })
    await waitFor(() => expect(b.backend.loadTopicCandidates).toHaveBeenCalledTimes(1))
    unmount()
    await act(async () => { late.resolve(prepared); await late.promise })
    expect(b.backend.prepareTopicCandidates).not.toHaveBeenCalled()
    const again = renderHook(() => useTopicCandidates(b.backend, job(), true))
    await waitFor(() => expect(again.result.current.status).toBe('ready'))
    expect(b.backend.prepareTopicCandidates).not.toHaveBeenCalled()
  })
  it('ne livre pas une fausse réussite sur erreur de préparation et abandonne une réponse au démontage', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => missing), prepareTopicCandidates: vi.fn(async () => { throw new Error('cache corrompu') }) })
    const { result, unmount } = renderHook(() => useTopicCandidates(b.backend, job(), true))
    await waitFor(() => expect(result.current.status).toBe('absent'))
    await act(async () => { await result.current.prepare() })
    expect(result.current.status).toBe('error')
    expect(result.current.snapshot).toBeNull()
    unmount()
  })
})
