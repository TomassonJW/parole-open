import { useId, useState, type FormEvent } from 'react'
import { baseName } from '../lib/format'
import { SOURCE_LANGUAGES, TARGET_LANGUAGES, type ModelStatus, type ReportModelStatus, type StartOptions } from '../lib/types'
import { Spinner } from './ui'

interface StartFormProps {
  path: string
  busy: boolean
  blockedReason: string | null
  modelStatus: ModelStatus | null
  reportModels: ReportModelStatus[]
  installError: string | null
  installProgress: { received: number; total: number } | null
  onInstall: (id: string) => void
  onStart: (options: StartOptions) => void
  onCancel: () => void
}

export function StartForm({ path, busy, blockedReason, modelStatus, reportModels, installError, installProgress, onInstall, onStart, onCancel }: StartFormProps) {
  const id = useId()
  const [source, setSource] = useState('auto')
  const [target, setTarget] = useState('')
  const [report, setReport] = useState(false)
  const [reportLanguage, setReportLanguage] = useState('')
  const [reportModelId, setReportModelId] = useState('baseline')
  const availableReportModels = reportModels.filter((model) => model.available_for_new_jobs === true
    || (model.id === 'baseline' && model.available_for_new_jobs !== false))
  const selectedModel = availableReportModels.find((model) => model.id === reportModelId)
  const modelIssue = report && !selectedModel && (reportModelId !== 'baseline' || reportModels.length > 0)
    ? 'Ce modèle de compte rendu est indisponible ou retiré des nouveaux traitements.'
    : report && selectedModel && !selectedModel.installed
      ? 'Modèle de compte rendu non installé : installez-le explicitement avant de lancer.'
      : null
  const reportIssue = !report || !reportLanguage || reportLanguage === target
    ? null
    : source === 'auto'
      ? 'Pour un compte rendu dans la langue des paroles, il faut préciser la langue parlée.'
      : source !== reportLanguage
        ? 'La langue du compte rendu doit être la langue des paroles ou de la traduction.'
        : null
  const coreReady = modelStatus?.core_installed ?? modelStatus?.installed ?? true
  const needsBaseline = Boolean(target) || (report && reportModelId === 'baseline')
  const baselineReady = reportModels.find((model) => model.id === 'baseline')?.installed ?? modelStatus?.installed ?? true
  const setupIssue = !coreReady || (needsBaseline && !baselineReady)
    ? 'Préparez les modèles nécessaires avant de lancer ce travail.' : null
  const reason = blockedReason ?? setupIssue ?? reportIssue ?? modelIssue

  function submit(event: FormEvent) {
    event.preventDefault()
    if (busy || reason) return
    onStart({ mediaPath: path, sourceLanguage: source, targetLanguage: target || null, reportLanguage: report ? reportLanguage || null : null, reportModelId: report ? reportModelId : 'baseline', generateReport: report })
  }

  return (
    <form className="panel panel--focus form" onSubmit={submit} aria-labelledby={`${id}-title`}>
      <div className="file-card">
        <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" className="file-card__icon">
          <path d="M5 10v4M9 7v10M13 9v6M17 5v14M21 10v4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
        <div className="file-card__text">
          <h2 id={`${id}-title`} className="file-card__name">
            {baseName(path)}
          </h2>
          <p className="file-card__path" title={path}>
            {path}
          </p>
        </div>
        <button type="button" className="button button--ghost button--sm" onClick={onCancel} disabled={busy}>
          Changer de fichier
        </button>
      </div>

      <div className="form__grid">
        <div className="field">
          <label className="field__label" htmlFor={`${id}-source`}>
            Langue parlée
          </label>
          <select id={`${id}-source`} className="input" value={source} onChange={(e) => setSource(e.target.value)} disabled={busy}>
            {SOURCE_LANGUAGES.map((l) => (
              <option key={l.value} value={l.value}>
                {l.label}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label className="field__label" htmlFor={`${id}-target`}>
            Traduire vers
          </label>
          <select id={`${id}-target`} className="input" value={target} onChange={(e) => setTarget(e.target.value)} disabled={busy}>
            {TARGET_LANGUAGES.map((l) => (
              <option key={l.value || 'none'} value={l.value}>
                {l.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      <label className="check">
        <input type="checkbox" checked={report} onChange={(e) => setReport(e.target.checked)} disabled={busy} />
        <span>
          <span className="check__label">Générer un compte rendu</span>
          <span className="check__hint">Passages cités et horodatés, à vérifier avant diffusion.</span>
        </span>
      </label>

      {report && (
        <div className="field">
          <label className="field__label" htmlFor={`${id}-report-language`}>Langue du compte rendu</label>
          <select id={`${id}-report-language`} className="input" value={reportLanguage} onChange={(event) => setReportLanguage(event.target.value)} disabled={busy}>
            <option value="">{target ? 'Même langue que la traduction' : 'Même langue que la transcription'}</option>
            <option value="fr">Français</option>
            <option value="en">Anglais</option>
          </select>
          <span className="check__hint">Choisis la langue des paroles ou de la traduction. Les citations proviennent du texte utilisé ; vérifie-les avant diffusion.</span>
        </div>
      )}

      {report && (
        <div className="field">
          <label className="field__label" htmlFor={`${id}-report-model`}>Modèle du compte rendu</label>
          <select id={`${id}-report-model`} className="input" value={reportModelId} onChange={(event) => setReportModelId(event.target.value)} disabled={busy}>
            {reportModels.length === 0 && <option value="baseline">Modèle de base (Qwen2.5-1.5B)</option>}
            {availableReportModels.map((model) => <option key={model.id} value={model.id}>{model.id === 'baseline' ? `Modèle de base - ${model.name}` : model.name}</option>)}
          </select>
          {selectedModel && <span className="check__hint">Fichier local : {(selectedModel.bytes / 1_000_000_000).toFixed(2)} Go. {selectedModel.installed ? 'Installé.' : 'Non installé.'} Traduction : modèle de base inchangé. Les comptes rendus rédigés doivent être relus avec leurs sources.</span>}
          {selectedModel && !selectedModel.installed && selectedModel.id !== 'baseline' && (
            <button type="button" className="button button--secondary button--sm" disabled={busy} onClick={() => onInstall(selectedModel.id)}>
              Installer ce modèle ({(selectedModel.bytes / 1_000_000_000).toFixed(2)} Go)
            </button>
          )}
          {busy && selectedModel?.id !== 'baseline' && installProgress && <p className="muted small" role="status">Téléchargement et vérification locale : {Math.round(installProgress.received / 1_000_000)} Mo reçus sur {Math.round(installProgress.total / 1_000_000)} Mo.</p>}
          {installError && <p className="status status--warning small" role="alert">Installation impossible : {installError}</p>}
        </div>
      )}

      {reason && (
        <p className="status status--warning small" id={`${id}-blocked`}>
          {reason}
        </p>
      )}

      <div className="actions">
        <button
          type="submit"
          className="button button--primary"
          disabled={busy || reason !== null}
          aria-describedby={reason ? `${id}-blocked` : undefined}
        >
          {busy ? (
            <>
              <Spinner /> Démarrage…
            </>
          ) : (
            'Lancer la transcription'
          )}
        </button>
      </div>
    </form>
  )
}
