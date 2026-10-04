import { readFileSync } from 'node:fs'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TranscriptExplorer } from '../src/components/TranscriptExplorer'
import { fakeBackend, makeJob } from './fakeBackend'

const path = process.env.PAROLE_AUDIO_FIXTURE_OUTPUT
const ID = '11111111-1111-4111-8111-111111111111'
const packet = (suffix = '') => Uint8Array.from(readFileSync(path! + suffix)).buffer
const passages = [{ text: 'premier', start_ms: 100, end_ms: 800, speaker_id: null, translated_text: null }, { text: 'second', start_ms: 3250, end_ms: 3800, speaker_id: null, translated_text: null }]
const job = (id = ID) => makeJob({ id, segments: passages, duration_ms: 4000, chunk_ms: 1000 })
let create: ReturnType<typeof vi.fn>, revoke: ReturnType<typeof vi.fn>
const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
const pause = vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
function deferred<T>() { let resolve!: (value: T) => void; return { promise: new Promise<T>(r => { resolve = r }), resolve } }

beforeEach(() => {
  let number = 0
  create = vi.fn(() => `blob:fic-${++number}`)
  revoke = vi.fn()
  Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: create })
  Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revoke })
  play.mockClear(); pause.mockClear()
})
afterEach(() => { vi.clearAllMocks() })

