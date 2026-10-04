import { fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { TranscriptExplorer, type TranscriptPresentation } from '../src/components/TranscriptExplorer'
import { makeJob } from './fakeBackend'

const segment = (text: string, index: number, speaker_id: string | null = 'A', translated_text: string | null = null) =>
  ({ text, start_ms: index * 1000, end_ms: index * 1000 + 900, speaker_id, translated_text })
const presented = (blocks: TranscriptPresentation['blocks'], options: Partial<TranscriptPresentation['options']> = {}): TranscriptPresentation => ({
  options: { mode: 'fluid', pause_ms: 2000, show_timestamps: true, ...options }, blocks, speakerColors: { A: '#ff0000', B: '#00ff00' },
})
const block = (indices: number[], speaker_id: string | null = 'A') => ({ segment_indices: indices, start_ms: indices[0] * 1000, end_ms: indices.at(-1)! * 1000 + 900, speaker_id })
const anchors = () => [...document.querySelectorAll('.segment')].map(node => Number(node.getAttribute('data-segment-index')))

describe('projection de blocs du moteur', () => {
  it('affiche le locuteur et l’horaire une fois par bloc sans confondre A-B-A ni perdre les ancres', () => {
    const job = makeJob({ segments: [segment('premier', 0), segment('second', 1), segment('milieu', 2, 'B'), segment('retour', 3)] })
    const { container } = render(<TranscriptExplorer job={job} translated={false} presentation={presented([block([0, 1]), block([2], 'B'), block([3])])} />)
    expect(anchors()).toEqual([0, 1, 2, 3])
    expect(container.querySelectorAll('.transcript-block')).toHaveLength(3)
    expect(container.querySelectorAll('.segment__speaker')).toHaveLength(3)
    expect(container.querySelectorAll('.segment__time')).toHaveLength(3)
    expect(container.querySelectorAll('.transcript-block__swatch')).toHaveLength(3)
    expect(container.querySelector('.transcript-block__passages')?.tagName).toBe('P')
    expect(container.querySelector('.transcript-block .segment')?.tagName).toBe('SPAN')
    expect(container.querySelector('.transcript-block__listen-passage')).toHaveTextContent('')
    expect(screen.getByText('premier')).toBeInTheDocument()
    expect(screen.getByText('retour')).toBeInTheDocument()
  })
  it('filtre des blocs entiers et révèle précisément une occurrence au deuxième segment', async () => {
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
    try {
      const job = makeJob({ segments: [{ ...segment('contexte', 0), start_ms: 5000, end_ms: 5900 }, { ...segment('mot recherché', 1), start_ms: 7000, end_ms: 7900 }, { ...segment('ailleurs', 2, 'B'), start_ms: 9000, end_ms: 9900 }] })
      const projection = presented([{ segment_indices: [0, 1], start_ms: 5000, end_ms: 7900, speaker_id: 'A' }, { segment_indices: [2], start_ms: 9000, end_ms: 9900, speaker_id: 'B' }])
      const { container } = render(<TranscriptExplorer job={job} translated={false} presentation={projection} playbackSource={{ src: 'blob:fiction', timelineOffsetMs: 0, durationMs: 12000 }} />)
      const user = userEvent.setup()
      await user.type(screen.getByRole('searchbox'), 'recherché')
      expect(await screen.findByText('1 / 1')).toBeInTheDocument()
      await user.click(screen.getByLabelText('Blocs contenant un résultat'))
      expect(anchors()).toEqual([0, 1])
      expect(container.querySelector('.transcript-mark--current')?.closest('.segment')).toHaveAttribute('data-segment-index', '1')
      const audio = container.querySelector('audio')!
      await user.click(within(container.querySelector('.transcript-block') as HTMLElement).getByRole('button', { name: /écouter le bloc/i }))
      fireEvent.loadedMetadata(audio)
      expect(audio.currentTime).toBe(3)
      await user.click(within(container.querySelector('.segment[data-segment-index="1"]') as HTMLElement).getByRole('button', { name: /écouter ce passage/i }))
      fireEvent.loadedMetadata(audio)
      expect(audio.currentTime).toBe(5)
      expect(play).toHaveBeenCalledTimes(2)
    } finally { play.mockRestore() }
  })
  it('borne un bloc long à 100 segments par page et indique sa suite sans oublier de ligne', async () => {
    const segments = Array.from({ length: 205 }, (_, i) => segment(`ligne ${i}`, i))
    const { container } = render(<TranscriptExplorer job={makeJob({ segments })} translated={false} presentation={presented([block(segments.map((_, i) => i))])} />)
    const user = userEvent.setup()
    const seen: number[] = []
    for (let page = 0; page < 3; page++) {
      seen.push(...anchors())
      expect(container.querySelectorAll('.transcript-block')).toHaveLength(1)
      if (page) expect(screen.getByText('Suite du bloc')).toBeInTheDocument()
      if (page < 2) await user.click(within(screen.getByRole('navigation', { name: 'Pages de transcription' })).getByRole('button', { name: 'Page suivante' }))
    }
    expect(seen).toEqual(segments.map((_, i) => i))
  })
  it('masque les horaires sans masquer le nom, garde la traduction partielle et les segments actifs chevauchants', () => {
    const segments = [segment('source A', 0, 'A', 'traduit A'), { ...segment('source B', 1, 'A'), start_ms: 500, end_ms: 1600 }]
    const { container, rerender } = render(<TranscriptExplorer job={makeJob({ segments })} translated={true} presentation={presented([block([0, 1])], { show_timestamps: false })} playbackSource={{ src: 'blob:fiction', timelineOffsetMs: 0, durationMs: 2000 }} />)
    expect(container.querySelectorAll('.segment__time')).toHaveLength(0)
    expect(container.querySelectorAll('.segment__speaker')).toHaveLength(1)
    expect(screen.getByText('traduit A')).toBeInTheDocument()
    expect(screen.getByText('[traduction manquante]')).toBeInTheDocument()
    const audio = container.querySelector('audio')!
    fireEvent.timeUpdate(audio, { target: { currentTime: 0.7 } })
    expect(container.querySelectorAll('.segment--playing')).toHaveLength(2)
    const pause = vi.spyOn(audio, 'pause')
    rerender(<TranscriptExplorer job={makeJob({ segments })} translated={true} presentation={presented([block([0]), block([1])], { mode: 'detailed' })} playbackSource={{ src: 'blob:fiction', timelineOffsetMs: 0, durationMs: 2000 }} />)
    expect(pause).not.toHaveBeenCalled()
    expect(container.querySelector('audio')).toBe(audio)
    expect(anchors()).toEqual([0, 1])
  })
  it('navigue vers un indice source au-delà de la page 100 après filtrage par bloc', async () => {
    const segments = Array.from({ length: 205 }, (_, i) => segment(i === 201 ? 'aiguille' : `ligne ${i}`, i))
    const job = makeJob({ segments })
    const projection = presented([block(Array.from({ length: 100 }, (_, i) => i)), block(Array.from({ length: 105 }, (_, i) => i + 100))])
    const navigation = { scope: { jobId: job.id, revision: 'r1' }, request: null as null | { jobId: string; revision: string; requestId: number; action: 'open' | 'listen'; index: number } }
    const { container, rerender } = render(<TranscriptExplorer job={job} translated={false} presentation={projection} navigation={navigation} />)
    const user = userEvent.setup()
    await user.type(screen.getByRole('searchbox'), 'aiguille')
    expect(await screen.findByText('1 / 1')).toBeInTheDocument()
    await user.click(screen.getByLabelText('Blocs contenant un résultat'))
    expect(anchors()).toEqual([200, 201, 202, 203, 204])
    navigation.request = { jobId: job.id, revision: 'r1', requestId: 1, action: 'open', index: 103 }
    rerender(<TranscriptExplorer job={job} translated={false} presentation={projection} navigation={navigation} />)
    expect(container.querySelector('.segment[data-segment-index="103"]')).toHaveFocus()
    expect(anchors()).toEqual(Array.from({ length: 100 }, (_, i) => i + 100))
  })
  it('préserve les résultats traversant deux segments et les repères précis', async () => {
    const job = makeJob({ segments: [segment('un début avec mot', 0), segment('suivant ici', 1), segment('fin', 2)] })
    const { container } = render(<TranscriptExplorer job={job} translated={false} presentation={presented([block([0, 1]), block([2])])} />)
    const user = userEvent.setup()
    await user.type(screen.getByRole('searchbox'), 'mot suivant')
    expect(await screen.findByText('1 / 1')).toBeInTheDocument()
    expect(container.querySelectorAll('mark')).toHaveLength(2)
    expect(anchors()).toEqual([0, 1, 2])
  })
})
