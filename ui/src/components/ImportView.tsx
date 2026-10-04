import type { ReactNode } from 'react'
import { formatDuration, progressRatio, STAGE_LABEL } from '../lib/format'
import type { BackendError, PendingStartLike } from './types'
import { needsRecovery, type Job } from '../lib/types'
import { Notice, Progress, Spinner, StageBadge } from './ui'

interface ImportViewProps {
  available: boolean
  dragging: boolean
  jobs: Job[]
  loaded: boolean
  listError: BackendError | null
  pendingStart: PendingStartLike | null
  /** Panneau d'état des modèles, affiché avant l'import. */
  models: ReactNode
  /** Formulaire d'options quand un fichier est choisi ; remplace la zone de dépôt. */
  startForm: ReactNode | null
  onPick: () => void
  onOpenJob: (job: Job) => void
  onRetryList: () => void
}

export function ImportView({
  available,
  dragging,
  jobs,
  loaded,
  listError,
  pendingStart,
  models,
  startForm,
  onPick,
  onOpenJob,
  onRetryList,
}: ImportViewProps) {
  const busy = pendingStart !== null
  return (
    <section className="view" aria-labelledby="view-title">
      <header className="view__header">
        <h1 id="view-title" className="view__title" tabIndex={-1}>
          Importer un enregistrement
        </h1>
        <p className="view__lead">
          Choisissez un fichier audio ou vidéo. La transcription est réalisée par le moteur installé sur cet ordinateur.
        </p>
      </header>

      {models}

      {startForm ?? (
      <div className={`dropzone${dragging ? ' dropzone--active' : ''}${available ? '' : ' dropzone--disabled'}`}>
        <svg className="dropzone__icon" viewBox="0 0 48 48" width="48" height="48" aria-hidden="true">
          <path
            d="M10 20v8M16 14v20M22 18v12M28 10v28M34 16v16M40 21v6"
            stroke="currentColor"
            strokeWidth="3"
            strokeLinecap="round"
          />
        </svg>
        <p className="dropzone__title">
          {dragging ? 'Relâchez pour importer ce fichier' : 'Glissez un fichier ici'}
        </p>
        <p className="dropzone__hint">MP3, WAV, M4A, FLAC, OGG, MP4, MOV, MKV, WEBM…</p>
        <button type="button" className="button button--primary" onClick={onPick} disabled={!available || busy}>
          {busy ? (
            <>
              <Spinner /> Import en cours…
            </>
          ) : (
            'Choisir un fichier…'
          )}
        </button>
        {busy && pendingStart && (
          <p className="dropzone__hint" role="status">
            « {pendingStart.name} » est transmis au moteur.
          </p>
        )}
      </div>
      )}

      <section className="panel" aria-labelledby="history-title">
        <div className="panel__head">
          <h2 id="history-title" className="panel__title">
            Transcriptions récentes
          </h2>
          {available && loaded && (
            <button type="button" className="button button--ghost button--sm" onClick={onRetryList}>
              Actualiser
            </button>
          )}
        </div>

        {listError && available && (
          <Notice
            tone="error"
            title="Impossible de lire la liste des transcriptions."
            detail={listError.detail}
            action={
              <button type="button" className="button button--secondary button--sm" onClick={onRetryList}>
                Réessayer
              </button>
            }
          >
            {listError.message}
          </Notice>
        )}

        {!loaded && available && (
          <p className="muted" role="status">
            <Spinner /> Lecture des transcriptions…
          </p>
        )}

        {available && loaded && !listError && jobs.length === 0 && (
          <p className="empty">Aucune transcription pour le moment. Importez un premier enregistrement pour commencer.</p>
        )}

        {!available && (
          <p className="empty">L'historique s'affichera lorsque le moteur local sera joignable.</p>
        )}

        {jobs.length > 0 && (
          <ul className="job-list">
            {jobs.map((job) => {
              const awaitingTranslation = job.stage === 'Transcribed' && job.target_language !== null && job.segments.some((segment) => segment.translated_text === null)
              const awaitingReport = job.stage === 'Transcribed' && job.generate_report && job.report === null
              const displayStage = needsRecovery(job) ? 'Interrupted' : awaitingTranslation ? 'Translating' : awaitingReport ? 'Reporting' : job.stage
              const ratio = (awaitingTranslation || awaitingReport)
                || (displayStage === 'Reporting' && job.phase_total === 0)
                || (displayStage === 'Translating' && job.segments.length === 0)
                ? null : progressRatio(job)
              const inProgress = ['Transcribing', 'Translating', 'Reporting', 'Interrupted'].includes(displayStage)
              return (
                <li key={job.id}>
                  <button
                    type="button"
                    className="job-row"
                    onClick={() => onOpenJob(job)}
                    aria-label={`${job.media_name}, ${STAGE_LABEL[displayStage]}`}
                  >
                    <span className="job-row__main">
                      <span className="job-row__name">{job.media_name}</span>
                      <span className="job-row__meta">
                        {job.duration_ms > 0 ? formatDuration(job.duration_ms) : 'Durée inconnue'}
                        {job.stage === 'Transcribed' && ` · ${job.segments.length} passage${job.segments.length > 1 ? 's' : ''}`}
                      </span>
                    </span>
                    {inProgress && (job.chunk_ms > 0 || displayStage === 'Reporting') && (
                      <span className="job-row__progress" aria-hidden="true">
                        <Progress label="" ratio={ratio} size="sm" />
                      </span>
                    )}
                    <StageBadge stage={displayStage} />
                    <svg className="job-row__chevron" viewBox="0 0 20 20" width="18" height="18" aria-hidden="true">
                      <path d="M8 5l5 5-5 5" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
                    </svg>
                  </button>
                </li>
              )
            })}
          </ul>
        )}
      </section>
    </section>
  )
}
