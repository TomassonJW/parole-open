import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open } from '@tauri-apps/plugin-dialog'
import { MEDIA_EXTENSIONS } from './format'
import { normalizeJob, normalizeModelStatus, type ExportFormat, type Job, type ModelStatus, type StartOptions } from './types'
import type { TopicSnapshot } from './topicSnapshot'
import { parsePresentationState, validateView, validatePreferences, type PresentationPreferences, type PresentationSnapshot, type PresentationState, type ViewOptions } from './transcriptPresentation'

/** Erreur destinée à l'utilisateur, toujours en français, avec le détail technique conservé. */
export class BackendError extends Error {
  readonly detail: string | null
  readonly kind: 'unavailable' | 'missing-command' | 'failed' | 'invalid-response'
  constructor(message: string, kind: BackendError['kind'], detail: string | null = null) {
    super(message)
    this.name = 'BackendError'
    this.kind = kind
    this.detail = detail
  }
}

/**
 * Arguments `invoke` attendus par la couche Tauri (clés camelCase côté JS,
 * converties en snake_case par Tauri pour les paramètres des commandes Rust).
 */
export interface StartJobArgs {
  mediaPath: string
  sourceLanguage: string
  targetLanguage: string | null
  reportLanguage: string | null
  reportModelId: string
  generateReport: boolean
}
export interface IdArgs {
  id: string
}
export interface SaveSpeakerNamesArgs {
  id: string
  names: Record<string, string>
}
export interface ExportJobArgs {
  id: string
  format: ExportFormat
  defaultName: string
  presentation?: PresentationPreferences
}
export const JOB_PROGRESS_EVENT = 'job-progress'
export const JOB_ACTIVITY_EVENT = 'job-activity'
export type JobActivity = { id: string; phase: string; chunk: number; total: number }

export const UNAVAILABLE_MESSAGE =
  "Le moteur local n'est pas joignable. Ouvrez Parole depuis l'application de bureau pour importer et transcrire."

export interface Backend {
  readonly available: boolean
  modelStatus(): Promise<ModelStatus>
  prepareModels(): Promise<ModelStatus | null>
  installReportModel(id: string): Promise<ModelStatus>
  startJob(options: StartOptions): Promise<Job>
  resumeJob(id: string): Promise<Job>
  cancelJob(id: string): Promise<void>
  listJobs(): Promise<Job[]>
  /** Enveloppe binaire PAU1 issue du travail sauvegardé. */
  loadAudioAt(id: string, atMs: number): Promise<ArrayBuffer>
  loadTopicCandidates(id: string): Promise<TopicSnapshot>
  prepareTopicCandidates(id: string): Promise<TopicSnapshot>
  saveSpeakerNames(id: string, names: Record<string, string>): Promise<Job>
  exportJob(id: string, format: ExportFormat, defaultName: string, presentation?: PresentationPreferences): Promise<string | null>
  loadPresentation(id: string): Promise<PresentationState>
  savePresentation(id: string, preferences: PresentationPreferences, expectedRevision: number): Promise<PresentationState>
  previewPresentation(id: string, options: ViewOptions): Promise<PresentationSnapshot>
  loadPresentationDefaults(): Promise<PresentationState>
  savePresentationDefaults(preferences: PresentationPreferences, expectedRevision: number): Promise<PresentationState>
  pickMediaFile(): Promise<string | null>
  /** Mises à jour poussées par le moteur (événement « job-progress »), si le moteur les émet. */
  onModelProgress?(handler: (progress: { received: number; total: number; model?: string }) => void): Promise<() => void>
  onJobUpdated(handler: (job: Job) => void): Promise<() => void>
  onJobActivity?(handler: (activity: JobActivity) => void): Promise<() => void>
  onFileDrop(handler: (event: FileDropEvent) => void): Promise<() => void>
}

export type FileDropEvent = { type: 'hover' } | { type: 'leave' } | { type: 'drop'; paths: string[] }

