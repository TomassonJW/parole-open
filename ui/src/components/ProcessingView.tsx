import { useEffect, useState } from 'react'
import { chunkCount, formatClock, formatDuration, processedMs, speakerLabel } from '../lib/format'
import { estimateReport, estimateSpeech } from '../lib/estimate'
import type { JobActivity } from '../lib/backend'
import { needsRecovery, type Job } from '../lib/types'
import type { PendingStartLike } from './types'
import { Notice, Progress, Spinner, StageBadge } from './ui'

interface ProcessingViewProps {
  job: Job | null
  history: Job[]
  activity: JobActivity | null
  pendingStart: PendingStartLike | null
  resuming: boolean
  onResume: (job: Job) => void
  onStop: (job: Job) => Promise<void>
  onOpenResults: (job: Job) => void
  onBack: () => void
}

function SpeechDetail({ job }: { job: Job }) {
  const total = chunkCount(job)
  if (total === 0) {
    if (job.completed_chunks > 0) {
      return (
        <>
          {job.completed_chunks} tranche{job.completed_chunks > 1 ? 's' : ''} traitée{job.completed_chunks > 1 ? 's' : ''}
          {job.duration_ms > 0 && ` · durée totale ${formatClock(job.duration_ms)}`} · le moteur ne communique pas de pourcentage.
        </>
      )
    }
    return <>Le moteur n'a pas encore communiqué d'avancement.</>
  }
  return (
    <>
      {job.completed_chunks} tranche{job.completed_chunks > 1 ? 's' : ''} sur {total} ·{' '}
      {formatClock(processedMs(job))} traitées sur {formatClock(job.duration_ms)}
    </>
  )
}

