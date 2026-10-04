import { readFileSync } from 'node:fs'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import readerFixture from './fixtures/topicReader.generated.json'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import fixture from './fixtures/topicSnapshot.generated.json'
import { ResultsView } from '../src/components/ResultsView'
import { fakeBackend, makeJob } from './fakeBackend'
import type { TopicSnapshot } from '../src/lib/topicSnapshot'

const job = () => makeJob({ id: fixture.job_id, segments: structuredClone(fixture.segments), speaker_names: { 'voix-é': 'Camille' } })
const deferred = <T,>() => { let resolve!: (value: T) => void; const promise = new Promise<T>(r => { resolve = r }); return { promise, resolve } }
const props = { onSaveNames: vi.fn(async () => {}), onExport: vi.fn(async () => null) }
const ready = fixture.loaded_snapshot as TopicSnapshot
const missing = fixture.missing_snapshot as TopicSnapshot
const prepared = fixture.prepared_snapshot as TopicSnapshot
async function open() { await userEvent.setup().click(screen.getByRole('button', { name: 'Pistes de sujets' })) }
async function word() { await userEvent.setup().click(screen.getByRole('button', { name: /mot budget/i })) }

describe('pistes dans les résultats réels', () => {
  it('lit un cache absent sans le préparer, puis prépare seulement au geste et conserve ses paroles', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => missing), prepareTopicCandidates: vi.fn(async () => prepared) }).backend
    const source = job(); const before = structuredClone(source)
    render(<ResultsView {...props} job={source} backend={b} />)
    await open()
    expect(b.loadTopicCandidates).toHaveBeenCalledExactlyOnceWith(source.id)
    await screen.findByText(/Pistes non préparées/)
    expect(b.prepareTopicCandidates).not.toHaveBeenCalled()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Préparer les pistes' }))
    await screen.findByRole('button', { name: /mot budget/i })
    expect(b.prepareTopicCandidates).toHaveBeenCalledExactlyOnceWith(source.id)
    expect(source).toEqual(before)
  })
  it('suspend le suivi visuel dès l’ouverture et le laisse suspendu après fermeture', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => ready) }).backend
    render(<ResultsView {...props} job={job()} backend={b} />)
    fireEvent.click(screen.getByRole('checkbox', { name: 'Suivre la lecture' }))
    expect(screen.queryByRole('button', { name: 'Revenir à la lecture' })).not.toBeInTheDocument()
    await open()
    expect(screen.getByRole('button', { name: 'Revenir à la lecture' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Replier les pistes' }))
    expect(screen.getByRole('button', { name: 'Revenir à la lecture' })).toBeInTheDocument()
  })
  it('garde le suivi suspendu lors du changement de langue avec les pistes ouvertes', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => ready) }).backend
    render(<ResultsView {...props} job={job()} backend={b} />)
    fireEvent.click(screen.getByRole('checkbox', { name: 'Suivre la lecture' }))
    await open()
    await screen.findByRole('button', { name: /mot budget/i })
    expect(screen.getByRole('button', { name: 'Revenir à la lecture' })).toBeInTheDocument()
    for (const name of ['Traduction', 'Transcription']) {
      fireEvent.click(screen.getByRole('button', { name }))
      expect(screen.getByRole('button', { name: 'Pistes de sujets' })).toHaveAttribute('aria-expanded', 'true')
      expect(screen.getByRole('button', { name: 'Revenir à la lecture' })).toBeInTheDocument()
      expect(screen.getByRole('checkbox', { name: 'Suivre la lecture' })).toBeChecked()
    }
    expect(b.loadTopicCandidates).toHaveBeenCalledTimes(1)
    expect(b.prepareTopicCandidates).not.toHaveBeenCalled()
    expect(b.loadAudioAt).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'Replier les pistes' }))
    fireEvent.click(screen.getByRole('button', { name: 'Revenir à la lecture' }))
    expect(screen.queryByRole('button', { name: 'Revenir à la lecture' })).not.toBeInTheDocument()
  })
  it.skipIf(!process.env.PAROLE_AUDIO_FIXTURE_OUTPUT)('garde le paquet Rust et le même audio pendant chargement, préparation et Voir', async () => {
    const payload = Uint8Array.from(readFileSync(process.env.PAROLE_AUDIO_FIXTURE_OUTPUT!)).buffer
    const b = fakeBackend({
      loadTopicCandidates: vi.fn(async () => readerFixture.missing_snapshot as TopicSnapshot),
      prepareTopicCandidates: vi.fn(async () => readerFixture.prepared_snapshot as TopicSnapshot),
      loadAudioAt: vi.fn(async () => payload),
    }).backend
    const previousCreate = Object.getOwnPropertyDescriptor(URL, 'createObjectURL')
    const previousRevoke = Object.getOwnPropertyDescriptor(URL, 'revokeObjectURL')
    const create = vi.fn(() => 'blob:real-packet'); const revoke = vi.fn()
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: create })
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revoke })
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
    const pause = vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
    try {
      const source = makeJob({ id: readerFixture.job_id, segments: readerFixture.segments, duration_ms: 4000 })
      const { container } = render(<ResultsView {...props} job={source} backend={b} />)
      fireEvent.click(within(screen.getByRole('navigation', { name: 'Pages de transcription' })).getByRole('button', { name: 'Page suivante' }))
      fireEvent.click(screen.getByRole('button', { name: 'Écouter ce passage 102' }))
      await waitFor(() => expect(container.querySelector('audio')?.getAttribute('src')).toBe('blob:real-packet'))
      const audio = container.querySelector('audio')!; audio.currentTime = 0.75
      fireEvent.play(audio)
      await open(); await screen.findByText(/Pistes non préparées/)
      expect(container.querySelector('audio')).toBe(audio)
      expect(audio.currentTime).toBe(0.75)
      await userEvent.setup().click(screen.getByRole('button', { name: 'Préparer les pistes' }))
      await userEvent.setup().click(await screen.findByRole('button', { name: /mot second/i }))
      expect(container.querySelector('audio')).toBe(audio)
      await userEvent.setup().click(within(container.querySelector('.topic-explorer__passage[data-segment-index="101"]') as HTMLElement).getByRole('button', { name: 'Voir le passage 102 dans la transcription' }))
      expect(container.querySelector('audio')).toBe(audio)
      expect(audio.currentTime).toBe(0.75)
      expect(create).toHaveBeenCalledTimes(1)
      expect(revoke).not.toHaveBeenCalled()
      expect(pause).not.toHaveBeenCalled()
      expect(play).not.toHaveBeenCalled()
      expect(b.loadAudioAt).toHaveBeenCalledTimes(1)
    } finally {
      cleanup()
      play.mockRestore(); pause.mockRestore()
      if (previousCreate) Object.defineProperty(URL, 'createObjectURL', previousCreate); else Reflect.deleteProperty(URL, 'createObjectURL')
      if (previousRevoke) Object.defineProperty(URL, 'revokeObjectURL', previousRevoke); else Reflect.deleteProperty(URL, 'revokeObjectURL')
    }
  })
  it('ouvre deux fois une preuve source depuis la traduction sous filtre sans chercher ni écouter', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => ready) }).backend
    const { container } = render(<ResultsView {...props} job={job()} backend={b} />)
    fireEvent.click(screen.getByRole('button', { name: 'Traduction' }))
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'aucun' } })
    await open(); await screen.findByRole('button', { name: /mot budget/i }); await word()
    const row = container.querySelector('.topic-explorer__passage[data-segment-index="1"]')!
    await userEvent.setup().click(within(row as HTMLElement).getByRole('button', { name: 'Voir le passage 2 dans la transcription' }))
    expect(screen.getByRole('heading', { name: 'Texte original' })).toBeInTheDocument()
    expect(container.querySelector('.segment[data-segment-index="1"]')).toHaveFocus()
    expect(b.loadAudioAt).not.toHaveBeenCalled()
    await open(); await word()
    await userEvent.setup().click(within(container.querySelector('.topic-explorer__passage[data-segment-index="1"]') as HTMLElement).getByRole('button', { name: 'Voir le passage 2 dans la transcription' }))
    expect(container.querySelector('.segment[data-segment-index="1"]')).toHaveFocus()
  })
  it('révèle la preuve 102 hors page et filtre sans demander de son', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => readerFixture.loaded_snapshot as TopicSnapshot) }).backend
    const source = makeJob({ id: readerFixture.job_id, segments: readerFixture.segments, duration_ms: 4000 })
    const { container } = render(<ResultsView {...props} job={source} backend={b} />)
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'premier' } })
    fireEvent.click(screen.getByRole('checkbox', { name: 'N’afficher que les résultats' }))
    await open()
    await userEvent.setup().click(await screen.findByRole('button', { name: /mot second/i }))
    await userEvent.setup().click(within(container.querySelector('.topic-explorer__passage[data-segment-index="101"]') as HTMLElement).getByRole('button', { name: 'Voir le passage 102 dans la transcription' }))
    expect(container.querySelector('.segment[data-segment-index="101"]')).toHaveFocus()
    expect(screen.getByRole('searchbox')).toHaveValue('premier')
    expect(screen.getByRole('checkbox', { name: 'N’afficher que les résultats' })).toBeChecked()
    expect(screen.queryByRole('navigation', { name: 'Pages de transcription' })).not.toBeInTheDocument()
    expect(b.loadAudioAt).not.toHaveBeenCalled()
  })
  it('saute à la page distante de la source originale, pas au rang du mot', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => readerFixture.loaded_snapshot as TopicSnapshot) }).backend
    const source = makeJob({ id: readerFixture.job_id, segments: readerFixture.segments, duration_ms: 4000 })
    const { container } = render(<ResultsView {...props} job={source} backend={b} />)
    await open()
    await userEvent.setup().click(await screen.findByRole('button', { name: /mot second/i }))
    await userEvent.setup().click(within(container.querySelector('.topic-explorer__passage[data-segment-index="101"]') as HTMLElement).getByRole('button', { name: 'Voir le passage 102 dans la transcription' }))
    expect(container.querySelector('.segment[data-segment-index="101"]')).toHaveFocus()
    expect(screen.getByRole('navigation', { name: 'Pages de transcription' })).toHaveTextContent('101–102 sur 102')
    expect(b.loadAudioAt).not.toHaveBeenCalled()
  })
  it('préserve les voix modifiées, revient du compte rendu à la source et relit le cache sans préparation', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => ready) }).backend
    const first = job(); first.report = 'Rapport provisoire'
    const { rerender } = render(<ResultsView {...props} job={first} backend={b} />)
    fireEvent.click(screen.getByRole('button', { name: 'Compte rendu' }))
    await open()
    expect(screen.getByRole('heading', { name: 'Texte original' })).toBeInTheDocument()
    await screen.findByRole('button', { name: /mot budget/i })
    const renamed = job(); renamed.speaker_names['voix-é'] = 'Zoé'; renamed.report = first.report
    rerender(<ResultsView {...props} job={renamed} backend={b} />)
    expect(screen.getAllByText('Zoé').length).toBeGreaterThan(0)
    expect(b.loadTopicCandidates).toHaveBeenCalledTimes(1)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Replier les pistes' }))
    expect(screen.getByRole('button', { name: 'Pistes de sujets' })).toHaveFocus()
    await open()
    await screen.findByRole('button', { name: /mot budget/i })
    expect(b.loadTopicCandidates).toHaveBeenCalledTimes(2)
    expect(b.prepareTopicCandidates).not.toHaveBeenCalled()
  })
  it('permet une relecture explicite après une erreur, sans préparer', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn().mockRejectedValueOnce(new Error('indisponible')).mockResolvedValue(ready) }).backend
    render(<ResultsView {...props} job={job()} backend={b} />)
    await open()
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Relire les pistes' }))
    await screen.findByRole('button', { name: /mot budget/i })
    expect(b.loadTopicCandidates).toHaveBeenCalledTimes(2)
    expect(b.prepareTopicCandidates).not.toHaveBeenCalled()
  })
  it('neutralise une ancienne action Voir après changement de source', async () => {
    const b = fakeBackend({ loadTopicCandidates: vi.fn().mockResolvedValueOnce(ready).mockResolvedValue(readerFixture.loaded_snapshot) }).backend
    const { container, rerender } = render(<ResultsView {...props} job={job()} backend={b} />)
    await open(); await userEvent.setup().click(await screen.findByRole('button', { name: /mot budget/i }))
    const previous = within(container.querySelector('.topic-explorer__passage[data-segment-index="1"]') as HTMLElement).getByRole('button', { name: 'Voir le passage 2 dans la transcription' })
    const current = makeJob({ id: readerFixture.job_id, segments: readerFixture.segments, duration_ms: 4000 })
    rerender(<ResultsView {...props} job={current} backend={b} />)
    fireEvent.click(previous)
    expect(screen.getByRole('button', { name: 'Pistes de sujets' })).toHaveAttribute('aria-expanded', 'false')
    expect(container.querySelector('.segment[data-segment-index="1"]')).not.toHaveFocus()
    expect(b.loadAudioAt).not.toHaveBeenCalled()
    await open()
    expect(await screen.findByRole('button', { name: /mot second/i })).toBeInTheDocument()
  })
  it('rejette un travail différent et ses anciennes actions sans promouvoir sa réponse tardive', async () => {
    const late = deferred<TopicSnapshot>()
    const b = fakeBackend({ loadTopicCandidates: vi.fn().mockReturnValueOnce(late.promise).mockResolvedValue(readerFixture.loaded_snapshot) }).backend
    const { container, rerender } = render(<ResultsView {...props} job={job()} backend={b} />)
    await open()
    await waitFor(() => expect(b.loadTopicCandidates).toHaveBeenCalledTimes(1))
    const newJob = makeJob({ id: readerFixture.job_id, segments: readerFixture.segments, duration_ms: 4000 })
    rerender(<ResultsView {...props} job={newJob} backend={b} />)
    expect(screen.getByRole('button', { name: 'Pistes de sujets' })).toHaveAttribute('aria-expanded', 'false')
    await act(async () => { late.resolve(ready); await late.promise })
    expect(screen.queryByRole('button', { name: /mot budget/i })).not.toBeInTheDocument()
    await open()
    await userEvent.setup().click(await screen.findByRole('button', { name: /mot second/i }))
    await userEvent.setup().click(within(container.querySelector('.topic-explorer__passage[data-segment-index="101"]') as HTMLElement).getByRole('button', { name: 'Voir le passage 102 dans la transcription' }))
    expect(container.querySelector('.segment[data-segment-index="101"]')).toHaveFocus()
    expect(b.loadTopicCandidates).toHaveBeenNthCalledWith(2, newJob.id)
    expect(b.prepareTopicCandidates).not.toHaveBeenCalled()
  })
  it('refuse les réponses anciennes et relit la source changée sans exposer les pistes précédentes', async () => {
    const late = deferred<TopicSnapshot>()
    const b = fakeBackend({ loadTopicCandidates: vi.fn().mockReturnValueOnce(late.promise).mockResolvedValue(missing) }).backend
    const original = job()
    const { rerender } = render(<ResultsView {...props} job={original} backend={b} />)
    await open()
    await waitFor(() => expect(b.loadTopicCandidates).toHaveBeenCalledTimes(1))
    const changed = job(); changed.segments[0].text += '!'
    rerender(<ResultsView {...props} job={changed} backend={b} />)
    expect(screen.queryByRole('button', { name: /mot budget/i })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Pistes de sujets' })).toHaveAttribute('aria-expanded', 'false')
    await open()
    await waitFor(() => expect(b.loadTopicCandidates).toHaveBeenCalledTimes(2))
    await act(async () => { late.resolve(ready); await late.promise })
    expect(screen.queryByRole('button', { name: /mot budget/i })).not.toBeInTheDocument()
    expect(b.prepareTopicCandidates).not.toHaveBeenCalled()
  })
  it.skipIf(!process.env.PAROLE_AUDIO_FIXTURE_OUTPUT)('écoute seulement au geste par le paquet Rust et le lecteur partagé', async () => {
    const payload = Uint8Array.from(readFileSync(process.env.PAROLE_AUDIO_FIXTURE_OUTPUT!)).buffer
    const loadAudioAt = vi.fn(async () => payload)
    const b = fakeBackend({ loadTopicCandidates: vi.fn(async () => readerFixture.loaded_snapshot as TopicSnapshot), loadAudioAt }).backend
    const previousCreate = Object.getOwnPropertyDescriptor(URL, 'createObjectURL')
    const previousRevoke = Object.getOwnPropertyDescriptor(URL, 'revokeObjectURL')
    const create = vi.fn(() => 'blob:packet-rust'); const revoke = vi.fn()
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: create })
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revoke })
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
    try {
      const source = makeJob({ id: readerFixture.job_id, segments: readerFixture.segments, duration_ms: 4000, chunk_ms: 1000 })
      const { container } = render(<ResultsView {...props} job={source} backend={b} />)
      await open()
      await screen.findByRole('button', { name: /mot second/i })
      expect(loadAudioAt).not.toHaveBeenCalled()
      await userEvent.setup().click(screen.getByRole('button', { name: /mot second/i }))
      const row = container.querySelector('.topic-explorer__passage[data-segment-index="101"]') as HTMLElement
      await userEvent.setup().click(within(row).getByRole('button', { name: 'Écouter le passage 102' }))
      await waitFor(() => expect(loadAudioAt).toHaveBeenCalledExactlyOnceWith(source.id, 1250))
      await waitFor(() => expect(container.querySelector('audio')?.getAttribute('src')).toBe('blob:packet-rust'))
      expect(play).not.toHaveBeenCalled()
      const audio = container.querySelector('audio')!
      fireEvent.loadedMetadata(audio)
      await waitFor(() => expect(play).toHaveBeenCalledTimes(1))
      expect(container.querySelector('.segment[data-segment-index="101"]')).toHaveFocus()
      expect(create).toHaveBeenCalledTimes(1)
    } finally {
      cleanup()
      play.mockRestore()
      if (previousCreate) Object.defineProperty(URL, 'createObjectURL', previousCreate); else Reflect.deleteProperty(URL, 'createObjectURL')
      if (previousRevoke) Object.defineProperty(URL, 'revokeObjectURL', previousRevoke); else Reflect.deleteProperty(URL, 'revokeObjectURL')
    }
  })
  it('affiche honnêtement l’indisponibilité du backend', async () => {
    const b = fakeBackend({ available: false }).backend
    render(<ResultsView {...props} job={job()} backend={b} />)
    await open()
    expect(screen.getByText(/moteur local indisponible/i)).toBeInTheDocument()
    expect(b.loadTopicCandidates).not.toHaveBeenCalled()
    expect(b.prepareTopicCandidates).not.toHaveBeenCalled()
  })
})
