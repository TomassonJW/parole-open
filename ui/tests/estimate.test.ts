import { describe, expect, it } from 'vitest'
import { estimateReport, estimateSpeech, formatEstimate } from '../src/lib/estimate'
import { makeJob } from './fakeBackend'

describe('durées locales non télémétriques', () => {
  it('annonce une fourchette initiale large et le temps écoulé plutôt qu’un faux pourcentage', () => {
    const job = makeJob({ duration_ms: 240_000, chunk_ms: 30_000, stage: 'Transcribing', timing: { transcription_ms: 0, translation_ms: 0, report_ms: 0, chunk_ms: [], chunk_audio_ms: [], active_since_ms: 1_000, active_chunk_since_ms: 1_000 } })
    const estimate = estimateSpeech(job, [], 11_000, 'windows')
    expect(estimate.elapsedMs).toBe(10_000)
    expect(estimate.rate).toBe(2)
    expect(estimate.remainingMs).toBeLessThan(480_000)
    expect(estimate.remainingMs).toBeGreaterThan(0)
    expect(formatEstimate(estimate)).toContain('estimation initiale')
  })

  it('ne remet pas le chronomètre à zéro entre préparation des voix et premier passage Whisper', () => {
    const preparing = makeJob({ stage: 'Transcribing', duration_ms: 60_000, timing: { preparation_ms: 0, transcription_ms: 0, translation_ms: 0, report_ms: 0, chunk_ms: [], chunk_audio_ms: [], active_since_ms: 10_000 } })
    const first = estimateSpeech(preparing, [], 15_000, 'windows')
    const started = makeJob({ stage: 'Transcribing', duration_ms: 60_000, timing: { preparation_ms: 5_000, transcription_ms: 0, translation_ms: 0, report_ms: 0, chunk_ms: [], chunk_audio_ms: [], active_since_ms: 15_000 } })
    const second = estimateSpeech(started, [], 15_000, 'windows')
    expect(first.elapsedMs).toBe(5_000)
    expect(second.elapsedMs).toBe(5_000)
  })

  it('ne déduit pas la préparation des voix des passages Whisper restant à calculer', () => {
    const preparing = makeJob({ stage: 'Transcribing', duration_ms: 60_000, timing: { preparation_ms: 0, transcription_ms: 0, translation_ms: 0, report_ms: 0, chunk_ms: [], chunk_audio_ms: [], active_since_ms: 1_000, active_chunk_since_ms: 0 } })
    const initial = estimateSpeech(preparing, [], 1_000, 'windows')
    const longPreparation = estimateSpeech(preparing, [], 301_000, 'windows')
    expect(longPreparation.elapsedMs).toBe(300_000)
    expect(longPreparation.remainingMs).toBe(initial.remainingMs)
    expect(longPreparation.source).toBe('initiale')
    const started = makeJob({ stage: 'Transcribing', duration_ms: 60_000, timing: { preparation_ms: 300_000, transcription_ms: 0, translation_ms: 0, report_ms: 0, chunk_ms: [], chunk_audio_ms: [], active_since_ms: 301_000, active_chunk_since_ms: 301_000 } })
    expect(estimateSpeech(started, [], 302_000, 'windows').remainingMs).toBe(initial.remainingMs - 1_000)
  })

  it('réduit le temps restant chaque seconde et affine après une tranche mesurée', () => {
    const job = makeJob({ duration_ms: 120_000, chunk_ms: 30_000, completed_chunks: 1, stage: 'Transcribing', timing: { transcription_ms: 60_000, translation_ms: 0, report_ms: 0, chunk_ms: [60_000], chunk_audio_ms: [30_000], active_since_ms: 100_000, active_chunk_since_ms: 100_000 } })
    const first = estimateSpeech(job, [], 101_000, 'windows')
    const next = estimateSpeech(job, [], 102_000, 'windows')
    expect(next.remainingMs).toBeLessThan(first.remainingMs)
    expect(next.rate).toBeGreaterThan(1.5)
    expect(next.source).toBe('mesurée')
  })

  it('réutilise les mesures du même système seulement et refuse une prévision Mac non mesurée', () => {
    const windows = makeJob({ id: 'ancien-pc', duration_ms: 30_000, timing: { platform: 'windows', transcription_ms: 60_000, translation_ms: 0, report_ms: 0, chunk_ms: [60_000], chunk_audio_ms: [30_000], active_since_ms: 0 } })
    const mac = makeJob({ id: 'ancien-mac', duration_ms: 30_000, timing: { platform: 'macos', transcription_ms: 30_000, translation_ms: 0, report_ms: 0, chunk_ms: [30_000], chunk_audio_ms: [30_000], active_since_ms: 0 } })
    const current = makeJob({ id: 'actuel', duration_ms: 60_000, stage: 'Transcribing' })
    const unmeasured = estimateSpeech(current, [windows], 0, 'mac')
    expect(unmeasured.source).toBe('calibrage')
    expect(formatEstimate(unmeasured)).toContain('premier passage terminé')
    expect(estimateSpeech(current, [windows, mac], 0, 'mac').samples).toBe(1)
    expect(estimateSpeech(current, [windows, mac], 0, 'mac').rate).toBeLessThan(1.5)
    expect(estimateSpeech(current, [windows, mac], 0, 'windows').samples).toBe(1)
  })

  it('recalcule le temps du compte rendu sur les étapes et ne prend pas les mesures Windows sur Mac', () => {
    const previous = makeJob({ id: 'pc', phase_done: 4, phase_total: 4, timing: { platform: 'windows', transcription_ms: 0, translation_ms: 0, report_ms: 80_000, chunk_ms: [], chunk_audio_ms: [], active_since_ms: 0 } })
    const unmeasured = makeJob({ id: 'mac', stage: 'Reporting', phase_done: 0, phase_total: 4 })
    expect(estimateReport(unmeasured, [previous], 2_000, 'mac').remainingMs).toBeNull()
    const job = makeJob({ id: 'mac', stage: 'Reporting', phase_done: 1, phase_total: 4, timing: { platform: 'macos', transcription_ms: 0, translation_ms: 0, report_ms: 12_000, chunk_ms: [], chunk_audio_ms: [], active_since_ms: 100_000 } })
    const first = estimateReport(job, [previous], 101_000, 'mac')
    const next = estimateReport(job, [previous], 102_000, 'mac')
    expect(first.remainingMs).toBe(35_000)
    expect(next.remainingMs).toBe(34_000)
    expect(next.source).toBe('mesurée')
    expect(next.elapsedMs).toBe(14_000)
  })
})
