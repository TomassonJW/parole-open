import type { Job, Segment, Stage } from './types'

/** Même calcul que `Job::chunks` côté Rust. */
export function chunkCount(job: Pick<Job, 'duration_ms' | 'chunk_ms'>): number {
  if (job.chunk_ms <= 0) return 0
  return Math.ceil(job.duration_ms / job.chunk_ms)
}

/** Pourcentage de la phase actuelle : uniquement les unités effectivement terminées. */
export function progressRatio(job: Pick<Job, 'duration_ms' | 'chunk_ms' | 'completed_chunks'> & Partial<Pick<Job, 'target_language' | 'segments' | 'generate_report' | 'stage' | 'phase_done' | 'phase_total'>>): number {
  const total = chunkCount(job)
  const speech = total === 0 ? 0 : Math.min(1, Math.max(0, job.completed_chunks / total))
  if (job.stage === 'Transcribed') return 1
  if (job.stage === 'Translating') {
    const count = job.segments?.length ?? 0
    return count ? Math.min(1, job.segments!.filter((s) => s.translated_text !== null).length / count) : 0
  }
  if (job.stage === 'Reporting') {
    return job.phase_total ? Math.min(0.95, (job.phase_done ?? 0) / job.phase_total) : 0
  }
  return speech
}

export function processedMs(job: Pick<Job, 'duration_ms' | 'chunk_ms' | 'completed_chunks'>): number {
  return Math.min(job.duration_ms, job.completed_chunks * job.chunk_ms)
}

/** 00:01:05 — ou 01:05 quand la durée est inférieure à une heure. */
export function formatClock(ms: number, forceHours = false): string {
  const total = Math.max(0, Math.floor(ms / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  const pad = (n: number) => String(n).padStart(2, '0')
  return h > 0 || forceHours ? `${pad(h)}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`
}

/** « 1 h 05 min », « 12 min 30 s », « 45 s ». */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  if (h > 0) return `${h} h ${String(m).padStart(2, '0')} min`
  if (m > 0) return s > 0 ? `${m} min ${String(s).padStart(2, '0')} s` : `${m} min`
  return `${s} s`
}

export function formatPercent(ratio: number): string {
  return `${Math.floor(ratio * 100)} %`
}

export const STAGE_LABEL: Record<Stage, string> = {
  Ready: 'En attente',
  Transcribing: 'Voix et transcription',
  Translating: 'Traduction en cours',
  Reporting: 'Compte rendu en cours',
  Interrupted: 'Interrompue',
  Transcribed: 'Terminée',
}

export const STAGE_TONE: Record<Stage, 'neutral' | 'active' | 'warning' | 'success'> = {
  Ready: 'neutral',
  Transcribing: 'active',
  Translating: 'active',
  Reporting: 'active',
  Interrupted: 'warning',
  Transcribed: 'success',
}

export const UNASSIGNED_SPEAKER = 'Locuteur non attribué'

/** Un nom enregistré est une chaîne propre au dictionnaire, jamais une valeur héritée. */
export function storedSpeakerName(names: Record<string, string>, id: string): string {
  const value = Object.hasOwn(names, id) ? names[id] : undefined
  return typeof value === 'string' ? value : ''
}

export function speakerLabel(job: Pick<Job, 'speaker_names'>, segment: Pick<Segment, 'speaker_id'>): string {
  if (!segment.speaker_id) return UNASSIGNED_SPEAKER
  const name = storedSpeakerName(job.speaker_names, segment.speaker_id).trim()
  return name ? name : segment.speaker_id
}

export function speakerIds(job: Pick<Job, 'segments'>): string[] {
  const seen = new Set<string>()
  for (const s of job.segments) if (s.speaker_id) seen.add(s.speaker_id)
  return [...seen]
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]/)
  return parts[parts.length - 1] || path
}

export function stripExtension(name: string): string {
  const i = name.lastIndexOf('.')
  return i > 0 ? name.slice(0, i) : name
}

export const MEDIA_EXTENSIONS = [
  'mp3', 'wav', 'm4a', 'aac', 'flac', 'ogg', 'opus', 'wma',
  'mp4', 'mov', 'mkv', 'webm', 'avi', 'm4v',
] as const

export function isSupportedMedia(path: string): boolean {
  const ext = baseName(path).split('.').pop()?.toLowerCase() ?? ''
  return (MEDIA_EXTENSIONS as readonly string[]).includes(ext)
}
