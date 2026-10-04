import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import App from '../src/App'
import { BackendError } from '../src/lib/backend'
import { fakeBackend, makeJob } from './fakeBackend'

describe('App', () => {
  it('garde reprenable une traduction douteuse malgré un ancien état terminé et un rapport présent', async () => {
    const user = userEvent.setup()
    const suspect = makeJob({ stage: 'Transcribed', completed_chunks: 4, target_language: 'en',
      translation_issues: [0], generate_report: true, report: 'Rapport source conservé',
      segments: [{ start_ms: 0, end_ms: 2000, text: 'Bonjour', translated_text: 'Bonjour', speaker_id: null }],
    })
    const { backend } = fakeBackend({ listJobs: vi.fn(async () => [suspect]) })
    render(<App backend={backend} />)
    await user.click(await screen.findByRole('button', { name: /reunion.mp3, / }))
    expect(screen.getByRole('button', { name: 'Reprendre le traitement' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Voir les résultats conservés' }))
    expect(screen.getByText(/Résultat incomplet/)).toBeInTheDocument()
    await user.click(within(screen.getByRole('navigation', { name: 'Étapes' })).getByRole('button', { name: 'Traitement' }))
    await user.click(screen.getByRole('button', { name: 'Reprendre le traitement' }))
    expect(backend.resumeJob).toHaveBeenCalledWith('job-1')
  })
  it('un modèle facultatif explicitement autorisé ne nécessite pas le modèle de traduction, sauf si une traduction est demandée', async () => {
    const user = userEvent.setup()
    const candidate = { id: 'fixture-optional', name: 'Modèle facultatif de test', available_for_new_jobs: true, file: 'fixture.gguf', bytes: 2497280736, sha256: 'a'.repeat(64), url: 'https://example.invalid/pinned', installed: true }
    const baseline = { ...candidate, id: 'baseline', name: 'Modèle de base', installed: false }
    const { backend } = fakeBackend({ modelStatus: vi.fn(async () => ({ installed: false, core_installed: true, missing: ['Modèle de traduction absent'], report_models: [baseline, candidate] })) })
    render(<App backend={backend} />)
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    await user.click(screen.getByLabelText(/Générer un compte rendu/))
    await user.selectOptions(screen.getByLabelText('Modèle du compte rendu'), candidate.id)
    expect(screen.getByRole('button', { name: 'Lancer la transcription' })).toBeEnabled()
    await user.selectOptions(screen.getByLabelText('Traduire vers'), 'en')
    expect(screen.getByRole('button', { name: 'Lancer la transcription' })).toBeDisabled()
    await user.selectOptions(screen.getByLabelText('Traduire vers'), '')
    await user.click(screen.getByRole('button', { name: 'Lancer la transcription' }))
    expect(backend.startJob).toHaveBeenCalledWith(expect.objectContaining({ reportModelId: candidate.id, targetLanguage: null }))
  })

  it('un modèle de compte rendu optionnel absent bloque seulement le rapport sélectionné et ne télécharge rien sans clic', async () => {
    const user = userEvent.setup()
    const candidate = { id: 'fixture-optional', name: 'Modèle facultatif de test', available_for_new_jobs: true, file: 'fixture.gguf', bytes: 2497280736, sha256: 'a'.repeat(64), url: 'https://example.invalid/pinned', installed: false }
    const baseline = { ...candidate, id: 'baseline', name: 'Modèle de base', installed: true }
    const installReportModel = vi.fn(async () => ({ installed: true, missing: [], report_models: [baseline, { ...candidate, installed: true }] }))
    const { backend } = fakeBackend({ modelStatus: vi.fn(async () => ({ installed: true, missing: [], report_models: [baseline, candidate] })), installReportModel })
    render(<App backend={backend} />)
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    await user.click(screen.getByLabelText(/Générer un compte rendu/))
    await user.selectOptions(screen.getByLabelText('Modèle du compte rendu'), candidate.id)
    expect(screen.getByRole('option', { name: 'Modèle facultatif de test' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Lancer la transcription' })).toBeDisabled()
    expect(installReportModel).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: /Installer ce modèle/ }))
    expect(installReportModel).toHaveBeenCalledExactlyOnceWith(candidate.id)
    expect(screen.getByRole('button', { name: 'Lancer la transcription' })).toBeEnabled()
    await user.click(screen.getByRole('button', { name: 'Lancer la transcription' }))
    expect(backend.startJob).toHaveBeenCalledWith(expect.objectContaining({ reportModelId: candidate.id, generateReport: true }))
  })

  it('signale honnêtement l’absence du moteur local', async () => {
    const { backend } = fakeBackend({ available: false, listJobs: vi.fn(async () => { throw new BackendError('x', 'unavailable') }) })
    render(<App backend={backend} />)
    expect(screen.getByText('Moteur local indisponible')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Choisir un fichier…' })).toBeDisabled()
    expect(backend.modelStatus).not.toHaveBeenCalled()
  })

  it('ne télécharge les modèles que sur action explicite', async () => {
    const user = userEvent.setup()
    const prepareModels = vi.fn(async () => ({ installed: true, missing: [] }))
    const { backend } = fakeBackend({
      modelStatus: vi.fn(async () => ({ installed: false, missing: ['whisper-small', 'pyannote'] })),
      prepareModels,
    })
    render(<App backend={backend} />)
    expect(await screen.findByText('whisper-small')).toBeInTheDocument()
    expect(prepareModels).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    expect(screen.getByRole('button', { name: 'Lancer la transcription' })).toBeDisabled()
    await user.click(screen.getByRole('button', { name: 'Préparer les modèles' }))
    expect(prepareModels).toHaveBeenCalledTimes(1)
    expect(await screen.findByText(/Modèles installés sur cet ordinateur/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Lancer la transcription' })).toBeEnabled()
  })

  it('indique quand model_status n’existe pas encore, sans bloquer', async () => {
    const { backend } = fakeBackend({
      modelStatus: vi.fn(async () => {
        throw new BackendError("absent", 'missing-command', 'Command model_status not found')
      }),
    })
    render(<App backend={backend} />)
    expect(await screen.findByText('État des modèles inconnu.')).toBeInTheDocument()
    expect(screen.getByText(/ne sait pas encore indiquer/)).toBeInTheDocument()
  })

  it('ne propose que les options prises en charge et affiche la progression réelle', async () => {
    const user = userEvent.setup()
    const { backend, push } = fakeBackend()
    render(<App backend={backend} />)
    await waitFor(() => expect(backend.modelStatus).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    expect(screen.getByRole('heading', { name: 'reunion.mp3' })).toBeInTheDocument()
    await user.selectOptions(screen.getByLabelText('Langue parlée'), 'fr')
    await user.selectOptions(screen.getByLabelText('Traduire vers'), 'en')
    await user.click(screen.getByLabelText(/Générer un compte rendu/))
    await user.click(screen.getByRole('button', { name: 'Lancer la transcription' }))
    expect(backend.startJob).toHaveBeenCalledWith({
      mediaPath: '/media/reunion.mp3',
      sourceLanguage: 'fr',
      targetLanguage: 'en',
      reportLanguage: null,
      reportModelId: 'baseline',
      generateReport: true,
    })
    const bar = await screen.findByRole('progressbar')
    expect(bar).toHaveAttribute('aria-valuenow', '0')
    push(makeJob({ stage: 'Transcribing', completed_chunks: 2, target_language: 'en', generate_report: true }))
    await waitFor(() => expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50'))
    expect(screen.getByText(/2 tranches sur 4/)).toBeInTheDocument()
  })

  it('choisit la langue du compte rendu indépendamment de la traduction', async () => {
    const user = userEvent.setup()
    const { backend } = fakeBackend()
    render(<App backend={backend} />)
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    expect(screen.queryByLabelText('Langue du compte rendu')).not.toBeInTheDocument()
    await user.selectOptions(screen.getByLabelText('Langue parlée'), 'fr')
    await user.selectOptions(screen.getByLabelText('Traduire vers'), 'en')
    await user.click(screen.getByLabelText(/Générer un compte rendu/))
    const language = screen.getByLabelText('Langue du compte rendu')
    expect(language).toHaveValue('')
    await user.selectOptions(language, 'fr')
    await user.click(screen.getByRole('button', { name: 'Lancer la transcription' }))
    expect(backend.startJob).toHaveBeenCalledWith({
      mediaPath: '/media/reunion.mp3', sourceLanguage: 'fr', targetLanguage: 'en',
      reportLanguage: 'fr', reportModelId: 'baseline', generateReport: true,
    })
  })

  it('demande la langue parlée quand le rapport choisi diffère de la traduction', async () => {
    const user = userEvent.setup()
    const { backend } = fakeBackend()
    render(<App backend={backend} />)
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    await user.selectOptions(screen.getByLabelText('Traduire vers'), 'en')
    await user.click(screen.getByLabelText(/Générer un compte rendu/))
    await user.selectOptions(screen.getByLabelText('Langue du compte rendu'), 'fr')
    expect(screen.getByText(/préciser la langue parlée/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Lancer la transcription' })).toBeDisabled()
    await user.selectOptions(screen.getByLabelText('Langue parlée'), 'en')
    expect(screen.getByText('La langue du compte rendu doit être la langue des paroles ou de la traduction.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Lancer la transcription' })).toBeDisabled()
    await user.selectOptions(screen.getByLabelText('Langue parlée'), 'fr')
    expect(screen.getByRole('button', { name: 'Lancer la transcription' })).toBeEnabled()
  })

  it('ne transmet pas de langue de compte rendu quand celui-ci est désactivé', async () => {
    const user = userEvent.setup()
    const { backend } = fakeBackend()
    render(<App backend={backend} />)
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    await user.click(screen.getByLabelText(/Générer un compte rendu/))
    await user.selectOptions(screen.getByLabelText('Langue du compte rendu'), 'en')
    await user.click(screen.getByLabelText(/Générer un compte rendu/))
    expect(screen.queryByLabelText('Langue du compte rendu')).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Lancer la transcription' }))
    expect(backend.startJob).toHaveBeenCalledWith(expect.objectContaining({ generateReport: false, reportLanguage: null }))
  })

  it('ne propose pas de relancer un travail dont le moteur a déjà signalé le démarrage', async () => {
    const user = userEvent.setup()
    const { backend, push } = fakeBackend()
    backend.startJob = vi.fn(async () => {
      push(makeJob({ stage: 'Transcribing' }))
      return makeJob({ stage: 'Ready' })
    })
    render(<App backend={backend} />)
    await waitFor(() => expect(backend.onJobUpdated).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    await user.click(screen.getByRole('button', { name: 'Lancer la transcription' }))
    expect(await screen.findByRole('button', { name: 'Arrêter le traitement' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Démarrer la transcription' })).not.toBeInTheDocument()
    expect(backend.resumeJob).not.toHaveBeenCalled()
  })

  it('ne fait pas régresser un compte rendu déjà lancé quand la réponse initiale arrive tard', async () => {
    const user = userEvent.setup()
    const { backend, push } = fakeBackend()
    backend.startJob = vi.fn(async () => {
      push(makeJob({ stage: 'Reporting', completed_chunks: 4, phase_done: 1, phase_total: 3 }))
      return makeJob({ stage: 'Transcribing', completed_chunks: 0 })
    })
    render(<App backend={backend} />)
    await waitFor(() => expect(backend.onJobUpdated).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    await user.click(screen.getByRole('button', { name: 'Lancer la transcription' }))
    expect(await screen.findByText('Compte rendu en cours')).toBeInTheDocument()
    expect(screen.queryByText(/Transcription restante :/)).not.toBeInTheDocument()
  })

  it('affiche un chronomètre et une fourchette sur un travail actif, sans inventer une tranche terminée', async () => {
    const { backend, push, step } = fakeBackend()
    render(<App backend={backend} />)
    await waitFor(() => expect(backend.onJobActivity).toHaveBeenCalled())
    act(() => {
      push(makeJob({ stage: 'Transcribing', timing: { transcription_ms: 0, translation_ms: 0, report_ms: 0, chunk_ms: [], chunk_audio_ms: [], active_since_ms: Date.now() - 5_000 } }))
      step({ id: 'job-1', phase: 'transcription Whisper', chunk: 0, total: 4 })
    })
    await userEvent.setup().click(await screen.findByRole('button', { name: /reunion.mp3, Voix et transcription/ }))
    expect(await screen.findByText(/Temps écoulé :/)).toBeInTheDocument()
    expect(screen.getByText(/Étape en cours : transcription Whisper/)).toBeInTheDocument()
    expect(within(screen.getByRole('progressbar', { name: 'Transcription' })).getByText(/Reste estimé/)).toBeInTheDocument()
    expect(screen.getByText(/0 tranche sur/)).toBeInTheDocument()
  })

  it('annonce et recale le temps restant du compte rendu après les passages mesurés', async () => {
    const { backend, push } = fakeBackend()
    render(<App backend={backend} />)
    await waitFor(() => expect(backend.onJobUpdated).toHaveBeenCalled())
    act(() => push(makeJob({ stage: 'Reporting', completed_chunks: 4, generate_report: true, phase_done: 1, phase_total: 4, timing: { transcription_ms: 40_000, translation_ms: 0, report_ms: 12_000, chunk_ms: [40_000], chunk_audio_ms: [20_000], active_since_ms: Date.now() - 1_000 } })))
    await userEvent.setup().click(await screen.findByRole('button', { name: /reunion.mp3,.*Compte rendu/ }))
    expect(within(screen.getByRole('progressbar', { name: 'Compte rendu' })).getByText(/Reste estimé/)).toBeInTheDocument()
    expect(screen.getByText(/Temps écoulé :/)).toBeInTheDocument()
  })

  it('garde la jauge de transcription visible et montre celle du compte rendu sans faux saut de progression', async () => {
    const { backend, push } = fakeBackend()
    render(<App backend={backend} />)
    await waitFor(() => expect(backend.onJobUpdated).toHaveBeenCalled())
    act(() => push(makeJob({ stage: 'Transcribing', duration_ms: 20_000, chunk_ms: 20_000, completed_chunks: 0, generate_report: true, timing: { transcription_ms: 0, translation_ms: 0, report_ms: 0, chunk_ms: [], chunk_audio_ms: [], active_since_ms: Date.now() - 2_000, active_chunk_since_ms: Date.now() - 2_000 } })))
    await userEvent.setup().click(await screen.findByRole('button', { name: /reunion.mp3, Voix et transcription/ }))
    const transcription = await screen.findByRole('progressbar', { name: 'Transcription' })
    expect(transcription).toHaveAttribute('aria-valuenow', '0')
    expect(within(transcription).getByText(/Reste estimé/)).toBeInTheDocument()
    act(() => push(makeJob({ stage: 'Transcribed', duration_ms: 20_000, chunk_ms: 20_000, completed_chunks: 1, generate_report: true, report: null })))
    expect(screen.getByRole('progressbar', { name: 'Transcription' })).toHaveAttribute('aria-valuenow', '100')
    expect(screen.getByRole('progressbar', { name: 'Compte rendu' })).not.toHaveAttribute('aria-valuenow')
    expect(screen.queryByRole('button', { name: 'Voir la transcription' })).not.toBeInTheDocument()
    act(() => push(makeJob({ stage: 'Reporting', duration_ms: 20_000, chunk_ms: 20_000, completed_chunks: 1, generate_report: true, phase_done: 0, phase_total: 0 })))
    await waitFor(() => expect(screen.getByRole('progressbar', { name: 'Transcription' })).toHaveAttribute('aria-valuenow', '100'))
    const waitingReport = screen.getByRole('progressbar', { name: 'Compte rendu' })
    expect(waitingReport).not.toHaveAttribute('aria-valuenow')
    expect(within(waitingReport).getByText(/Estimation en cours/)).toBeInTheDocument()
    act(() => push(makeJob({ stage: 'Reporting', duration_ms: 20_000, chunk_ms: 20_000, completed_chunks: 1, generate_report: true, phase_done: 1, phase_total: 3, timing: { transcription_ms: 40_000, translation_ms: 0, report_ms: 9_000, chunk_ms: [40_000], chunk_audio_ms: [20_000], active_since_ms: Date.now() - 1_000 } })))
    const report = screen.getByRole('progressbar', { name: 'Compte rendu' })
    await waitFor(() => expect(report).toHaveAttribute('aria-valuenow', '33'))
    expect(within(report).getByText(/Reste estimé/)).toBeInTheDocument()
    expect(screen.getByRole('progressbar', { name: 'Transcription' })).toHaveAttribute('aria-valuenow', '100')
  })

  it('conserve la mini-jauge dans la liste pendant le compte rendu et sa préparation', async () => {
    const { backend, push } = fakeBackend({ listJobs: vi.fn(async () => [makeJob({ stage: 'Transcribed', completed_chunks: 1, duration_ms: 20_000, chunk_ms: 20_000, generate_report: true, report: null })]) })
    render(<App backend={backend} />)
    const row = await screen.findByRole('button', { name: /reunion.mp3,.*Compte rendu/ })
    expect(row.querySelector('.job-row__progress .progress__track')).toBeInTheDocument()
    expect(row.querySelector('.progress__track')).not.toHaveAttribute('aria-valuenow')
    act(() => push(makeJob({ stage: 'Reporting', completed_chunks: 1, duration_ms: 20_000, chunk_ms: 20_000, generate_report: true, phase_done: 1, phase_total: 3 })))
    expect(row.querySelector('.job-row__progress .progress__track')).toHaveAttribute('aria-valuenow', '33')
  })

  it('demande l’arrêt au moteur pour le bon travail', async () => {
    const user = userEvent.setup()
    const { backend } = fakeBackend()
    render(<App backend={backend} />)
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    await user.click(screen.getByRole('button', { name: 'Lancer la transcription' }))
    await user.click(await screen.findByRole('button', { name: 'Arrêter le traitement' }))
    expect(backend.cancelJob).toHaveBeenCalledWith('job-1')
    expect(screen.getByRole('button', { name: 'Arrêt demandé…' })).toBeDisabled()
  })

  it('sans taille de tranche, la barre reste indéterminée (aucun pourcentage inventé)', async () => {
    const user = userEvent.setup()
    const { backend } = fakeBackend({ startJob: vi.fn(async () => makeJob({ stage: 'Transcribing', chunk_ms: 0, completed_chunks: 3 })) })
    render(<App backend={backend} />)
    await waitFor(() => expect(backend.modelStatus).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    await user.click(screen.getByRole('button', { name: 'Lancer la transcription' }))
    const bar = await screen.findByRole('progressbar')
    expect(bar).not.toHaveAttribute('aria-valuenow')
    expect(bar).toHaveAttribute('aria-valuetext', 'Progression non communiquée')
    expect(screen.getByText(/ne communique pas de pourcentage/)).toBeInTheDocument()
  })

  it('affiche l’erreur réelle de start_job et revient à l’import', async () => {
    const user = userEvent.setup()
    const { backend } = fakeBackend({
      startJob: vi.fn(async () => {
        throw new BackendError("ffprobe introuvable", 'failed', 'ffprobe: No such file')
      }),
    })
    render(<App backend={backend} />)
    await waitFor(() => expect(backend.modelStatus).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: 'Choisir un fichier…' }))
    await user.click(screen.getByRole('button', { name: 'Lancer la transcription' }))
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Impossible de lancer la transcription de « reunion.mp3 »')
    expect(alert).toHaveTextContent('ffprobe introuvable')
    expect(screen.getByRole('heading', { level: 1, name: 'Importer un enregistrement' })).toBeInTheDocument()
  })

  it('refuse un fichier déposé non média', async () => {
    const { backend, drop } = fakeBackend()
    render(<App backend={backend} />)
    await waitFor(() => expect(backend.onFileDrop).toHaveBeenCalled())
    drop({ type: 'drop', paths: ['/x/notes.pdf'] })
    expect(await screen.findByRole('alert')).toHaveTextContent('Format non pris en charge.')
    expect(backend.startJob).not.toHaveBeenCalled()
  })

  it('laisse consulter et exporter les paroles conservées après une traduction échouée', async () => {
    const user = userEvent.setup()
    const interrupted = makeJob({
      stage: 'Interrupted', completed_chunks: 4, target_language: 'en',
      report_language: 'fr', generate_report: true, report_format_version: 1,
      report: '# Réunion\n\n## Synthèse\n\nBudget en attente&#46;',
      error: 'Traduction non terminée : réponse du modèle invalide.',
      segments: [{ start_ms: 0, end_ms: 2000, text: 'Bonjour à tous.', speaker_id: null, translated_text: null }],
    })
    const { backend } = fakeBackend({ listJobs: vi.fn(async () => [interrupted]) })
    render(<App backend={backend} />)
    await user.click(await screen.findByRole('button', { name: 'reunion.mp3, Interrompue' }))
    await user.click(screen.getByRole('button', { name: 'Voir les résultats conservés' }))
    expect(screen.getByText(/Résultat incomplet/)).toBeInTheDocument()
    expect(screen.getByText('Bonjour à tous.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Compte rendu' }))
    expect(screen.getByText('Budget en attente.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Exporter…' }))
    expect(backend.exportJob).toHaveBeenCalledWith('job-1', 'txt', 'reunion', expect.objectContaining({ schema_version: 1 }))
    await user.click(within(screen.getByRole('navigation', { name: 'Étapes' })).getByRole('button', { name: 'Traitement' }))
    await user.click(screen.getByRole('button', { name: 'Reprendre le traitement' }))
    expect(backend.resumeJob).toHaveBeenCalledWith('job-1')
  })

  it('n’invente pas de compte rendu dans la langue traduite quand cette traduction manque', async () => {
    const user = userEvent.setup()
    const interrupted = makeJob({
      stage: 'Interrupted', completed_chunks: 4, target_language: 'en', report_language: 'en',
      generate_report: true, report: null, error: 'Traduction non terminée',
      segments: [{ start_ms: 0, end_ms: 2000, text: 'Source conservée.', speaker_id: null, translated_text: null }],
    })
    const { backend } = fakeBackend({ listJobs: vi.fn(async () => [interrupted]) })
    render(<App backend={backend} />)
    await user.click(await screen.findByRole('button', { name: 'reunion.mp3, Interrompue' }))
    await user.click(screen.getByRole('button', { name: 'Voir les résultats conservés' }))
    expect(screen.getByText('Source conservée.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Compte rendu' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Traduction' })).toBeDisabled()
    expect(screen.getByText(/Résultat incomplet/)).toBeInTheDocument()
  })

  it('ne présente pas comme disponible un résultat sans aucune parole conservée', async () => {
    const { backend } = fakeBackend({ listJobs: vi.fn(async () => [makeJob({ stage: 'Interrupted', error: 'Traduction interrompue', segments: [] })]) })
    render(<App backend={backend} />)
    await userEvent.setup().click(await screen.findByRole('button', { name: 'reunion.mp3, Interrompue' }))
    expect(screen.queryByRole('button', { name: 'Voir les résultats conservés' })).not.toBeInTheDocument()
  })

  it('reprend une transcription interrompue', async () => {
    const user = userEvent.setup()
    const interrupted = makeJob({ stage: 'Interrupted', completed_chunks: 1, error: 'processus arrêté' })
    const { backend } = fakeBackend({ listJobs: vi.fn(async () => [interrupted]) })
    render(<App backend={backend} />)
    await user.click(await screen.findByRole('button', { name: 'reunion.mp3, Interrompue' }))
    expect(screen.getByText('Le traitement a été interrompu.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Reprendre le traitement' }))
    expect(backend.resumeJob).toHaveBeenCalledWith('job-1')
  })

  it('renomme les locuteurs, affiche le compte rendu et exporte', async () => {
    const user = userEvent.setup()
    const done = makeJob({
      stage: 'Transcribed',
      completed_chunks: 4,
      source_language: 'fr',
      report: 'Décisions : budget validé.',
      segments: [
        { start_ms: 0, end_ms: 2000, text: 'Bonjour à tous', speaker_id: 'S1', translated_text: 'Hello everyone' },
        { start_ms: 2000, end_ms: 4000, text: 'On commence', speaker_id: null, translated_text: null },
      ],
    })
    const { backend } = fakeBackend({
      listJobs: vi.fn(async () => [done]),
      saveSpeakerNames: vi.fn(async (_id: string, names: Record<string, string>) => ({ ...done, speaker_names: names })),
    })
    render(<App backend={backend} />)
    await user.click(await screen.findByRole('button', { name: 'reunion.mp3, Terminée' }))
    await user.click(screen.getByRole('button', { name: 'Traduction' }))
    expect(screen.getByText('Hello everyone')).toBeInTheDocument()
    await user.click(within(screen.getByRole('navigation', { name: 'Afficher les résultats' })).getByRole('button', { name: 'Transcription' }))
    expect(screen.getByText('Locuteur non attribué')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Compte rendu' }))
    expect(screen.getByText('Décisions : budget validé.')).toBeInTheDocument()
    expect(screen.getByText(/langue : français/)).toBeInTheDocument()

    await user.click(within(screen.getByRole('navigation', { name: 'Afficher les résultats' })).getByRole('button', { name: 'Transcription' }))
    await user.type(screen.getByLabelText('Rechercher dans la transcription'), 'commence')
    const list = screen.getAllByRole('list').find((l) => l.tagName === 'OL' && l.classList.contains('transcript'))!
    expect(within(list).getAllByRole('listitem')).toHaveLength(2)
    expect(within(list).getByText('Bonjour à tous')).toBeInTheDocument()
    expect(await screen.findByText('1 / 1')).toBeInTheDocument()
    await user.click(screen.getByLabelText('N’afficher que les résultats'))
    expect(within(list).getAllByRole('listitem')).toHaveLength(1)
    expect(within(list).queryByText('Bonjour à tous')).not.toBeInTheDocument()
    await user.clear(screen.getByLabelText('Rechercher dans la transcription'))
    expect(within(list).getAllByRole('listitem')).toHaveLength(2)

    await user.type(screen.getByLabelText('Nom pour « S1 »'), 'Alice')
    await user.click(screen.getByRole('button', { name: 'Enregistrer les noms' }))
    expect(backend.saveSpeakerNames).toHaveBeenCalledWith('job-1', { S1: 'Alice' })
    expect(await screen.findByText('Noms enregistrés.')).toBeInTheDocument()
    expect(screen.getByText('Alice', { selector: '.segment__speaker' })).toBeInTheDocument()

    await user.click(screen.getByLabelText(/Sous-titres SRT/))
    await user.click(screen.getByRole('button', { name: 'Exporter…' }))
    expect(backend.exportJob).toHaveBeenCalledWith('job-1', 'srt', 'reunion')
    expect(await screen.findByText('Fichier enregistré : /export/reunion.srt')).toBeInTheDocument()
    await user.click(screen.getByLabelText(/Document Word/))
    await user.click(screen.getByRole('button', { name: 'Exporter…' }))
    expect(backend.exportJob).toHaveBeenCalledWith('job-1', 'docx', 'reunion', expect.objectContaining({ schema_version: 1 }))
  })

  it('une annulation du dialogue natif ne prétend pas avoir exporté', async () => {
    const user = userEvent.setup()
    const done = makeJob({ stage: 'Transcribed', segments: [{ start_ms: 0, end_ms: 1, text: 'Bonjour.', speaker_id: null, translated_text: null }] })
    const { backend } = fakeBackend({
      listJobs: vi.fn(async () => [done]),
      exportJob: vi.fn(async () => null),
    })
    render(<App backend={backend} />)
    await user.click(await screen.findByRole('button', { name: 'reunion.mp3, Terminée' }))
    await user.click(screen.getByRole('button', { name: 'Exporter…' }))
    expect(backend.exportJob).toHaveBeenCalled()
    expect(screen.queryByText(/Fichier enregistré/)).not.toBeInTheDocument()
  })

  it('affiche l’échec d’export sans prétendre au succès', async () => {
    const user = userEvent.setup()
    const done = makeJob({ stage: 'Transcribed', segments: [{ start_ms: 0, end_ms: 1, text: 'a', speaker_id: null, translated_text: null }] })
    const { backend } = fakeBackend({
      listJobs: vi.fn(async () => [done]),
      exportJob: vi.fn(async () => {
        throw new BackendError("La fonction « export_job » n'est pas encore disponible dans le moteur local.", 'missing-command', 'Command export_job not found')
      }),
    })
    render(<App backend={backend} />)
    await user.click(await screen.findByRole('button', { name: 'reunion.mp3, Terminée' }))
    await user.click(screen.getByRole('button', { name: 'Exporter…' }))
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent("L'export a échoué.")
    expect(alert).toHaveTextContent('pas encore disponible')
    expect(screen.queryByText(/Fichier enregistré/)).not.toBeInTheDocument()
  })
})