function rawMessage(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  if (error && typeof error === 'object' && 'message' in error) return String((error as { message: unknown }).message)
  try {
    return JSON.stringify(error)
  } catch {
    return String(error)
  }
}

export function toBackendError(command: string, error: unknown): BackendError {
  if (error instanceof BackendError) return error
  const detail = rawMessage(error)
  if (/command\s+\S+\s+not\s+found/i.test(detail) || /not allowed|forbidden/i.test(detail)) {
    return new BackendError(
      `La fonction « ${command} » n'est pas encore disponible dans le moteur local.`,
      'missing-command',
      detail,
    )
  }
  return new BackendError(detail || `L'action « ${command} » a échoué sans message.`, 'failed', detail)
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) throw new BackendError(UNAVAILABLE_MESSAGE, 'unavailable')
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    throw toBackendError(command, error)
  }
}

function parseJob(command: string, raw: unknown): Job {
  try {
    return normalizeJob(raw)
  } catch (error) {
    throw new BackendError(rawMessage(error), 'invalid-response', `${command}: ${JSON.stringify(raw)}`)
  }
}

function parseTopicSnapshot(raw: unknown): TopicSnapshot {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) throw new BackendError('Réponse des pistes illisible.', 'invalid-response')
  const r = raw as Record<string, unknown>
  if (r.schema_version !== 1 || typeof r.job_id !== 'string' || typeof r.source_revision !== 'string'
    || !/^[0-9a-f]{64}$/.test(r.source_revision) || !Object.hasOwn(r, 'candidates')
    || (r.candidates !== null && (typeof r.candidates !== 'object' || Array.isArray(r.candidates)))) {
    throw new BackendError('Réponse des pistes illisible.', 'invalid-response')
  }
  return raw as TopicSnapshot
}

