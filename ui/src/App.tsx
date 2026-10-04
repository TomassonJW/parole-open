import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { ImportView } from './components/ImportView'
import { ProcessingView } from './components/ProcessingView'
import { ModelsPanel } from './components/ModelsPanel'
import { ResultsView } from './components/ResultsView'
import { StartForm } from './components/StartForm'
import { Notice } from './components/ui'
import { useJobs } from './hooks/useJobs'
import { useModels } from './hooks/useModels'
import { BackendError, UNAVAILABLE_MESSAGE, tauriBackend, type Backend } from './lib/backend'
import { baseName, isSupportedMedia } from './lib/format'
import { needsRecovery, type Job, type StartOptions } from './lib/types'

export type View = 'import' | 'processing' | 'results'

const STEPS: ReadonlyArray<{ view: View; label: string }> = [
  { view: 'import', label: 'Importer' },
  { view: 'processing', label: 'Traitement' },
  { view: 'results', label: 'Transcription' },
]

interface Alert {
  title: string
  message: string
  detail: string | null
}

function hasFinalResult(job: Job | null): job is Job {
  return job?.stage === 'Transcribed'
    && job.translation_issues.length === 0
    && (!job.generate_report || job.report !== null)
    && (!job.target_language || job.segments.every((segment) => segment.translated_text !== null))
}

function hasReadableResults(job: Job | null): job is Job {
  if (job && needsRecovery(job)) return job.segments.length > 0
  return hasFinalResult(job)
}

function toAlert(title: string, error: unknown): Alert {
  if (error instanceof BackendError) {
    return { title, message: error.message, detail: error.detail && error.detail !== error.message ? error.detail : null }
  }
  return { title, message: error instanceof Error ? error.message : String(error), detail: null }
}

