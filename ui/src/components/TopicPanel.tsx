import { useTopicCandidates } from '../hooks/useTopicCandidates'
import type { Backend } from '../lib/backend'
import type { Job } from '../lib/types'
import { TopicExplorer } from './TopicExplorer'

export function TopicPanel({ backend, job, source, onNavigate, onClose, onRetry, canListen }: {
  backend: Backend
  job: Job
  source: string
  onNavigate: (index: number, action: 'open' | 'listen', source: string) => void
  onClose: () => void
  onRetry: () => void
  canListen: boolean
}) {
  const { status, snapshot, error, prepare } = useTopicCandidates(backend, job, true)
  const ready = status === 'ready' && snapshot?.candidates != null
  return <aside className="topic-panel" aria-label="Pistes sauvegardées">
    <div className="topic-panel__bar"><strong>Pistes sauvegardées</strong><button type="button" className="button button--secondary" onClick={onClose}>Replier les pistes</button></div>
    {status === 'loading' && <p role="status">Lecture des pistes sauvegardées…</p>}
    {status === 'preparing' && <p role="status">Préparation des pistes…</p>}
    {status === 'absent' && <div><p>Pistes non préparées pour ce travail. Aucune recherche automatique.</p><button type="button" className="button button--secondary" onClick={() => { void prepare() }}>Préparer les pistes</button></div>}
    {status === 'unavailable' && <p role="status">Moteur local indisponible : pistes non accessibles.</p>}
    {(status === 'stale' || status === 'error') && <div role="alert"><p>{error ?? 'Pistes sauvegardées non vérifiables.'}</p><button type="button" className="button button--secondary" onClick={onRetry}>Relire les pistes</button></div>}
    {ready && <TopicExplorer workScope={job.id} candidateScope={snapshot.job_id} sourceRevision={snapshot.source_revision} candidateRevision={snapshot.source_revision} segments={job.segments} speakerNames={job.speaker_names} candidates={snapshot.candidates} onOpen={index => onNavigate(index, 'open', source)} onListen={canListen ? index => onNavigate(index, 'listen', source) : undefined} />}
  </aside>
}