export const tauriBackend: Backend = {
  get available() {
    return isTauri()
  },
  async loadAudioAt(id, atMs) {
    if (!Number.isSafeInteger(atMs) || atMs < 0) throw new BackendError('Instant audio invalide.', 'invalid-response')
    // Never forward the backend's raw error detail (it could contain a local path).
    try { return await call<ArrayBuffer>('load_audio_at', { id, atMs }) }
    catch { throw new BackendError('Extrait audio indisponible ou non vérifiable.', 'failed') }
  },
  async loadTopicCandidates(id) {
    return parseTopicSnapshot(await call<unknown>('load_topic_candidates', { id }))
  },
  async prepareTopicCandidates(id) {
    return parseTopicSnapshot(await call<unknown>('prepare_topic_candidates', { id }))
  },
  async modelStatus() {
    const raw = await call<unknown>('model_status')
    try {
      return normalizeModelStatus(raw)
    } catch (error) {
      throw new BackendError(rawMessage(error), 'invalid-response', `model_status: ${JSON.stringify(raw)}`)
    }
  },
  async prepareModels() {
    const raw = await call<unknown>('prepare_models')
    // Si le moteur renvoie l'état final, on l'utilise ; sinon l'appelant relit model_status.
    try {
      return normalizeModelStatus(raw)
    } catch {
      return null
    }
  },
  async installReportModel(id) {
    const raw = await call<unknown>('install_report_model', { id })
    try {
      return normalizeModelStatus(raw)
    } catch (error) {
      throw new BackendError(rawMessage(error), 'invalid-response', `install_report_model: ${JSON.stringify(raw)}`)
    }
  },
  async startJob({ mediaPath, sourceLanguage, targetLanguage, reportLanguage, reportModelId, generateReport }) {
    const args: StartJobArgs = { mediaPath, sourceLanguage, targetLanguage, reportLanguage, reportModelId: reportModelId ?? 'baseline', generateReport }
    return parseJob('start_job', await call('start_job', { ...args }))
  },
  async resumeJob(id) {
    const args: IdArgs = { id }
    return parseJob('resume_job', await call('resume_job', { ...args }))
  },
  async cancelJob(id) {
    try {
      await invoke('cancel_job', { id })
    } catch (error) {
      throw toBackendError('cancel_job', error)
    }
  },
  async listJobs() {
    const raw = await call<unknown>('list_jobs')
    if (!Array.isArray(raw)) {
      throw new BackendError('Réponse du moteur illisible : une liste de traitements était attendue.', 'invalid-response')
    }
    return raw.map((item) => parseJob('list_jobs', item))
  },
  async saveSpeakerNames(id, names) {
    const args: SaveSpeakerNamesArgs = { id, names }
    return parseJob('save_speaker_names', await call('save_speaker_names', { ...args }))
  },
  async loadPresentation(id) { return parsePresentationState(await call('load_presentation', { id })) },
  async savePresentation(id, preferences, expectedRevision) {
    validatePreferences(preferences)
    return parsePresentationState(await call('save_presentation', { id, preferences, expectedRevision }))
  },
  async previewPresentation(id, options) {
    validateView(options)
    return await call<PresentationSnapshot>('preview_presentation', { id, options })
  },
  async loadPresentationDefaults() { return parsePresentationState(await call('load_presentation_defaults'), undefined, true) },
  async savePresentationDefaults(preferences, expectedRevision) {
    validatePreferences(preferences, undefined, true)
    return parsePresentationState(await call('save_presentation_defaults', { preferences, expectedRevision }), undefined, true)
  },
  async exportJob(id, format, defaultName, presentation) {
    if (presentation) validatePreferences(presentation)
    const args: ExportJobArgs = { id, format, defaultName, ...(presentation ? { presentation } : {}) }
    const written = await call<unknown>('export_job', { ...args })
    if (written === null) return null
    if (typeof written === 'string' && written) return written
    throw new BackendError("Réponse d'export illisible : aucun emplacement confirmé.", 'invalid-response')
  },
  async pickMediaFile() {
    if (!isTauri()) throw new BackendError(UNAVAILABLE_MESSAGE, 'unavailable')
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        title: 'Choisir un enregistrement',
        filters: [{ name: 'Audio et vidéo', extensions: [...MEDIA_EXTENSIONS] }],
      })
      return typeof picked === 'string' ? picked : null
    } catch (error) {
      throw toBackendError('plugin:dialog|open', error)
    }
  },
  async onModelProgress(handler) {
    if (!isTauri()) return () => {}
    return listen<unknown>('model-progress', (event) => {
      const value = event.payload
      if (!value || typeof value !== 'object') return
      const { received, total, model } = value as Record<string, unknown>
      if (typeof received === 'number' && typeof total === 'number' && Number.isFinite(received) && Number.isFinite(total) && total > 0 && received >= 0 && received <= total) {
        handler({ received, total, model: typeof model === 'string' ? model : undefined })
      }
    })
  },
  async onJobUpdated(handler) {
    if (!isTauri()) return () => {}
    return listen<unknown>(JOB_PROGRESS_EVENT, (event) => {
      try {
        handler(normalizeJob(event.payload))
      } catch {
        // Un événement mal formé est ignoré : la liste relue via list_jobs fait foi.
      }
    })
  },
  async onJobActivity(handler) {
    if (!isTauri()) return () => {}
    const phases = new Set(['préparation des modèles de voix', 'séparation des voix', 'préparation audio', 'transcription Whisper'])
    return listen<unknown>(JOB_ACTIVITY_EVENT, (event) => {
      const item = event.payload as Partial<JobActivity> | null
      if (item && typeof item.id === 'string' && typeof item.phase === 'string' && phases.has(item.phase)
        && Number.isInteger(item.chunk) && Number.isInteger(item.total) && (item.chunk ?? -1) >= 0
        && (item.total ?? 0) > 0 && (item.chunk ?? 0) < (item.total ?? 0)) {
        handler(item as JobActivity)
      }
    })
  },
  async onFileDrop(handler) {
    if (!isTauri()) return () => {}
    return getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload
      if (p.type === 'enter' || p.type === 'over') handler({ type: 'hover' })
      else if (p.type === 'leave') handler({ type: 'leave' })
      else if (p.type === 'drop') handler({ type: 'drop', paths: p.paths })
    })
  },
}