describe.skipIf(!path)('pont UI avec enveloppes émises par le producteur Rust (transport simulé)', () => {
  it('arrête aussi le dernier extrait à sa borne planifiée, sans suivant', async () => {
    const loadAudioAt = vi.fn(async (_id: string, at: number) => at === 0 ? packet('.overlap-first') : packet('.overlap-last'))
    const { container } = render(<TranscriptExplorer job={makeJob({ id: ID, duration_ms: 1800, chunk_ms: 900, segments: passages.slice(0, 1) })} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    fireEvent.click(screen.getByRole('button', { name: 'Lire' }))
    await waitFor(() => expect(container.querySelector('audio')).not.toBeNull())
    const first = container.querySelector('audio')!
    fireEvent.loadedMetadata(first)
    first.currentTime = 0.9
    fireEvent.timeUpdate(first)
    await waitFor(() => expect(loadAudioAt).toHaveBeenNthCalledWith(2, ID, 900))
    await waitFor(() => expect(container.querySelector('audio')).not.toBe(first))
    const last = container.querySelector('audio')!
    fireEvent.loadedMetadata(last)
    fireEvent.play(last)
    pause.mockClear()
    last.currentTime = 0.9
    fireEvent.timeUpdate(last)
    expect(pause).toHaveBeenCalledTimes(1)
    expect(screen.getByRole('button', { name: 'Lire' })).toBeEnabled()
    expect(loadAudioAt).toHaveBeenCalledTimes(2)
  })
  it('ne confie aucun échantillon excédentaire au lecteur même si les événements arrivent tard', async () => {
    const loadAudioAt = vi.fn(async () => packet('.overlap-first'))
    const { container } = render(<TranscriptExplorer job={makeJob({ id: ID, duration_ms: 1800, chunk_ms: 900, segments: passages.slice(0, 1) })} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    fireEvent.click(screen.getByRole('button', { name: 'Lire' }))
    await waitFor(() => expect(container.querySelector('audio')).not.toBeNull())
    const blob = create.mock.calls[0][0] as Blob
    const raw = await new Promise<ArrayBuffer>((resolve, reject) => {
      const reader = new FileReader(); reader.onload = () => resolve(reader.result as ArrayBuffer); reader.onerror = reject; reader.readAsArrayBuffer(blob)
    })
    const view = new DataView(raw)
    expect(view.getUint32(40, true) / 32).toBe(900)
    expect(view.getUint32(4, true)).toBe(raw.byteLength - 8)
    const original = new Uint8Array(packet('.overlap-first'), 84)
    expect(new Uint8Array(raw).subarray(44)).toEqual(original.subarray(44, 44 + 900 * 32))
    expect(original.length).toBe(44 + 1000 * 32) // paquet/source intact, seule la vue audible est bornée
  })
  it('avance et recule entre tranches depuis une horloge fractionnaire réelle', async () => {
    const loadAudioAt = vi.fn(async (_id: string, at: number) => {
      if (at === 0) return packet('.nav-first')
      if (at === 10125) return packet('.nav-forward')
      if (at === 125) return packet('.nav-back')
      throw new Error('Instant inattendu dans cette fixture')
    })
    const { container } = render(<TranscriptExplorer job={makeJob({ id: ID, duration_ms: 20000, chunk_ms: 1000, segments: passages.slice(0, 1) })} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    fireEvent.click(screen.getByRole('button', { name: 'Lire' }))
    await waitFor(() => expect(container.querySelector('audio')).not.toBeNull())
    const first = container.querySelector('audio')!
    fireEvent.loadedMetadata(first); fireEvent.play(first)
    first.currentTime = 0.12575
    fireEvent.timeUpdate(first)
    fireEvent.click(screen.getByRole('button', { name: 'Avancer de 10 secondes' }))
    await waitFor(() => expect(loadAudioAt).toHaveBeenNthCalledWith(2, ID, 10125))
    await waitFor(() => expect(container.querySelector('audio')).not.toBe(first))
    const second = container.querySelector('audio')!
    fireEvent.loadedMetadata(second); fireEvent.play(second)
    expect(second.currentTime).toBe(0.125)
    second.currentTime = 0.12575
    fireEvent.timeUpdate(second)
    fireEvent.click(screen.getByRole('button', { name: 'Reculer de 10 secondes' }))
    await waitFor(() => expect(loadAudioAt).toHaveBeenNthCalledWith(3, ID, 125))
    await waitFor(() => expect(container.querySelector('audio')).not.toBe(second))
    const third = container.querySelector('audio')!
    fireEvent.loadedMetadata(third)
    expect(third.currentTime).toBe(0.125)
    expect(play).toHaveBeenCalledTimes(3)
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
  it('écoute la tranche du passage demandé et révoque au changement de travail', async () => {
    const loadAudioAt = vi.fn(async (_id: string, at: number) => at === 0 ? packet('.first') : packet())
    const { rerender, unmount, container } = render(<TranscriptExplorer job={job()} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    fireEvent.click(screen.getAllByRole('button', { name: /Écouter ce passage/ })[1])
    await waitFor(() => expect(loadAudioAt).toHaveBeenCalledWith(ID, 1250)) // contexte 2 s dans la deuxième tranche
    await waitFor(() => expect(container.querySelector('audio')?.getAttribute('src')).toBe('blob:fic-1'))
    fireEvent.loadedMetadata(container.querySelector('audio')!)
    expect(play).toHaveBeenCalledTimes(1)
    const old = container.querySelector('audio')!
    rerender(<TranscriptExplorer job={job('22222222-2222-4222-8222-222222222222')} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    await waitFor(() => expect(revoke).toHaveBeenCalledWith('blob:fic-1'))
    expect(container.querySelector('audio')).toBeNull()
    fireEvent.timeUpdate(old)
    unmount()
  })
  it('ignore l’extrait annulé et attend la réponse vérifiée avant reprise', async () => {
    const first = deferred<ArrayBuffer>(), second = deferred<ArrayBuffer>()
    const loadAudioAt = vi.fn().mockImplementationOnce(() => first.promise).mockImplementationOnce(() => second.promise)
    const { container } = render(<TranscriptExplorer job={job()} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    fireEvent.click(screen.getAllByRole('button', { name: /Écouter ce passage/ })[0])
    expect(loadAudioAt).toHaveBeenCalledTimes(1)
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }))
    await act(async () => { first.resolve(packet('.first')) })
    expect(create).not.toHaveBeenCalled()
    expect(play).not.toHaveBeenCalled()
    fireEvent.click(screen.getAllByRole('button', { name: /Écouter ce passage/ })[1])
    await waitFor(() => expect(loadAudioAt).toHaveBeenCalledTimes(2))
    expect(loadAudioAt).toHaveBeenNthCalledWith(2, ID, 1250)
    expect(container.querySelector('audio')).toBeNull()
    expect(create).not.toHaveBeenCalled()
    expect(play).not.toHaveBeenCalled()
    await act(async () => { second.resolve(packet()) })
    await waitFor(() => expect(container.querySelector('audio')?.getAttribute('src')).toBe('blob:fic-1'))
    expect(create).toHaveBeenCalledTimes(1)
    const audio = container.querySelector('audio')!
    fireEvent.loadedMetadata(audio)
    expect(audio.currentTime).toBe(0.25)
    expect(play).toHaveBeenCalledTimes(1)
  })
  it('enchaîne uniquement la borne planifiée, et refuse le WAV suivant absent sans substitution', async () => {
    const loadAudioAt = vi.fn().mockResolvedValueOnce(packet('.first')).mockRejectedValueOnce(new Error('/chemin/secret.wav'))
    const { container } = render(<TranscriptExplorer job={job()} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    fireEvent.click(screen.getAllByRole('button', { name: /Écouter ce passage/ })[0])
    await waitFor(() => expect(container.querySelector('audio')).not.toBeNull())
    const audio = container.querySelector('audio')!
    fireEvent.loadedMetadata(audio)
    audio.currentTime = 1
    fireEvent.timeUpdate(audio)
    await waitFor(() => expect(loadAudioAt).toHaveBeenNthCalledWith(2, ID, 1000))
    expect(await screen.findByRole('alert')).toHaveTextContent('Extrait audio indisponible ou non vérifiable')
    expect(screen.getByRole('alert')).not.toHaveTextContent('/chemin/')
    expect(revoke).toHaveBeenCalledWith('blob:fic-1')
  })
  it('un recul vers une autre tranche pendant la pause charge sans relancer', async () => {
    const loadAudioAt = vi.fn(async (_id: string, at: number) => at === 0 ? packet('.first') : packet())
    const { container } = render(<TranscriptExplorer job={job()} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    fireEvent.click(screen.getAllByRole('button', { name: /Écouter ce passage/ })[1])
    await waitFor(() => expect(container.querySelector('audio')).not.toBeNull())
    fireEvent.loadedMetadata(container.querySelector('audio')!)
    fireEvent.play(container.querySelector('audio')!)
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }))
    fireEvent.click(screen.getByRole('button', { name: 'Reculer de 10 secondes' }))
    await waitFor(() => expect(loadAudioAt).toHaveBeenNthCalledWith(2, ID, 0))
    await waitFor(() => expect(container.querySelector('audio')?.getAttribute('src')).toBe('blob:fic-2'))
    fireEvent.loadedMetadata(container.querySelector('audio')!)
    expect(play).toHaveBeenCalledTimes(1)
  })
  it('écarte la réponse d’un travail quitté pendant le chargement', async () => {
    const first = deferred<ArrayBuffer>()
    const backend = fakeBackend({ loadAudioAt: vi.fn(() => first.promise) }).backend
    const { rerender } = render(<TranscriptExplorer job={job()} translated={false} backend={backend} />)
    fireEvent.click(screen.getAllByRole('button', { name: /Écouter ce passage/ })[0])
    rerender(<TranscriptExplorer job={job('22222222-2222-4222-8222-222222222222')} translated={false} backend={backend} />)
    await act(async () => { first.resolve(packet('.first')) })
    expect(create).not.toHaveBeenCalled()
  })
  it('interrompt sur un vrai trou entre durée WAV et borne demandée, sans fausse continuité', async () => {
    const loadAudioAt = vi.fn(async () => packet('.gap'))
    const { container } = render(<TranscriptExplorer job={job()} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    fireEvent.click(screen.getAllByRole('button', { name: /Écouter ce passage/ })[0])
    await waitFor(() => expect(container.querySelector('audio')).not.toBeNull())
    fireEvent.ended(container.querySelector('audio')!)
    expect(screen.getByRole('alert')).toHaveTextContent('Interruption entre les extraits audio vérifiés')
    expect(loadAudioAt).toHaveBeenCalledTimes(1)
  })
  it('ne crée pas de Blob pour un paquet corrompu', async () => {
    const broken = packet('.first'); new Uint8Array(broken)[0] = 0
    const loadAudioAt = vi.fn(async () => broken)
    render(<TranscriptExplorer job={job()} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    fireEvent.click(screen.getAllByRole('button', { name: /Écouter ce passage/ })[0])
    expect(await screen.findByRole('alert')).toHaveTextContent('Extrait audio indisponible ou non vérifiable')
    expect(create).not.toHaveBeenCalled()
  })
  it('ignore la réponse tardive remplacée par un autre passage et refuse un paquet corrompu', async () => {
    const first = deferred<ArrayBuffer>()
    const loadAudioAt = vi.fn().mockImplementationOnce(() => first.promise).mockImplementationOnce(async () => packet())
    const { container } = render(<TranscriptExplorer job={job()} translated={false} backend={fakeBackend({ loadAudioAt }).backend} />)
    fireEvent.click(screen.getAllByRole('button', { name: /Écouter ce passage/ })[0])
    fireEvent.click(screen.getAllByRole('button', { name: /Écouter ce passage/ })[1])
    expect(loadAudioAt).toHaveBeenCalledTimes(1)
    await act(async () => { first.resolve(packet('.first')) })
    await waitFor(() => expect(loadAudioAt).toHaveBeenCalledTimes(2))
    expect(create).toHaveBeenCalledTimes(1)
    expect(container.querySelector('audio')).not.toBeNull()
  })
})
