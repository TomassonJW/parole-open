import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ResultsView } from '../src/components/ResultsView'
import { fakeBackend, makeJob } from './fakeBackend'

describe('présentation dans les résultats', () => {
  it('affiche l’échec des options futures et propose une reprise distincte du document', async () => {
    const job = makeJob({ stage: 'Transcribed' })
    const { backend } = fakeBackend()
    vi.mocked(backend.savePresentationDefaults).mockRejectedValueOnce(new Error('Défauts indisponibles'))
    render(<ResultsView job={job} backend={backend} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    fireEvent.click(screen.getByRole('button', { name: 'Présentation' }))
    const apply = screen.getByRole('button', { name: /Utiliser ces options pour les futurs documents/ })
    await waitFor(() => expect(apply).toBeEnabled())
    fireEvent.click(apply)
    const retry = await screen.findByRole('button', { name: 'Réessayer les options pour les futurs documents' })
    expect(screen.getByRole('alert')).toHaveTextContent('Défauts indisponibles')
    expect(backend.savePresentationDefaults).toHaveBeenCalledTimes(1)
    expect(backend.savePresentation).not.toHaveBeenCalled()
    fireEvent.click(retry)
    await screen.findByText('Options enregistrées pour les futurs documents, sans couleurs.')
    expect(backend.savePresentationDefaults).toHaveBeenCalledTimes(2)
    expect(backend.savePresentation).not.toHaveBeenCalled()
  })

  it.each([
    ['json', /Données JSON/],
    ['srt', /Sous-titres SRT/],
    ['vtt', /Sous-titres WebVTT/],
  ] as const)('la panne des préférences ne bloque pas l’export original %s', async (format, label) => {
    const job = makeJob({ stage: 'Transcribed', segments: [{ start_ms: 0, end_ms: 100, text: 'Bonjour', speaker_id: null, translated_text: null }] })
    const { backend } = fakeBackend({ loadPresentation: vi.fn(async () => { throw new Error('Préférences inaccessibles') }) })
    render(<ResultsView job={job} backend={backend} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    await waitFor(() => expect(screen.getByRole('status', { name: /aperçu de la transcription/i })).toHaveTextContent('Préférences inaccessibles'))
    expect(screen.getByRole('button', { name: 'Exporter…' })).toBeDisabled()
    fireEvent.click(screen.getByLabelText(label))
    expect(screen.getByRole('button', { name: 'Exporter…' })).toBeEnabled()
    fireEvent.click(screen.getByRole('button', { name: 'Exporter…' }))
    await waitFor(() => expect(backend.exportJob).toHaveBeenCalledExactlyOnceWith(job.id, format, 'reunion'))
  })
  it('ouvre le panneau, dissocie les horaires export et transmet le choix figé à Word', async () => {
    const job = makeJob({ stage: 'Transcribed' })
    const { backend } = fakeBackend()
    render(<ResultsView job={job} backend={backend} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    fireEvent.click(screen.getByRole('button', { name: 'Présentation' }))
    await screen.findByRole('heading', { name: 'Présentation' })
    fireEvent.click(screen.getByLabelText('Horaires à l’écran'))
    fireEvent.click(screen.getByLabelText('Choix distincts pour l’export'))
    fireEvent.click(screen.getByLabelText('Horaires dans l’export'))
    await waitFor(() => expect(backend.savePresentation).toHaveBeenCalled())
  })
  it('envoie le dernier choix explicite à exporter, sans altérer le JSON original', async () => {
    const job = makeJob({ stage: 'Transcribed', segments: [{ start_ms: 0, end_ms: 100, text: 'Bonjour', speaker_id: null, translated_text: null }] })
    const { backend } = fakeBackend()
    render(<ResultsView job={job} backend={backend} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    fireEvent.click(screen.getByRole('button', { name: 'Présentation' }))
    await screen.findByRole('heading', { name: 'Présentation' })
    fireEvent.click(screen.getByLabelText('Transcription seule'))
    fireEvent.click(screen.getByLabelText(/Document Word/))
    fireEvent.click(screen.getByRole('button', { name: 'Exporter…' }))
    await waitFor(() => expect(backend.exportJob).toHaveBeenCalledWith(job.id, 'docx', 'reunion', expect.objectContaining({ export: expect.objectContaining({ content: 'transcript' }) })))
  })
  it('rend visible l’attente et l’échec de projection près du lecteur sans le démonter', async () => {
    const job = makeJob({ stage: 'Transcribed', segments: [{ start_ms: 0, end_ms: 100, text: 'Bonjour', speaker_id: null, translated_text: null }] })
    let rejectPreview!: (error: Error) => void
    const preview = new Promise<never>((_resolve, reject) => { rejectPreview = reject })
    const { backend } = fakeBackend({ previewPresentation: vi.fn(async () => preview) })
    render(<ResultsView job={job} backend={backend} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    expect(await screen.findByRole('status', { name: /aperçu de la transcription/i })).toHaveTextContent(/chargement/i)
    expect(screen.getByRole('heading', { name: 'Texte original' })).toBeInTheDocument()
    rejectPreview(new Error('projection cassée'))
    expect(await screen.findByRole('status', { name: /aperçu de la transcription/i })).toHaveTextContent(/projection cassée/i)
    expect(screen.getByRole('heading', { name: 'Texte original' })).toBeInTheDocument()
  })
  it('réinitialise lecture et export sans effacer les couleurs', async () => {
    const job = makeJob({ stage: 'Transcribed', segments: [{ start_ms: 0, end_ms: 100, text: 'Bonjour', speaker_id: 'A', translated_text: null }] })
    const { backend } = fakeBackend()
    render(<ResultsView job={job} backend={backend} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    fireEvent.click(screen.getByRole('button', { name: 'Présentation' }))
    await waitFor(() => expect(screen.getByLabelText('Horaires à l’écran')).not.toBeDisabled())
    fireEvent.click(screen.getByLabelText('Transcription seule'))
    fireEvent.click(screen.getByLabelText('Choix distincts pour l’export'))
    fireEvent.click(screen.getByRole('button', { name: /Attribuer #2563eb à A/i }))
    fireEvent.click(screen.getByRole('button', { name: /Réinitialiser les options de lecture et d’export/i }))
    expect(screen.getByLabelText('Transcription seule')).not.toBeChecked()
    expect(screen.getByLabelText('Choix distincts pour l’export')).not.toBeChecked()
    await waitFor(() => expect(backend.savePresentation).toHaveBeenCalledWith(job.id, expect.objectContaining({ speaker_colors: { A: '#2563eb' }, export: expect.objectContaining({ content: 'complete', linked: true }) }), expect.any(Number)))
  })
  it('regroupe les contrôles et place le panneau dans la colonne latérale en regard du lecteur', async () => {
    const { backend } = fakeBackend()
    render(<ResultsView job={makeJob({ stage: 'Transcribed' })} backend={backend} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    const controls = screen.getByRole('group', { name: 'Outils de transcription' })
    expect(within(controls).getByRole('button', { name: 'Présentation' })).toBeInTheDocument()
    expect(within(controls).getByRole('button', { name: 'Pistes de sujets' })).toBeInTheDocument()
    fireEvent.click(within(controls).getByRole('button', { name: 'Présentation' }))
    expect(await within(screen.getByRole('complementary', { name: 'Locuteurs et export' })).findByRole('region', { name: 'Présentation' })).toBeInTheDocument()
  })
})
