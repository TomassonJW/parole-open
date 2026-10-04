import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import { topicSource } from '../lib/topicSnapshot'
import type { TranscriptNavigation } from '../lib/transcriptNavigation'
import { TopicPanel } from './TopicPanel'
import '../topicPanel.css'
import { formatDuration, speakerIds, storedSpeakerName, stripExtension } from '../lib/format'
import type { Backend } from '../lib/backend'
import type { PlaybackSource } from '../lib/playbackTime'
import { TranscriptExplorer } from './TranscriptExplorer'
import { EXPORT_FORMATS, SOURCE_LANGUAGES, needsRecovery, type ExportFormat, type Job } from '../lib/types'
import { Notice, Spinner, StageBadge } from './ui'
import { ReportDocument } from './ReportDocument'
import { PresentationPanel } from './PresentationPanel'
import { usePresentation } from '../hooks/usePresentation'
import type { PresentationPreferences } from '../lib/transcriptPresentation'

type Status =
  | { kind: 'idle' }
  | { kind: 'busy' }
  | { kind: 'success'; message: string }
  | { kind: 'error'; message: string; detail: string | null }

interface ResultsViewProps {
  job: Job
  onSaveNames: (job: Job, names: Record<string, string>) => Promise<unknown>
  onExport: (job: Job, format: ExportFormat, defaultName: string, presentation?: PresentationPreferences) => Promise<string | null>
  playbackSource?: PlaybackSource | null
  backend?: Backend
  settingsRequest?: number
}

function languageLabel(code: string): string {
  return SOURCE_LANGUAGES.find((l) => l.value === code)?.label.toLowerCase() ?? code
}

function errorStatus(error: unknown): Status {
  const detail = error && typeof error === 'object' && 'detail' in error ? (error as { detail: string | null }).detail : null
  const message = error instanceof Error ? error.message : String(error)
  return { kind: 'error', message, detail: detail && detail !== message ? detail : null }
}

function SpeakersPanel({ job, onSave }: { job: Job; onSave: ResultsViewProps['onSaveNames'] }) {
  const ids = useMemo(() => speakerIds(job), [job])
  const [draft, setDraft] = useState<Record<string, string>>(() => ({ ...job.speaker_names }))
  const [status, setStatus] = useState<Status>({ kind: 'idle' })
  const baseId = useId()

  useEffect(() => {
    setDraft({ ...job.speaker_names })
  }, [job.id, job.speaker_names])

  const dirty = ids.some((id) => storedSpeakerName(draft, id).trim() !== storedSpeakerName(job.speaker_names, id).trim())

  async function submit(event: FormEvent) {
    event.preventDefault()
    const names: Record<string, string> = Object.fromEntries(
      ids.map((id) => [id, storedSpeakerName(draft, id).trim()] as const).filter(([, value]) => value.length > 0),
    )
    setStatus({ kind: 'busy' })
    try {
      await onSave(job, names)
      setStatus({ kind: 'success', message: 'Noms enregistrés.' })
    } catch (error) {
      setStatus(errorStatus(error))
    }
  }

  return (
    <section className="panel" aria-labelledby={`${baseId}-title`}>
      <h2 id={`${baseId}-title`} className="panel__title">
        Locuteurs
      </h2>
      {ids.length === 0 ? (
        <p className="muted small">
          Aucune voix n'a été distinguée dans cet enregistrement. Les passages restent marqués « Locuteur non attribué » :
          aucun nom n'est deviné.
        </p>
      ) : (
        <form className="form" onSubmit={submit}>
          {ids.map((id) => (
            <div className="field" key={id}>
              <label className="field__label" htmlFor={`${baseId}-${id}`}>
                Nom pour « {id} »
              </label>
              <input
                id={`${baseId}-${id}`}
                className="input"
                type="text"
                value={storedSpeakerName(draft, id)}
                placeholder={id}
                autoComplete="off"
                onChange={(e) => {
                  setDraft((d) => ({ ...d, [id]: e.target.value }))
                  if (status.kind !== 'busy') setStatus({ kind: 'idle' })
                }}
              />
            </div>
          ))}
          <button type="submit" className="button button--secondary" disabled={!dirty || status.kind === 'busy'}>
            {status.kind === 'busy' ? (
              <>
                <Spinner /> Enregistrement…
              </>
            ) : (
              'Enregistrer les noms'
            )}
          </button>
          <div aria-live="polite">
            {status.kind === 'success' && <p className="status status--success">{status.message}</p>}
          </div>
          {status.kind === 'error' && (
            <Notice tone="error" title="Les noms n'ont pas été enregistrés." detail={status.detail}>
              {status.message}
            </Notice>
          )}
        </form>
      )}
    </section>
  )
}

