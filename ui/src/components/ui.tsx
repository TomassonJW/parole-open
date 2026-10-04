import { useId, type ReactNode } from 'react'
import { STAGE_LABEL, STAGE_TONE, formatPercent } from '../lib/format'
import type { Stage } from '../lib/types'

export function StageBadge({ stage }: { stage: Stage }) {
  return (
    <span className={`badge badge--${STAGE_TONE[stage]}`}>
      <span className="badge__dot" aria-hidden="true" />
      {STAGE_LABEL[stage]}
    </span>
  )
}

interface ProgressProps {
  label: string
  /** null = durée inconnue : barre indéterminée, sans pourcentage inventé. */
  ratio: number | null
  detail?: ReactNode
  remaining?: string
  size?: 'sm' | 'lg'
}

export function Progress({ label, ratio, detail, remaining, size = 'lg' }: ProgressProps) {
  const id = useId()
  const determinate = ratio !== null
  const value = determinate ? Math.round(ratio * 100) : undefined
  return (
    <div className={`progress progress--${size}`}>
      <div className="progress__head">
        <span id={id} className="progress__label">
          {label}
        </span>
        {determinate && <span className="progress__value">{formatPercent(ratio)}</span>}
      </div>
      <div
        className={`progress__track${determinate ? '' : ' progress__track--indeterminate'}${remaining && size === 'lg' ? ' progress__track--annotated' : ''}`}
        role="progressbar"
        aria-labelledby={id}
        aria-valuemin={determinate ? 0 : undefined}
        aria-valuemax={determinate ? 100 : undefined}
        aria-valuenow={value}
        aria-valuetext={`${determinate ? formatPercent(ratio) : 'Progression non communiquée'}${remaining ? ` ; ${remaining}` : ''}`}
      >
        <div className="progress__fill" style={determinate ? { width: `${value}%` } : undefined} />
        {remaining && size === 'lg' && <span className="progress__remaining" aria-hidden="true">{remaining}</span>}
      </div>
      {detail && <p className="progress__detail">{detail}</p>}
    </div>
  )
}

interface NoticeProps {
  tone: 'error' | 'warning' | 'info' | 'success'
  title: string
  children?: ReactNode
  detail?: string | null
  onDismiss?: () => void
  action?: ReactNode
}

export function Notice({ tone, title, children, detail, onDismiss, action }: NoticeProps) {
  const role = tone === 'error' ? 'alert' : 'status'
  return (
    <div className={`notice notice--${tone}`} role={role}>
      <div className="notice__body">
        <p className="notice__title">{title}</p>
        {children && <div className="notice__text">{children}</div>}
        {detail && (
          <details className="notice__detail">
            <summary>Détail technique</summary>
            <code>{detail}</code>
          </details>
        )}
        {action && <div className="notice__actions">{action}</div>}
      </div>
      {onDismiss && (
        <button type="button" className="icon-button" onClick={onDismiss} aria-label="Fermer le message">
          <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
            <path d="M5 5l10 10M15 5L5 15" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
          </svg>
        </button>
      )}
    </div>
  )
}

export function Spinner() {
  return <span className="spinner" aria-hidden="true" />
}
