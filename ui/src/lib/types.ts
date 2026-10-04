/**
 * Contrat de données partagé avec la couche Tauri.
 * Réponses : enveloppe `{ id, job }` où `job` suit les champs Rust en snake_case.
 */
export type Stage = 'Ready' | 'Transcribing' | 'Translating' | 'Reporting' | 'Interrupted' | 'Transcribed'

export interface Segment {
  start_ms: number
  end_ms: number
  text: string
  /** Aucun locuteur n'est inventé : null tant qu'aucune séparation des voix n'a eu lieu. */
  speaker_id: string | null
  translated_text: string | null
}

export interface JobTiming {
  platform?: string
  preparation_ms?: number
  transcription_ms: number
  translation_ms: number
  report_ms: number
  chunk_ms: number[]
  chunk_audio_ms: number[]
  active_since_ms: number
  active_chunk_since_ms?: number
}

export interface Job {
  /** Identifiant stable fourni par la couche Tauri (champ `id` de l'enveloppe). */
  id: string
  media_name: string
  duration_ms: number
  /** Taille d'une tranche ; 0 si le moteur ne la communique pas (pas de pourcentage calculé alors). */
  chunk_ms: number
  completed_chunks: number
  stage: Stage
  segments: Segment[]
  speaker_names: Record<string, string>
  error: string | null
  /** Langue source retenue par le moteur (« auto » ou code ISO), si communiquée. */
  source_language: string | null
  target_language: string | null
  report_language: string | null
  report_model_id: string
  generate_report: boolean
  phase_done: number
  phase_total: number
  translation_issues: number[]
  /** Compte rendu généré par le moteur, s'il existe. Jamais produit côté interface. */
  report: string | null
  /** 0 : ancien rapport brut ; 1 : champs échappés pour Markdown. */
  report_format_version?: number
  /** Mesures enregistrées uniquement sur cet ordinateur, facultatives pour les anciens travaux. */
  timing?: JobTiming
}

export interface ReportModelStatus {
  id: string
  name: string
  file: string
  bytes: number
  sha256: string
  url: string
  installed: boolean
  /** Les modèles facultatifs exigent une permission explicite ; absente dans les anciens moteurs. */
  available_for_new_jobs?: boolean
}

/** Réponse de `model_status`. */
export interface ModelStatus {
  installed: boolean
  /** Moteurs et modèles de transcription/voix, sans le modèle de langue de base. */
  core_installed?: boolean
  missing: string[]
  report_models?: ReportModelStatus[]
}

/** Options transmises à `start_job`. */
export interface StartOptions {
  mediaPath: string
  /** Code ISO 639-1, ou « auto » pour laisser le moteur détecter la langue. */
  sourceLanguage: string
  /** Code ISO 639-1 de traduction, ou null pour ne pas traduire. */
  targetLanguage: string | null
  reportLanguage: string | null
  reportModelId?: string
  generateReport: boolean
}

export const SOURCE_LANGUAGES: ReadonlyArray<{ value: string; label: string }> = [
  { value: 'auto', label: 'Détection automatique' },
  { value: 'fr', label: 'Français' },
  { value: 'en', label: 'Anglais' },
  { value: 'es', label: 'Espagnol' },
  { value: 'de', label: 'Allemand' },
  { value: 'it', label: 'Italien' },
  { value: 'pt', label: 'Portugais' },
  { value: 'nl', label: 'Néerlandais' },
]

export const TARGET_LANGUAGES: ReadonlyArray<{ value: string; label: string }> = [
  { value: '', label: 'Aucune traduction' },
  ...SOURCE_LANGUAGES.filter((l) => ['fr', 'en'].includes(l.value)),
]

export function normalizeModelStatus(raw: unknown): ModelStatus {
  if (!raw || typeof raw !== 'object' || typeof (raw as Record<string, unknown>).installed !== 'boolean') {
    throw new Error('Réponse du moteur illisible : état des modèles attendu.')
  }
  const r = raw as Record<string, unknown>
  const missing = Array.isArray(r.missing) ? r.missing.filter((m): m is string => typeof m === 'string') : []
  const report_models = Array.isArray(r.report_models)
    ? r.report_models.map((entry: unknown) => {
      if (!entry || typeof entry !== 'object') throw new Error('Catalogue des modèles de compte rendu illisible.')
      const m = entry as Record<string, unknown>
      if (typeof m.id !== 'string' || typeof m.name !== 'string' || typeof m.file !== 'string'
        || typeof m.bytes !== 'number' || !Number.isSafeInteger(m.bytes) || m.bytes <= 0
        || typeof m.sha256 !== 'string' || typeof m.url !== 'string' || typeof m.installed !== 'boolean'
        || (m.available_for_new_jobs !== undefined && typeof m.available_for_new_jobs !== 'boolean')) {
        throw new Error('Catalogue des modèles de compte rendu illisible.')
      }
      return { id: m.id, name: m.name, file: m.file, bytes: m.bytes, sha256: m.sha256, url: m.url, installed: m.installed,
        available_for_new_jobs: m.available_for_new_jobs === true || (m.id === 'baseline' && m.available_for_new_jobs === undefined) }
    })
    : undefined
  if (r.core_installed !== undefined && typeof r.core_installed !== 'boolean') {
    throw new Error('Réponse du moteur illisible : état du socle attendu.')
  }
  return { installed: r.installed as boolean, ...(typeof r.core_installed === 'boolean' ? { core_installed: r.core_installed } : {}), missing, ...(report_models ? { report_models } : {}) }
}