function ExportPanel({ job, onExport, presentation, ready }: { job: Job; onExport: ResultsViewProps['onExport']; presentation?: PresentationPreferences; ready: boolean }) {
  const [format, setFormat] = useState<ExportFormat>('txt')
  const [status, setStatus] = useState<Status>({ kind: 'idle' })
  const baseId = useId()
  const incomplete = needsRecovery(job)
  const needsPresentation = (['txt', 'md', 'docx'] as ExportFormat[]).includes(format)
  const unsupported = incomplete && !needsPresentation
  const presentationUnavailable = needsPresentation && !ready

  useEffect(() => {
    if (unsupported) setFormat('txt')
  }, [unsupported])

  async function run() {
    if (unsupported || presentationUnavailable) return
    setStatus({ kind: 'busy' })
    try {
      const written = presentation ? await onExport(job, format, stripExtension(job.media_name), presentation) : await onExport(job, format, stripExtension(job.media_name))
      setStatus(written ? { kind: 'success', message: `Fichier enregistré : ${written}` } : { kind: 'idle' })
    } catch (error) {
      setStatus(errorStatus(error))
    }
  }

  return (
    <section className="panel" aria-labelledby={`${baseId}-title`}>
      <h2 id={`${baseId}-title`} className="panel__title">
        Exporter
      </h2>
      {incomplete && <p className="status">Seuls les documents texte, Markdown et Word portent l'avertissement de résultat incomplet. Les sous-titres et les données JSON seront proposés après la reprise.</p>}
      <fieldset className="choices">
        <legend className="sr-only">Format d'export</legend>
        {EXPORT_FORMATS.map((f) => (
          <label key={f.value} className={`choice${format === f.value ? ' choice--checked' : ''}`}>
            <input
              type="radio"
              name={`${baseId}-format`}
              value={f.value}
              checked={format === f.value}
              disabled={incomplete && !(['txt', 'md', 'docx'] as ExportFormat[]).includes(f.value)}
              onChange={() => {
                setFormat(f.value)
                if (status.kind !== 'busy') setStatus({ kind: 'idle' })
              }}
            />
            <span className="choice__text">
              <span className="choice__label">{f.label}</span>
              <span className="choice__hint">{f.hint}</span>
            </span>
          </label>
        ))}
      </fieldset>
      <p className="muted small">TXT et Markdown : présentation du texte sans couleurs ; Word : couleurs des locuteurs. SRT et VTT : fragments et horaires d’origine ; JSON : archive originale sans mise en forme.</p>
      {presentationUnavailable && <p role="alert">Réglages de présentation indisponibles : ouvrir Présentation pour réessayer avant l’export.</p>}
      <button
        type="button"
        className="button button--primary button--block"
        onClick={run}
        disabled={status.kind === 'busy' || job.segments.length === 0 || unsupported || presentationUnavailable}
      >
        {status.kind === 'busy' ? (
          <>
            <Spinner /> Export en cours…
          </>
        ) : (
          'Exporter…'
        )}
      </button>
      <div aria-live="polite">
        {status.kind === 'success' && <p className="status status--success break">{status.message}</p>}
      </div>
      {status.kind === 'error' && (
        <Notice tone="error" title="L'export a échoué." detail={status.detail}>
          {status.message}
        </Notice>
      )}
    </section>
  )
}

export function ResultsView({ job, onSaveNames, onExport, playbackSource, backend, settingsRequest }: ResultsViewProps) {
  const [view, setView] = useState<'transcript' | 'translation' | 'report'>('transcript')
  const presentation = usePresentation(backend, job)
  const [presentationOpen, setPresentationOpen] = useState(false)
  const presentationButton = useRef<HTMLButtonElement>(null)
  useEffect(() => { if (settingsRequest) setPresentationOpen(true) }, [settingsRequest])
  const exportWithPresentation: ResultsViewProps['onExport'] = (target, format, name) => {
    if (!backend) return onExport(target, format, name)
    if (format === 'srt' || format === 'vtt' || format === 'json') return backend.exportJob(target.id, format, name)
    if (!presentation.state) throw new Error('Réglages de présentation non chargés.')
    return backend.exportJob(target.id, format, name, presentation.draft)
  }
  const [reportStatus, setReportStatus] = useState<Status>({ kind: 'idle' })
  // Keep the player's revision stable from its first render, before loading pistes.
  let source: string | null = null
  try { source = topicSource(job) } catch { /* Invalid source stays closed and cannot navigate. */ }
  const committedSource = useRef<string | null>(null)
  const nextRequest = useRef(0)
  const pisteButton = useRef<HTMLButtonElement>(null)
  const [panel, setPanel] = useState<{ source: string; open: boolean } | null>(null)
  const [navigation, setNavigation] = useState<TranscriptNavigation['request']>(null)
  const [readAttempt, setReadAttempt] = useState(0)
  const panelOpen = !!source && panel?.source === source && panel.open
  useLayoutEffect(() => {
    committedSource.current = source
    setPanel(previous => previous?.source === source ? previous : null)
    setNavigation(previous => previous?.jobId === job.id && previous.revision === source ? previous : null)
  }, [source, job.id])
  function navigate(index: number, action: 'open' | 'listen', requestedSource: string) {
    if (!source || requestedSource !== source || committedSource.current !== source || !Number.isSafeInteger(index) || index < 0 || index >= job.segments.length) return
    nextRequest.current += 1
    setNavigation({ jobId: job.id, revision: source, requestId: nextRequest.current, action, index })
    setPanel({ source, open: false })
    setView('transcript')
  }
  async function refreshReport() {
    setReportStatus({ kind: 'busy' })
    try {
      await onSaveNames(job, job.speaker_names)
      setReportStatus({ kind: 'success', message: 'Compte rendu actualisé à partir des données locales conservées.' })
    } catch (error) {
      setReportStatus(errorStatus(error))
    }
  }
  useEffect(() => { setView('transcript') }, [job.id])
  const speakers = speakerIds(job).length

  return (
    <section className="view view--wide" aria-labelledby="view-title">
      <header className="view__header view__header--row">
        <div>
          <p className="view__eyebrow">Transcription</p>
          <h1 id="view-title" className="view__title view__title--file" tabIndex={-1}>
            {job.media_name}
          </h1>
          <p className="view__lead">
            {job.duration_ms > 0 ? formatDuration(job.duration_ms) : 'Durée inconnue'} · {job.segments.length} passage
            {job.segments.length > 1 ? 's' : ''} ·{' '}
            {speakers === 0 ? 'locuteurs non distingués' : `${speakers} locuteur${speakers > 1 ? 's' : ''}`}
            {job.source_language && ` · langue : ${languageLabel(job.source_language)}`}
          </p>
        </div>
        <StageBadge stage={needsRecovery(job) ? 'Interrupted' : job.stage} />
      </header>

      {needsRecovery(job) && (
        <Notice tone="warning" title={job.translation_issues.length > 0 ? 'Résultat incomplet - traduction à vérifier' : 'Résultat incomplet - traitement interrompu'} detail={job.error ?? (job.translation_issues.length > 0 ? `${job.translation_issues.length} passage(s) demandent une nouvelle vérification.` : null)}>
          Les paroles enregistrées restent consultables et exportables, mais la transcription peut être partielle, la traduction inachevée et le compte rendu absent ou provisoire. Reprends le traitement depuis l'étape Traitement avant toute diffusion.
        </Notice>
      )}
      {job.translation_issues.length > 0 && (
        <Notice tone="warning" title="Traduction à relire">
          {job.translation_issues.length} passage{job.translation_issues.length > 1 ? 's' : ''} peuvent être restés dans la langue source. Le texte est conservé, mais sa qualité doit être vérifiée avant diffusion.
        </Notice>
      )}
      <div className="results-toolbar">
      <nav className="results-tabs" aria-label="Afficher les résultats">
        <button type="button" className={`button ${view === 'transcript' ? 'button--primary' : 'button--secondary'}`} onClick={() => setView('transcript')}>Transcription</button>
        <button type="button" className={`button ${view === 'translation' ? 'button--primary' : 'button--secondary'}`} onClick={() => setView('translation')} disabled={!job.segments.some((s) => s.translated_text !== null)}>Traduction</button>
        <button type="button" className={`button ${view === 'report' ? 'button--primary' : 'button--secondary'}`} onClick={() => setView('report')} disabled={!job.report}>Compte rendu</button>
      </nav>
      <div className="presentation-tools" role="group" aria-label="Outils de transcription">
        <button ref={presentationButton} type="button" className="button button--secondary" aria-expanded={presentationOpen} aria-controls="presentation-panel" onClick={() => setPresentationOpen(open => !open)}>Présentation</button>
        <button ref={pisteButton} type="button" className="button button--secondary" aria-expanded={panelOpen} aria-controls="results-topic-panel" disabled={!source} onClick={() => { if (source && committedSource.current === source) { if (!panelOpen && view === 'report') setView('transcript'); setPanel({ source, open: !panelOpen }) } }}>Pistes de sujets</button>
      </div>
      </div>
      {panelOpen && source && backend && <div id="results-topic-panel"><TopicPanel key={readAttempt} backend={backend} job={job} source={source} canListen={backend.available || !!playbackSource} onRetry={() => { if (source === committedSource.current) setReadAttempt(n => n + 1) }} onNavigate={navigate} onClose={() => { setPanel({ source, open: false }); pisteButton.current?.focus() }} /></div>}
      {panelOpen && !backend && <p id="results-topic-panel" role="status">Moteur local indisponible : pistes non accessibles.</p>}
      <div className="results">
        {view === 'report' ? (
          <section className="panel results__transcript" aria-labelledby="report-title">
            <div className="panel__head">
              <h2 id="report-title" className="panel__title">Compte rendu à vérifier</h2>
              <button type="button" className="button button--secondary" onClick={refreshReport} disabled={reportStatus.kind === 'busy'} title="Recompose le compte rendu enregistré sans relancer la transcription">
                {reportStatus.kind === 'busy' ? 'Actualisation…' : 'Actualiser le compte rendu'}
              </button>
            </div>
            <div aria-live="polite">
              {reportStatus.kind === 'success' && <p className="status status--success">{reportStatus.message}</p>}
            </div>
            {reportStatus.kind === 'error' && <Notice tone="error" title="Actualisation impossible" detail={reportStatus.detail}>{reportStatus.message}</Notice>}
            <ReportDocument content={job.report ?? ''} formatVersion={job.report_format_version ?? 0} />
          </section>
        ) : (
        <section className="panel results__transcript" aria-labelledby="transcript-title">
          <div className="panel__head">
            <h2 id="transcript-title" className="panel__title">
              {view === 'translation' ? 'Traduction' : 'Texte original'}
            </h2>
          </div>
          <div className="presentation-stage">
            {!presentation.snapshot && <div className="presentation-stage__notice" role="status" aria-label="Aperçu de la transcription">
              {presentation.error ? `Aperçu indisponible : ${presentation.error} · affichage détaillé original.` : presentation.previewError ? `Aperçu indisponible : ${presentation.previewError} · affichage détaillé original.` : !backend ? 'Aperçu indisponible : moteur local absent · affichage détaillé original.' : 'Chargement de l’aperçu de la transcription… affichage détaillé original provisoire.'}
              {(presentation.previewError || presentation.error) && <button type="button" className="button button--secondary" onClick={presentation.retry}>Réessayer</button>}
            </div>}
            <TranscriptExplorer job={job} translated={view === 'translation'} playbackSource={playbackSource} backend={backend} navigation={source ? { scope: { jobId: job.id, revision: source }, request: navigation?.jobId === job.id && navigation.revision === source ? navigation : null } : undefined} contextBrowsing={panelOpen} presentation={presentation.snapshot ? { options: presentation.draft.screen, blocks: presentation.snapshot.blocks, speakerColors: presentation.draft.speaker_colors } : undefined} />
          </div>
        </section>
        )}

        <aside className={`results__side${presentationOpen ? ' results__side--presentation' : ''}`} aria-label="Locuteurs et export">
          {presentationOpen && <PresentationPanel job={job} presentation={presentation} onClose={() => { setPresentationOpen(false); presentationButton.current?.focus() }} />}
          <SpeakersPanel job={job} onSave={onSaveNames} />
          <ExportPanel job={job} onExport={exportWithPresentation} presentation={presentation.draft} ready={!backend || !!presentation.state} />
        </aside>
      </div>
    </section>
  )
}
