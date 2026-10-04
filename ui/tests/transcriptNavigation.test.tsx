import { useState } from 'react'
import { readFileSync } from 'node:fs'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { TranscriptExplorer } from '../src/components/TranscriptExplorer'
import type { TranscriptNavigation } from '../src/lib/transcriptNavigation'
import { fakeBackend, makeJob } from './fakeBackend'

const segments = Array.from({ length: 305 }, (_, i) => ({ text: i === 3 ? 'cible' : `passage ${i}`, translated_text: `traduction ${i}`, speaker_id: null, start_ms: i * 1000, end_ms: i * 1000 + 900 }))
const job = makeJob({ segments })
const scope = { jobId: job.id, revision: 'revision-source-A' }
const fixture = process.env.PAROLE_AUDIO_FIXTURE_OUTPUT
function deferred<T>() { let resolve!: (value: T) => void; return { promise: new Promise<T>(r => { resolve = r }), resolve } }
function Parent({ source = true }: { source?: boolean }) {
  const [navigation, setNavigation] = useState<TranscriptNavigation>({ scope, request: null })
  const [serial, setSerial] = useState(0)
  function send(action: 'open' | 'listen', index = 202) {
    const next = serial + 1
    setSerial(next)
    setNavigation({ scope, request: { ...scope, action, index, requestId: next } })
  }
  return <><button onClick={() => send('open')}>Ouvrir externe</button><button onClick={() => send('listen')}>Écouter externe</button><button onClick={() => send('open', 3)}>Ouvrir proche</button>
    <TranscriptExplorer job={job} translated={false} navigation={navigation} playbackSource={source ? { src: 'blob:fiction', timelineOffsetMs: 0, durationMs: 305000 } : null} /></>
}