export type ExportFormat = 'txt' | 'md' | 'docx' | 'srt' | 'vtt' | 'json'

export function needsRecovery(job: Job): boolean {
  return job.stage === 'Interrupted'
    || (job.stage === 'Transcribed' && job.target_language !== null && job.translation_issues.length > 0)
}

export const EXPORT_FORMATS: ReadonlyArray<{ value: ExportFormat; label: string; hint: string }> = [
  { value: 'txt', label: 'Texte (.txt)', hint: 'Transcription, traduction et compte rendu.' },
  { value: 'md', label: 'Document Markdown (.md)', hint: 'Document structuré, lisible dans un éditeur.' },
  { value: 'docx', label: 'Document Word (.docx)', hint: 'Transcription, traduction et compte rendu dans un document.' },
  { value: 'srt', label: 'Sous-titres SRT (.srt)', hint: 'Pour les lecteurs vidéo et le montage.' },
  { value: 'vtt', label: 'Sous-titres WebVTT (.vtt)', hint: 'Pour le web et les navigateurs.' },
  { value: 'json', label: 'Données JSON (.json)', hint: 'Pour un traitement automatique.' },
]

const STAGES: readonly Stage[] = ['Ready', 'Transcribing', 'Translating', 'Reporting', 'Interrupted', 'Transcribed']

function asNumber(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : 0
}

function asString(value: unknown): string | null {
  return typeof value === 'string' ? value : null
}

function normalizeTiming(raw: unknown): JobTiming {
  const r = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {}
  const numbers = (value: unknown): number[] => Array.isArray(value)
    ? value.slice(0, 10000).filter((n): n is number => typeof n === 'number' && Number.isFinite(n) && n >= 0)
    : []
  return {
    platform: typeof r.platform === 'string' && ['windows', 'macos', 'linux'].includes(r.platform) ? r.platform : '',
    preparation_ms: asNumber(r.preparation_ms),
    transcription_ms: asNumber(r.transcription_ms),
    translation_ms: asNumber(r.translation_ms),
    report_ms: asNumber(r.report_ms),
    chunk_ms: numbers(r.chunk_ms),
    chunk_audio_ms: numbers(r.chunk_audio_ms),
    active_since_ms: asNumber(r.active_since_ms),
    active_chunk_since_ms: asNumber(r.active_chunk_since_ms),
  }
}

function normalizeSegment(raw: unknown): Segment | null {
  if (!raw || typeof raw !== 'object') return null
  const r = raw as Record<string, unknown>
  const text = asString(r.text)
  if (text === null) return null
  return {
    start_ms: asNumber(r.start_ms),
    end_ms: asNumber(r.end_ms),
    text,
    speaker_id: asString(r.speaker_id),
    translated_text: asString(r.translated_text),
  }
}

/**
 * Valide une réponse du moteur. Lève une erreur explicite plutôt que
 * d'afficher un état inventé si la forme reçue ne correspond pas au contrat.
 */
export function normalizeJob(raw: unknown): Job {
  if (!raw || typeof raw !== 'object') {
    throw new Error('Réponse du moteur illisible : un traitement était attendu.')
  }
  const envelope = raw as Record<string, unknown>
  const id = asString(envelope.id)
  if (!id) {
    throw new Error('Réponse du moteur incomplète : identifiant du traitement manquant.')
  }
  const inner = envelope.job
  if (!inner || typeof inner !== 'object') {
    throw new Error('Réponse du moteur incomplète : détail du traitement manquant.')
  }
  const r = inner as Record<string, unknown>
  const media = asString(r.media_name)
  if (!media) {
    throw new Error('Réponse du moteur incomplète : nom du fichier manquant.')
  }
  const stage = STAGES.includes(r.stage as Stage) ? (r.stage as Stage) : null
  if (!stage) {
    throw new Error(`Réponse du moteur incohérente : état « ${String(r.stage)} » inconnu.`)
  }
  const names: Record<string, string> = r.speaker_names && typeof r.speaker_names === 'object'
    ? Object.fromEntries(Object.entries(r.speaker_names).filter((entry): entry is [string, string] => typeof entry[1] === 'string'))
    : {}
  const segments = Array.isArray(r.segments)
    ? r.segments.map(normalizeSegment).filter((s): s is Segment => s !== null)
    : []
  return {
    id,
    media_name: media,
    duration_ms: asNumber(r.duration_ms),
    chunk_ms: asNumber(r.chunk_ms),
    completed_chunks: asNumber(r.completed_chunks),
    stage,
    segments,
    speaker_names: names,
    error: asString(r.error),
    source_language: asString(r.source_language),
    target_language: asString(r.target_language),
    report_language: asString(r.report_language),
    report_model_id: typeof r.report_model_id === 'string' ? r.report_model_id : 'baseline',
    generate_report: r.generate_report === true,
    phase_done: asNumber(r.phase_done),
    phase_total: asNumber(r.phase_total),
    translation_issues: Array.isArray(r.translation_issues) ? r.translation_issues.filter((n): n is number => Number.isInteger(n) && n >= 0) : [],
    report: asString(r.report),
    report_format_version: r.report_format_version === 1 ? 1 : 0,
    timing: normalizeTiming(r.timing),
  }
}
