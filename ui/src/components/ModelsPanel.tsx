import type { ModelsState } from '../hooks/useModels'
import type { BackendError } from '../lib/backend'
import { Notice, Progress, Spinner } from './ui'

interface ModelsPanelProps {
  state: ModelsState
  preparing: boolean
  progress: { received: number; total: number; model?: string } | null
  prepareError: BackendError | null
  onPrepare: () => void
  onRecheck: () => void
}

export function ModelsPanel({ state, preparing, progress, prepareError, onPrepare, onRecheck }: ModelsPanelProps) {
  if (state.kind === 'unavailable') return null

  if (state.kind === 'checking') {
    return (
      <p className="muted small" role="status">
        <Spinner /> Vérification des modèles locaux…
      </p>
    )
  }

  if (state.kind === 'unknown') {
    return (
      <Notice
        tone="warning"
        title="État des modèles inconnu."
        detail={state.error.detail}
        action={
          <button type="button" className="button button--secondary button--sm" onClick={onRecheck}>
            Vérifier à nouveau
          </button>
        }
      >
        {state.error.kind === 'missing-command'
          ? "Cette version du moteur ne sait pas encore indiquer si les modèles sont installés. L'import reste possible : le moteur signalera lui-même un modèle manquant."
          : state.error.message}
      </Notice>
    )
  }

  const { status } = state
  if (status.installed && status.missing.length === 0) {
    return (
      <p className="status status--success small" role="status">
        Modèles installés sur cet ordinateur. Le traitement ne nécessite plus Internet.
      </p>
    )
  }

  return (
    <section className="panel panel--attention" aria-labelledby="models-title">
      <h2 id="models-title" className="panel__title">
        Modèles à préparer
      </h2>
      <p className="small">
        Les modèles de transcription et de langue occupent environ 1,7 Go. Leur téléchargement ne démarre qu'après votre accord ;
        ils restent ensuite disponibles sans Internet.
      </p>
      {status.missing.length > 0 ? (
        <ul className="tag-list" aria-label="Modèles manquants">
          {status.missing.map((m) => (
            <li key={m} className="tag">
              {m}
            </li>
          ))}
        </ul>
      ) : (
        <p className="muted small">Le moteur n'a pas précisé quels modèles manquent.</p>
      )}
      {preparing ? (
        <Progress
          label={progress?.model?.endsWith('.gguf') ? 'Préparation de la langue' : 'Préparation de la transcription'}
          ratio={progress ? progress.received / progress.total : null}
          detail={progress ? `${Math.round(progress.received / 1_000_000)} Mo reçus sur ${Math.round(progress.total / 1_000_000)} Mo` : 'Connexion et vérification du modèle en cours…'}
        />
      ) : (
        <div className="actions">
          <button type="button" className="button button--primary" onClick={onPrepare}>
            Préparer les modèles
          </button>
        </div>
      )}
      {prepareError && (
        <Notice tone="error" title="La préparation des modèles a échoué." detail={prepareError.detail}>
          {prepareError.kind === 'missing-command'
            ? "Cette version du moteur ne sait pas encore télécharger les modèles."
            : prepareError.message}
        </Notice>
      )}
    </section>
  )
}