export function ProcessingView({ job, history, activity, pendingStart, resuming, onResume, onStop, onOpenResults, onBack }: ProcessingViewProps) {
  const [stopping, setStopping] = useState(false)
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!job || !['Transcribing', 'Translating', 'Reporting'].includes(job.stage)) return
    setNow(Date.now())
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [job?.id, job?.stage])
  useEffect(() => { if (!['Transcribing', 'Translating', 'Reporting'].includes(job?.stage ?? '')) setStopping(false) }, [job?.stage])
  const title = job?.media_name ?? pendingStart?.name ?? 'Traitement'

  if (!job && !pendingStart) {
    return (
      <section className="view" aria-labelledby="view-title">
        <header className="view__header">
          <h1 id="view-title" className="view__title" tabIndex={-1}>
            Traitement
          </h1>
        </header>
        <p className="empty">Aucun enregistrement en cours de traitement.</p>
        <button type="button" className="button button--secondary" onClick={onBack}>
          Importer un enregistrement
        </button>
      </section>
    )
  }

  const mustResume = job !== null && needsRecovery(job)
  const awaitingTranslation = job?.stage === 'Transcribed' && job.target_language !== null && job.segments.some((segment) => segment.translated_text === null)
  const awaitingReport = job?.stage === 'Transcribed' && job.generate_report && job.report === null
  const running = resuming || job?.stage === 'Transcribing' || job?.stage === 'Translating' || job?.stage === 'Reporting' || awaitingTranslation || awaitingReport || (!job && pendingStart !== null)
  const knownLength = job !== null && chunkCount(job) > 0
  const platform = /Macintosh|Mac OS X/i.test(navigator.userAgent) ? 'mac' : 'windows'
  const estimate = job && job.stage === 'Transcribing' && knownLength
    ? estimateSpeech(job, history, now, platform)
    : null
  const reportEstimate = job?.stage === 'Reporting'
    ? estimateReport(job, history, now, platform)
    : null
  const transcriptionRatio = job && knownLength ? Math.min(1, job.completed_chunks / chunkCount(job)) : null
  const transcriptionRemaining = job?.stage === 'Transcribing' && estimate
    ? estimate.source === 'calibrage' ? 'Calibrage en cours'
      : estimate.source === 'dépassée' ? 'Estimation dépassée · calcul en cours'
        : `Reste estimé : ${formatDuration(estimate.remainingMs)}`
    : transcriptionRatio === 1 ? 'Transcription terminée' : undefined
  const translatedCount = job?.segments.filter((segment) => segment.translated_text !== null).length ?? 0
  const translationRatio = job?.target_language && job.segments.length
    ? Math.min(1, translatedCount / job.segments.length) : null
  const reportRatio = job?.stage === 'Transcribed' && job.report !== null ? 1
    : job?.stage === 'Reporting' && job.phase_total ? Math.min(0.95, job.phase_done / job.phase_total) : null
  const reportRemaining = job?.stage === 'Reporting'
    ? !reportEstimate || reportEstimate.remainingMs === null ? 'Estimation en cours'
      : reportEstimate.source === 'dépassée' ? 'Estimation dépassée · calcul en cours'
        : `Reste estimé : ${formatDuration(reportEstimate.remainingMs)}`
    : awaitingReport ? 'Estimation en cours'
      : job?.stage === 'Transcribed' && job.report !== null ? 'Compte rendu terminé' : undefined
  const activeElapsed = job?.timing?.active_since_ms && running
    ? Math.max(0, now - job.timing.active_since_ms)
    : 0
  const totalElapsed = (job?.timing?.preparation_ms ?? 0) + (job?.timing?.transcription_ms ?? 0) + (job?.timing?.translation_ms ?? 0) + (job?.timing?.report_ms ?? 0) + activeElapsed
  const latest = job ? job.segments.slice(-3) : []

  return (
    <section className="view" aria-labelledby="view-title" aria-busy={running}>
      <header className="view__header view__header--row">
        <div>
          <p className="view__eyebrow">Traitement</p>
          <h1 id="view-title" className="view__title view__title--file" tabIndex={-1}>
            {title}
          </h1>
          {job && job.duration_ms > 0 && <p className="view__lead">Durée : {formatDuration(job.duration_ms)}</p>}
        </div>
        {job && <StageBadge stage={resuming ? 'Transcribing' : mustResume ? 'Interrupted' : awaitingTranslation ? 'Translating' : awaitingReport ? 'Reporting' : job.stage} />}
      </header>

      <div className="panel panel--focus">
        {!job && pendingStart && (
          <Progress
            label="Préparation de l'enregistrement"
            ratio={null}
            detail="Le moteur analyse le fichier. La progression s'affichera dès qu'il la communique."
          />
        )}

        {job && !knownLength && !running && job.stage !== 'Transcribed' && (
          <div className="progress">
            <p className="progress__label">
              {job.stage === 'Interrupted' ? 'Progression enregistrée' : 'En attente de démarrage'}
            </p>
            <p className="progress__detail"><SpeechDetail job={job} /></p>
          </div>
        )}

        {job && (knownLength || running || job.stage === 'Transcribed') && (
          <Progress
            label="Transcription"
            ratio={transcriptionRatio}
            remaining={transcriptionRemaining}
            detail={<><SpeechDetail job={job} />{estimate && <> · Temps écoulé : {formatDuration(estimate.elapsedMs)}{estimate.source === 'calibrage' ? ' · Calibrage après le premier passage.' : <> · Fourchette indicative : {formatDuration(estimate.lowMs)} à {formatDuration(estimate.highMs)}.</>}</>}</>}
          />
        )}

        {job && job.target_language && (job.stage === 'Translating' || job.stage === 'Reporting' || job.stage === 'Transcribed') && (
          <Progress
            label="Traduction"
            ratio={translationRatio}
            detail={`${translatedCount} passage${translatedCount > 1 ? 's' : ''} traduit${translatedCount > 1 ? 's' : ''} sur ${job.segments.length}`}
          />
        )}

        {job && job.generate_report && (job.stage === 'Reporting' || job.stage === 'Transcribed') && (
          <Progress
            label="Compte rendu"
            ratio={reportRatio}
            remaining={reportRemaining}
            detail={<>{job.stage === 'Reporting' && job.phase_total > 0 ? `${job.phase_done} étape${job.phase_done > 1 ? 's' : ''} terminée${job.phase_done > 1 ? 's' : ''} sur ${job.phase_total}` : job.stage === 'Transcribed' && job.report !== null ? 'Compte rendu enregistré' : 'Préparation des étapes du compte rendu'} · Temps écoulé : {formatDuration(reportEstimate?.elapsedMs ?? job.timing?.report_ms ?? 0)}{reportEstimate && reportEstimate.source === 'mesurée' ? ' · Estimation recalée après les étapes terminées.' : '.'}</>}
          />
        )}

        {job && running && (
          <div className="progress__detail" aria-live="off">
            <p>Temps écoulé total : {formatDuration(totalElapsed)}.</p>
            {job.stage === 'Transcribing' && <p>Étape en cours : {activity && activity.chunk === job.completed_chunks ? activity.phase : 'préparation ou traitement du prochain passage'}.</p>}
            <p>Les jauges avancent à chaque passage ou étape terminé.</p>
          </div>
        )}

        {running && (
          <p className="muted small" aria-live="polite">
            <Spinner /> Le moteur travaille localement. Le résultat est enregistré après chaque passage terminé.
          </p>
        )}

        {running && job && (
          <div className="actions">
            <button type="button" className="button button--secondary" disabled={stopping} onClick={async () => {
              setStopping(true)
              try { await onStop(job) } catch { setStopping(false) }
            }}>
              {stopping ? 'Arrêt demandé…' : 'Arrêter le traitement'}
            </button>
            {stopping && <span className="muted small" role="status">L'étape en cours se termine ; les passages déjà traités sont conservés.</span>}
          </div>
        )}

        {mustResume && !resuming && job && (
          <Notice
            tone="warning"
            title={job.stage === 'Interrupted' ? 'Le traitement a été interrompu.' : 'La traduction reste à vérifier.'}
            detail={job.error ?? (job.translation_issues.length > 0 ? `${job.translation_issues.length} passage(s) traduit(s) demandent une nouvelle vérification.` : null)}
            action={
              <>
                <button type="button" className="button button--primary" onClick={() => onResume(job)}>
                  Reprendre le traitement
                </button>
                {job.segments.length > 0 && (
                  <button type="button" className="button button--secondary" onClick={() => onOpenResults(job)}>
                    Voir les résultats conservés
                  </button>
                )}
              </>
            }
          >
            {job.completed_chunks > 0
              ? `Les ${job.completed_chunks} tranche${job.completed_chunks > 1 ? 's' : ''} déjà traitée${job.completed_chunks > 1 ? 's' : ''} sont conservées : la reprise continue là où elle s'est arrêtée.`
              : 'Aucune tranche n’a encore été enregistrée : la reprise repartira du début.'}
          </Notice>
        )}

        {job?.stage === 'Ready' && !resuming && (
          <div className="actions">
            <button type="button" className="button button--primary" onClick={() => onResume(job)}>
              Démarrer la transcription
            </button>
          </div>
        )}

        {job?.stage === 'Transcribed' && !mustResume && !awaitingTranslation && !awaitingReport && (
          <div className="actions">
            <button type="button" className="button button--primary" onClick={() => onOpenResults(job)}>
              Voir la transcription
            </button>
          </div>
        )}
      </div>

      {latest.length > 0 && job?.stage !== 'Transcribed' && (
        <section className="panel" aria-labelledby="preview-title">
          <h2 id="preview-title" className="panel__title">
            Derniers passages transcrits
          </h2>
          <ol className="transcript transcript--compact">
            {latest.map((segment, index) => (
              <li key={`${segment.start_ms}-${index}`} className="segment">
                <div className="segment__meta">
                  <time className="segment__time">{formatClock(segment.start_ms)}</time>
                  <span className={`segment__speaker${segment.speaker_id ? "" : " segment__speaker--none"}`}>{speakerLabel(job!, segment)}</span>
                </div>
                <p className="segment__text">{segment.text}</p>
              </li>
            ))}
          </ol>
        </section>
      )}
    </section>
  )
}
