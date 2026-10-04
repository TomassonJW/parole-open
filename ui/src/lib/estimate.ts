import { formatDuration, processedMs } from './format'
import type { Job } from './types'

type Platform = 'windows' | 'mac'
export type SpeechEstimate = {
  elapsedMs: number
  remainingMs: number
  lowMs: number
  highMs: number
  rate: number
  source: 'initiale' | 'mesurée' | 'dépassée' | 'calibrage'
  samples: number
}

/** Repère Windows approximatif : 40 s pour 20 s d'audio, préparation comprise. */
const INITIAL_RATE = 2

export function estimateSpeech(job: Job, history: Job[], now: number, platform: Platform): SpeechEstimate {
  let measuredAudio = 0
  let measuredElapsed = 0
  let samples = 0
  const expectedPlatform = platform === 'mac' ? 'macos' : 'windows'
  const previous = history.filter((x) => x.id !== job.id && x.timing?.platform === expectedPlatform)
  const current = !job.timing?.platform || job.timing.platform === expectedPlatform ? [job] : []
  for (const item of [...previous, ...current]) {
    const elapsed = item.timing?.chunk_ms ?? []
    const audio = item.timing?.chunk_audio_ms ?? []
    for (let i = 0; i < Math.min(elapsed.length, audio.length); i++) {
      if (audio[i] > 0 && elapsed[i] > 0) {
        measuredAudio += audio[i]
        measuredElapsed += elapsed[i]
        samples++
      }
    }
  }
  // Le repère Windows ne doit jamais devenir une mesure supposée du M3.
  const priorAudio = 30_000
  const rate = platform === 'mac'
    ? measuredAudio > 0 ? measuredElapsed / measuredAudio : 0
    : (measuredElapsed + priorAudio * INITIAL_RATE) / (measuredAudio + priorAudio)
  const activeMs = job.stage === 'Transcribing' && job.timing?.active_since_ms
    ? Math.max(0, now - job.timing.active_since_ms)
    : 0
  const elapsedMs = (job.timing?.preparation_ms ?? 0) + (job.timing?.transcription_ms ?? 0) + activeMs
  // La préparation est chronométrée mais ne traite pas encore d'audio.
  const activeChunkMs = job.stage === 'Transcribing' && job.timing?.active_chunk_since_ms
    ? Math.max(0, now - job.timing.active_chunk_since_ms)
    : 0
  if (platform === 'mac' && samples === 0) {
    return { elapsedMs, remainingMs: 0, lowMs: 0, highMs: 0, rate: 0, source: 'calibrage', samples }
  }
  const audioLeft = Math.max(0, job.duration_ms - processedMs(job))
  const expected = audioLeft * rate - activeChunkMs
  const initialMin = platform === 'mac' ? 0.5 : 0.75
  const initialMax = platform === 'mac' ? 3 : 3.5
  const lowRate = samples ? rate * 0.65 : initialMin
  const highRate = samples ? rate * 1.65 : initialMax
  return {
    elapsedMs,
    remainingMs: Math.max(0, expected),
    lowMs: Math.max(0, audioLeft * lowRate - activeChunkMs),
    highMs: Math.max(0, audioLeft * highRate - activeChunkMs),
    rate,
    source: expected < 0 && job.stage === 'Transcribing' ? 'dépassée' : samples ? 'mesurée' : 'initiale',
    samples,
  }
}

export interface ReportEstimate {
  elapsedMs: number
  remainingMs: number | null
  source: 'mesurée' | 'historique' | 'calibrage' | 'dépassée'
}

export function estimateReport(job: Job, history: Job[], now: number, platform: Platform): ReportEstimate {
  const expectedPlatform = platform === 'mac' ? 'macos' : 'windows'
  const completed = Math.min(job.phase_done, job.phase_total)
  const activeMs = job.stage === 'Reporting' && job.timing?.active_since_ms
    ? Math.max(0, now - job.timing.active_since_ms)
    : 0
  const measuredMs = job.timing?.report_ms ?? 0
  const elapsedMs = measuredMs + activeMs
  const prior = history.filter((item) => item.id !== job.id && item.timing?.platform === expectedPlatform && item.stage === 'Transcribed' && item.phase_total > 0 && (item.timing?.report_ms ?? 0) > 0)
  const historicalSteps = prior.reduce((sum, item) => sum + item.phase_total, 0)
  const historicalMs = prior.reduce((sum, item) => sum + (item.timing?.report_ms ?? 0), 0)
  const measuredSteps = measuredMs > 0 ? completed : 0
  const steps = measuredSteps + (historicalSteps ? 1 : 0)
  if (steps === 0 || job.phase_total <= 0) return { elapsedMs, remainingMs: null, source: 'calibrage' }
  const rate = (measuredMs + (historicalSteps ? historicalMs / historicalSteps : 0)) / steps
  const expected = (job.phase_total - completed) * rate - activeMs
  return {
    elapsedMs,
    remainingMs: Math.max(0, expected),
    source: expected < 0 && job.stage === 'Reporting' ? 'dépassée' : measuredSteps ? 'mesurée' : 'historique',
  }
}

export function formatReportEstimate(value: ReportEstimate): string {
  if (value.source === 'calibrage') return 'Compte rendu : estimation disponible après le premier passage mesuré.'
  if (value.source === 'dépassée') return 'Compte rendu : estimation dépassée ; génération toujours en cours.'
  return `Compte rendu restant : environ ${formatDuration(value.remainingMs ?? 0)} (${value.source === 'historique' ? 'd’après les anciens comptes rendus' : 'recalculé après les passages terminés'}).`
}

export function formatEstimate(value: SpeechEstimate): string {
  if (value.source === 'calibrage') return 'Estimation du Mac indisponible avant le premier passage terminé ; chronomètre et progression restent actifs.'
  if (value.source === 'dépassée') return 'Estimation dépassée : calcul toujours en cours ; durée à recalibrer.'
  const label = value.source === 'initiale' ? 'estimation initiale non mesurée' : `estimation affinée sur ${value.samples} tranche${value.samples > 1 ? 's' : ''}`
  return `Transcription restante : environ ${formatDuration(value.remainingMs)} (fourchette ${formatDuration(value.lowMs)} à ${formatDuration(value.highMs)}, ${label}).`
}