export default function App({ backend = tauriBackend }: { backend?: Backend }) {
  const { state, refresh, startJob, resumeJob, saveSpeakerNames, exportJob } = useJobs(backend)
  const models = useModels(backend)
  const [chosenPath, setChosenPath] = useState<string | null>(null)
  const [view, setView] = useState<View>('import')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [settingsRequest, setSettingsRequest] = useState(0)
  const [alert, setAlert] = useState<Alert | null>(null)
  const [dragging, setDragging] = useState(false)
  const firstRender = useRef(true)
  const appRoot = useRef<HTMLDivElement>(null)
  const topbar = useRef<HTMLElement>(null)

  useLayoutEffect(() => {
    const root = appRoot.current
    const header = topbar.current
    if (!root || !header) return
    const measure = () => root.style.setProperty('--reader-top-offset', `${Math.ceil(header.getBoundingClientRect().height)}px`)
    measure()
    const observer = typeof ResizeObserver === 'function' ? new ResizeObserver(measure) : null
    observer?.observe(header)
    window.addEventListener('resize', measure)
    return () => { observer?.disconnect(); window.removeEventListener('resize', measure) }
  }, [])

  const selected = useMemo(
    () => state.jobs.find((j) => j.id === selectedId) ?? null,
    [state.jobs, selectedId],
  )
  const available = backend.available

  // Une étape complémentaire lancée ou encore en cours garde sa jauge visible.
  useEffect(() => {
    if (view === 'results' && selected && !hasReadableResults(selected)) setView('processing')
  }, [view, selected])

  // Place le focus sur le titre de la vue à chaque changement d'étape.
  useEffect(() => {
    if (firstRender.current) {
      firstRender.current = false
      return
    }
    document.getElementById('view-title')?.focus()
  }, [view])

  const openJob = useCallback((job: Job) => {
    setSelectedId(job.id)
    setView(hasFinalResult(job) ? 'results' : 'processing')
  }, [])

  /** Un fichier choisi n'est jamais lancé d'office : l'utilisateur confirme ses options. */
  const importPath = useCallback((path: string) => {
    setAlert(null)
    setChosenPath(path)
    setView('import')
  }, [])

  const launch = useCallback(
    async (options: StartOptions) => {
      setAlert(null)
      setSelectedId(null)
      setView('processing')
      try {
        const job = await startJob(options)
        setChosenPath(null)
        setSelectedId(job.id)
        if (hasFinalResult(job)) setView('results')
      } catch (error) {
        setView('import')
        setAlert(toAlert(`Impossible de lancer la transcription de « ${baseName(options.mediaPath)} ».`, error))
        if (error instanceof BackendError && /mod[eè]le/i.test(error.message)) void models.check()
      }
    },
    [startJob, models.check],
  )

  const pickFile = useCallback(async () => {
    setAlert(null)
    try {
      const path = await backend.pickMediaFile()
      if (path) importPath(path)
    } catch (error) {
      setAlert(toAlert("Impossible d'ouvrir le sélecteur de fichiers.", error))
    }
  }, [backend, importPath])

  const blockedReason = models.preparing ? 'Installation des modèles en cours…' : null

  const resume = useCallback(
    async (job: Job) => {
      setAlert(null)
      try {
        const updated = await resumeJob(job.id)
        if (hasFinalResult(updated)) setView('results')
      } catch (error) {
        setAlert(toAlert('La transcription n’a pas pu reprendre.', error))
      }
    },
    [resumeJob],
  )

  // Glisser-déposer natif (Tauri fournit les chemins réels des fichiers).
  const dropRef = useRef({ importPath, busy: false })
  dropRef.current = { importPath, busy: state.pendingStart !== null }
  useEffect(() => {
    let unlisten: (() => void) | null = null
    let cancelled = false
    backend
      .onFileDrop((event) => {
        if (event.type === 'hover') setDragging(true)
        else if (event.type === 'leave') setDragging(false)
        else {
          setDragging(false)
          if (dropRef.current.busy) {
            setAlert({ title: 'Un import est déjà en cours.', message: 'Attendez la fin de la préparation du fichier précédent.', detail: null })
            return
          }
          const media = event.paths.filter(isSupportedMedia)
          if (media.length === 0) {
            setAlert({
              title: 'Format non pris en charge.',
              message: 'Déposez un fichier audio ou vidéo (MP3, WAV, M4A, FLAC, OGG, MP4, MOV, MKV, WEBM…).',
              detail: null,
            })
            return
          }
          dropRef.current.importPath(media[0])
        }
      })
      .then((fn) => {
        if (cancelled) fn()
        else unlisten = fn
      })
      .catch(() => {})
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [backend])

  const stepEnabled: Record<View, boolean> = {
    import: true,
    processing: selected !== null || state.pendingStart !== null,
    results: hasReadableResults(selected),
  }

  return (
    <div className="app" ref={appRoot}>
      <a className="skip-link" href="#main">
        Aller au contenu
      </a>
      <header className="topbar" ref={topbar}>
        <div className="brand">
          <svg viewBox="0 0 32 32" width="28" height="28" aria-hidden="true" className="brand__mark">
            <rect width="32" height="32" rx="8" fill="currentColor" />
            <path d="M9 12v8M13 9v14M17 12v8M21 14v4M25 12.5v7" stroke="#fff" strokeWidth="2.2" strokeLinecap="round" />
          </svg>
          <span className="brand__name">Parole</span>
          <span className="brand__tag">Transcription locale</span>
        </div>
        <nav aria-label="Étapes" className="steps">
          <ol>
            {STEPS.map((step, index) => (
              <li key={step.view}>
                <button
                  type="button"
                  className={`step${view === step.view ? ' step--current' : ''}`}
                  aria-current={view === step.view ? 'step' : undefined}
                  disabled={!stepEnabled[step.view]}
                  onClick={() => setView(step.view)}
                >
                  <span className="step__index" aria-hidden="true">
                    {index + 1}
                  </span>
                  <span className="step__label">{step.label}</span>
                </button>
              </li>
            ))}
          </ol>
        </nav>
        <button type="button" className="button button--secondary" disabled={!selected || !hasReadableResults(selected)} onClick={() => { setView('results'); setSettingsRequest(n => n + 1) }}>Réglages</button>
      </header>

      <main id="main" className={`main${view === 'results' ? ' main--wide' : ''}`}>
        {!available && (
          <Notice tone="info" title="Moteur local indisponible">
            {UNAVAILABLE_MESSAGE}
          </Notice>
        )}
        {alert && (
          <Notice tone="error" title={alert.title} detail={alert.detail} onDismiss={() => setAlert(null)}>
            {alert.message}
          </Notice>
        )}

        {view === 'import' && (
          <ImportView
            available={available}
            dragging={dragging}
            jobs={state.jobs}
            loaded={state.loaded}
            listError={state.listError?.kind === 'unavailable' ? null : state.listError}
            pendingStart={state.pendingStart}
            models={
              <ModelsPanel
                state={models.state}
                preparing={models.preparing}
                progress={models.progress}
                prepareError={models.prepareError}
                onPrepare={() => void models.prepare()}
                onRecheck={() => void models.check()}
              />
            }
            startForm={
              chosenPath ? (
                <StartForm
                  key={chosenPath}
                  path={chosenPath}
                  busy={state.pendingStart !== null || models.preparing}
                  blockedReason={blockedReason}
                  modelStatus={models.state.kind === 'ready' ? models.state.status : null}
                  reportModels={models.state.kind === 'ready' ? models.state.status.report_models ?? [] : []}
                  installError={models.prepareError?.message ?? null}
                  installProgress={models.progress}
                  onInstall={(id) => void models.install(id)}
                  onStart={(options) => void launch(options)}
                  onCancel={() => setChosenPath(null)}
                />
              ) : null
            }
            onPick={pickFile}
            onOpenJob={openJob}
            onRetryList={() => void refresh()}
          />
        )}
        {view === 'processing' && (
          <ProcessingView
            job={selected}
            history={state.jobs}
            activity={state.activity?.id === selected?.id ? state.activity : null}
            pendingStart={selected ? null : state.pendingStart}
            resuming={selected ? state.resuming.has(selected.id) : false}
            onResume={resume}
            onStop={async (job) => {
              try { await backend.cancelJob(job.id) }
              catch (error) { setAlert(toAlert("Impossible de demander l'arrêt de la transcription.", error)); throw error }
            }}
            onOpenResults={(job) => {
              setSelectedId(job.id)
              if (hasReadableResults(job)) setView('results')
            }}
            onBack={() => setView('import')}
          />
        )}
        {view === 'results' && selected && (
          <ResultsView
            job={selected}
            backend={backend}
            settingsRequest={settingsRequest}
            onSaveNames={(job, names) => saveSpeakerNames(job.id, names)}
            onExport={exportJob}
          />
        )}
        {view === 'results' && !selected && (
          <section className="view" aria-labelledby="view-title">
            <h1 id="view-title" className="view__title" tabIndex={-1}>
              Transcription
            </h1>
            <p className="empty">Cette transcription n'est plus disponible.</p>
          </section>
        )}
      </main>
    </div>
  )
}