describe('navigation externe du lecteur', () => {
  it.each([
    { audioIndex: 202, externalButton: 'Ouvrir proche', externalIndex: 3 },
    { audioIndex: 3, externalButton: 'Ouvrir externe', externalIndex: 202 },
  ])('rend la page au suivi audio $audioIndex après une ouverture externe $externalIndex sous filtre', async ({ audioIndex, externalButton, externalIndex }) => {
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
    const pause = vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
    try {
      const { container } = render(<Parent />)
      const audio = container.querySelector('audio')!
      const at = audioIndex + 0.2
      audio.currentTime = at
      fireEvent.timeUpdate(audio)
      fireEvent.click(screen.getByLabelText('Suivre la lecture'))
      fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'cible' } })
      await screen.findByText('1 / 1')
      fireEvent.click(screen.getByLabelText('N’afficher que les résultats'))
      fireEvent.click(screen.getByText(externalButton))
      expect(container.querySelector(`.segment[data-segment-index="${externalIndex}"]`)).toHaveFocus()
      const returnButton = screen.getByRole('button', { name: 'Revenir à la lecture' })
      returnButton.focus()
      expect(returnButton).toHaveFocus()
      fireEvent.click(returnButton)
      expect(container.querySelector(`.segment[data-segment-index="${audioIndex}"]`)).toHaveClass('segment--playing')
      expect(screen.getByLabelText('N’afficher que les résultats')).not.toBeChecked()
      expect(screen.getByLabelText('Suivre la lecture')).toHaveFocus()
      expect(screen.getByRole('searchbox')).toHaveValue('cible')
      expect(screen.getByText('1 / 1')).toBeInTheDocument()
      expect(container.querySelector('audio')).toBe(audio)
      expect(audio.currentTime).toBe(at)
      fireEvent.timeUpdate(audio)
      expect(container.querySelector(`.segment[data-segment-index="${audioIndex}"]`)).toHaveClass('segment--playing')
      expect(play).not.toHaveBeenCalled()
      expect(pause).not.toHaveBeenCalled()
    } finally { play.mockRestore(); pause.mockRestore() }
  })
  it('réactive le suivi par sa case sans laisser la cible externe reprendre la page', async () => {
    const { container } = render(<Parent />)
    const audio = container.querySelector('audio')!
    audio.currentTime = 202.2
    fireEvent.timeUpdate(audio)
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'cible' } })
    await screen.findByText('1 / 1')
    fireEvent.click(screen.getByLabelText('N’afficher que les résultats'))
    fireEvent.click(screen.getByText('Ouvrir proche'))
    expect(container.querySelector('.segment[data-segment-index="3"]')).toHaveFocus()
    const follow = screen.getByLabelText('Suivre la lecture')
    expect(follow).not.toBeChecked()
    follow.focus()
    fireEvent.click(follow)
    expect(follow).toBeChecked()
    expect(follow).toHaveFocus()
    expect(container.querySelector('.segment[data-segment-index="202"]')).toHaveClass('segment--playing')
    audio.currentTime = 203.2
    fireEvent.timeUpdate(audio)
    expect(container.querySelector('.segment[data-segment-index="203"]')).toHaveClass('segment--playing')
    expect(follow).toHaveFocus()
  })
  it('ouvre deux fois le passage original distant, visible et focalisé même sous filtre, sans déplacer l’audio', async () => {
    const previous = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollIntoView')
    const scroll = vi.fn()
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: scroll })
    try {
      const { container } = render(<Parent />)
      fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'cible' } })
      await screen.findByText('1 / 1')
      fireEvent.click(screen.getByLabelText('N’afficher que les résultats'))
      expect(container.querySelectorAll('.segment')).toHaveLength(1)
      const audio = container.querySelector('audio')!
      fireEvent.click(screen.getByText('Ouvrir externe'))
      const target = container.querySelector('.segment[data-segment-index="202"]') as HTMLElement
      expect(target).toBeInTheDocument()
      expect(target).toHaveFocus()
      expect(container.querySelector('.segment[data-segment-index="3"]')).toBeInTheDocument()
      expect(container.querySelectorAll('.segment').length).toBeLessThanOrEqual(100)
      expect(audio.currentTime).toBe(0)
      expect(screen.getByText('1 / 1')).toBeInTheDocument()
      scroll.mockClear()
      fireEvent.click(screen.getByText('Ouvrir externe'))
      expect(scroll).toHaveBeenCalled()
      expect(target).toHaveFocus()
      await waitFor(() => expect(container.querySelector('.segment[data-segment-index="202"]')).toBeInTheDocument())
      expect(audio.currentTime).toBe(0)
    } finally {
      if (previous) Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', previous)
      else Reflect.deleteProperty(HTMLElement.prototype, 'scrollIntoView')
    }
  })
  it('conserve la cible malgré le résultat tardif de recherche et pagine sur l’index original', async () => {
    const previous = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollIntoView')
    const revealed: string[] = []
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true,
      value: function (this: HTMLElement) { revealed.push(this.closest('.segment')?.getAttribute('data-segment-index') ?? '') } })
    try {
      const { container } = render(<Parent />)
      fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'cible' } })
      fireEvent.click(screen.getByLabelText('N’afficher que les résultats'))
      fireEvent.click(screen.getByText('Ouvrir externe'))
      expect(container.querySelector('.segment[data-segment-index="202"]')).toHaveFocus()
      await screen.findByText('1 / 1')
      expect(container.querySelector('.segment[data-segment-index="202"]')).toHaveFocus()
      expect(container.querySelector('.segment[data-segment-index="3"]')).toBeInTheDocument()
      expect(container.querySelectorAll('.segment')).toHaveLength(2)
      expect(revealed.at(-1)).toBe('202')
    } finally {
      if (previous) Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', previous)
      else Reflect.deleteProperty(HTMLElement.prototype, 'scrollIntoView')
    }
  })
  it('recalcule la page épinglée lorsque de nombreux résultats arrivent après la navigation', async () => {
    const { container } = render(<Parent />)
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'passage' } })
    fireEvent.click(screen.getByLabelText('N’afficher que les résultats'))
    fireEvent.click(screen.getByText('Ouvrir externe'))
    expect(container.querySelector('.segment[data-segment-index="202"]')).toHaveFocus()
    await screen.findByText('1 / 304')
    expect(container.querySelector('.segment[data-segment-index="202"]')).toHaveFocus()
    expect(container.querySelectorAll('.segment').length).toBeLessThanOrEqual(100)
  })
  it('redonne la main à une sélection de résultat explicite après une intention externe', async () => {
    const { container } = render(<Parent />)
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'cible' } })
    await screen.findByText('1 / 1')
    fireEvent.click(screen.getByLabelText('N’afficher que les résultats'))
    fireEvent.click(screen.getByText('Ouvrir externe'))
    expect(container.querySelector('.segment[data-segment-index="202"]')).toHaveFocus()
    fireEvent.click(screen.getByRole('button', { name: /occurrence suivante/i }))
    expect(container.querySelector('.segment[data-segment-index="202"]')).toBeNull()
    expect(container.querySelector('.segment[data-segment-index="3"]')).toBeInTheDocument()
  })
  it('rejette indices invalides et demandes périmées, puis accepte une nouvelle demande après changement de travail et révision', () => {
    function Controls() {
      const [current, setCurrent] = useState(job)
      const [context, setContext] = useState(scope)
      const [request, setRequest] = useState<TranscriptNavigation['request']>(null)
      const [number, setNumber] = useState(0)
      function send(index: number, contextOverride = context) {
        const next = number + 1; setNumber(next)
        setRequest({ ...contextOverride, requestId: next, action: 'open', index })
      }
      return <>
        <button onClick={() => send(NaN)}>NaN</button><button onClick={() => send(1.5)}>fraction</button>
        <button onClick={() => send(305)}>hors plage</button><button onClick={() => send(-1)}>négatif</button>
        <button onClick={() => send(202, { jobId: 'étranger', revision: context.revision })}>hors travail</button>
        <button onClick={() => send(202, { ...context, revision: 'périmée' })}>périmée</button>
        <button onClick={() => send(202)}>valide</button>
        <button onClick={() => setContext({ ...context, revision: 'revision-source-B' })}>réviser</button>
        <button onClick={() => { setCurrent(makeJob({ ...job, id: 'job-2' })); setContext({ jobId: 'job-2', revision: 'revision-source-C' }) }}>remplacer</button>
        <TranscriptExplorer job={current} translated={false} navigation={{ scope: context, request }} />
      </>
    }
    const { container } = render(<Controls />)
    for (const label of ['NaN', 'fraction', 'hors plage', 'négatif', 'hors travail', 'périmée']) {
      fireEvent.click(screen.getByText(label))
      expect(container.querySelector('.segment[data-segment-index="202"]')).toBeNull()
      expect(container.querySelectorAll('audio')).toHaveLength(0)
    }
    fireEvent.click(screen.getByText('valide'))
    expect(container.querySelector('.segment[data-segment-index="202"]')).toHaveFocus()
    fireEvent.click(screen.getByText('réviser'))
    expect(container.querySelector('.segment[data-segment-index="202"]')).toBeNull()
    fireEvent.click(screen.getByText('valide'))
    expect(container.querySelector('.segment[data-segment-index="202"]')).toHaveFocus()
    fireEvent.click(screen.getByText('remplacer'))
    expect(container.querySelector('.segment[data-segment-index="202"]')).toBeNull()
    fireEvent.click(screen.getByText('valide'))
    expect(container.querySelector('.segment[data-segment-index="202"]')).toHaveFocus()
  })
  it('ouvre sans audio disponible sans fabriquer un lecteur', () => {
    const { container } = render(<Parent source={false} />)
    fireEvent.click(screen.getByText('Ouvrir externe'))
    expect(container.querySelector('.segment[data-segment-index="202"]')).toHaveFocus()
    fireEvent.click(screen.getByText('Écouter externe'))
    expect(container.querySelectorAll('audio')).toHaveLength(0)
    expect(screen.getByText('Lecture audio pas encore disponible.')).toBeInTheDocument()
  })
  it('écoute via le lecteur existant et refuse le paquet tardif après changement de révision', async () => {
    if (!fixture) throw new Error('Fixture producteur requise : PAROLE_AUDIO_FIXTURE_OUTPUT')
    const first = deferred<ArrayBuffer>()
    const loadAudioAt = vi.fn(() => first.promise)
    const backend = fakeBackend({ loadAudioAt }).backend
    const original = makeJob({ id: '11111111-1111-4111-8111-111111111111', segments: [segments[0], { ...segments[3], start_ms: 3250, end_ms: 3800 }], duration_ms: 4000, chunk_ms: 1000 })
    const initial = { jobId: original.id, revision: 'A' }
    function Reader() {
      const [context, setContext] = useState(initial)
      const [request, setRequest] = useState<TranscriptNavigation['request']>(null)
      return <><button onClick={() => setRequest({ ...context, index: 1, action: 'listen', requestId: 1 })}>lancer</button>
        <button onClick={() => setContext({ ...context, revision: 'B' })}>réviser</button>
        <button onClick={() => setRequest({ ...context, index: 1, action: 'listen', requestId: 2 })}>relancer</button>
        <TranscriptExplorer job={original} translated={false} backend={backend} navigation={{ scope: context, request }} /></>
    }
    const create = vi.fn(() => 'blob:source')
    const previous = Object.getOwnPropertyDescriptor(URL, 'createObjectURL')
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: create })
    try {
      const { container } = render(<Reader />)
      fireEvent.click(screen.getByText('lancer'))
      expect(loadAudioAt).toHaveBeenCalledWith(original.id, 1250)
      fireEvent.click(screen.getByText('réviser'))
      await act(async () => first.resolve(Uint8Array.from(readFileSync(fixture)).buffer))
      expect(create).not.toHaveBeenCalled()
      expect(container.querySelector('audio')).toBeNull()
    } finally {
      if (previous) Object.defineProperty(URL, 'createObjectURL', previous)
      else Reflect.deleteProperty(URL, 'createObjectURL')
    }
  })
  it('traverse la vraie enveloppe audio via écouter, puis rejette une réponse tardive du travail remplacé', async () => {
    if (!fixture) throw new Error('Fixture producteur requise : PAROLE_AUDIO_FIXTURE_OUTPUT')
    const id = '11111111-1111-4111-8111-111111111111'
    const first = deferred<ArrayBuffer>()
    const backend = fakeBackend({ loadAudioAt: vi.fn().mockResolvedValueOnce(Uint8Array.from(readFileSync(fixture)).buffer).mockImplementationOnce(() => first.promise) }).backend
    const read = backend.loadAudioAt as ReturnType<typeof vi.fn>
    const original = makeJob({ id, segments: [{ ...segments[3], start_ms: 3250, end_ms: 3800 }], duration_ms: 4000, chunk_ms: 1000 })
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
    const pause = vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
    const previousCreate = Object.getOwnPropertyDescriptor(URL, 'createObjectURL')
    const previousRevoke = Object.getOwnPropertyDescriptor(URL, 'revokeObjectURL')
    const create = vi.fn((_blob: Blob) => 'blob:transcript-external')
    const revoke = vi.fn()
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: create })
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revoke })
    function Reader() {
      const [current, setCurrent] = useState(original)
      const [scope, setScope] = useState({ jobId: id, revision: 'A' })
      const [request, setRequest] = useState<TranscriptNavigation['request']>(null)
      return <><button onClick={() => setRequest({ ...scope, action: 'listen', index: 0, requestId: 1 })}>écouter original</button>
        <button onClick={() => setRequest({ ...scope, action: 'listen', index: 0, requestId: 2 })}>réécouter</button>
        <button onClick={() => { setCurrent(makeJob({ ...original, id: 'job-replaced' })); setScope({ jobId: 'job-replaced', revision: 'B' }) }}>autre travail</button>
        <TranscriptExplorer job={current} translated backend={backend} navigation={{ scope, request }} /></>
    }
    try {
      const { container } = render(<Reader />)
      fireEvent.click(screen.getByText('écouter original'))
      expect(read).toHaveBeenCalledWith(id, 1250)
      await waitFor(() => expect(container.querySelector('audio')?.getAttribute('src')).toBe('blob:transcript-external'))
      const element = container.querySelector('audio')!
      expect(create).toHaveBeenCalledTimes(1)
      expect(create.mock.calls[0][0]).toBeInstanceOf(Blob)
      fireEvent.loadedMetadata(element)
      expect(play).toHaveBeenCalledTimes(1)
      expect(element.currentTime).toBe(0.25) // 1250 ms depuis le début de la tranche réelle (offset 1000).
      fireEvent.click(screen.getByText('réécouter'))
      expect(read).toHaveBeenCalledTimes(2)
      fireEvent.click(screen.getByText('autre travail'))
      await act(async () => first.resolve(Uint8Array.from(readFileSync(fixture)).buffer))
      expect(create).toHaveBeenCalledTimes(1)
      expect(revoke).toHaveBeenCalledWith('blob:transcript-external')
      expect(container.querySelector('audio')).toBeNull()
    } finally {
      play.mockRestore(); pause.mockRestore()
      if (previousCreate) Object.defineProperty(URL, 'createObjectURL', previousCreate)
      else Reflect.deleteProperty(URL, 'createObjectURL')
      if (previousRevoke) Object.defineProperty(URL, 'revokeObjectURL', previousRevoke)
      else Reflect.deleteProperty(URL, 'revokeObjectURL')
    }
  })
})
