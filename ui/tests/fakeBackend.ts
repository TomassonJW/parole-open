import { vi } from 'vitest'
import type { Backend, FileDropEvent, JobActivity } from '../src/lib/backend'
import type { Job, ModelStatus } from '../src/lib/types'
import { defaultPreferences, sourceRevision } from '../src/lib/transcriptPresentation'

export function makeJob(overrides: Partial<Job> = {}): Job {
  return {
    id: 'job-1',
    media_name: 'reunion.mp3',
    duration_ms: 120_000,
    chunk_ms: 30_000,
    completed_chunks: 0,
    stage: 'Ready',
    segments: [],
    speaker_names: {},
    error: null,
    source_language: null,
    target_language: null,
    report_language: null,
    report_model_id: 'baseline',
    generate_report: false,
    phase_done: 0,
    phase_total: 0,
    translation_issues: [],
    report: null,
    ...overrides,
  }
}

export function fakeBackend(overrides: Partial<Backend> = {}) {
  const dropHandlers: Array<(e: FileDropEvent) => void> = []
  const progressHandlers: Array<(j: Job) => void> = []
  const activityHandlers: Array<(a: JobActivity) => void> = []
  const installed: ModelStatus = { installed: true, missing: [] }
  const backend: Backend = {
    available: true,
    modelStatus: vi.fn(async () => installed),
    prepareModels: vi.fn(async () => installed),
    installReportModel: vi.fn(async () => installed),
    startJob: vi.fn(async () => makeJob({ stage: 'Transcribing' })),
    resumeJob: vi.fn(async (id: string) => makeJob({ id, stage: 'Transcribing' })),
    cancelJob: vi.fn(async () => {}),
    listJobs: vi.fn(async () => []),
    loadTopicCandidates: vi.fn(async () => { throw new Error('Pistes fictives absentes') }),
    prepareTopicCandidates: vi.fn(async () => { throw new Error('Pistes fictives absentes') }),
    loadAudioAt: vi.fn(async () => { throw new Error('Extrait audio fictif absent') }),
    saveSpeakerNames: vi.fn(async (id: string, names: Record<string, string>) => makeJob({ id, speaker_names: names })),
    exportJob: vi.fn(async (_id: string, format, name: string) => `/export/${name}.${format}`),
    loadPresentation: vi.fn(async () => ({ preferences: defaultPreferences(), revision: 0, warning: null, writable: true })),
    savePresentation: vi.fn(async (_id, preferences, expectedRevision) => ({ preferences, revision: expectedRevision + 1, warning: null, writable: true })),
    previewPresentation: vi.fn(async (id, options) => ({ schema_version: 1 as const, job_id: id, source_revision: await sourceRevision(makeJob({ id })), options, blocks: [] })),
    loadPresentationDefaults: vi.fn(async () => ({ preferences: defaultPreferences(), revision: 0, warning: null, writable: true })),
    savePresentationDefaults: vi.fn(async (preferences, expectedRevision) => ({ preferences, revision: expectedRevision + 1, warning: null, writable: true })),
    pickMediaFile: vi.fn(async () => '/media/reunion.mp3'),
    onJobUpdated: vi.fn(async (h: (j: Job) => void) => {
      progressHandlers.push(h)
      return () => {}
    }),
    onJobActivity: vi.fn(async (h: (a: JobActivity) => void) => {
      activityHandlers.push(h)
      return () => {}
    }),
    onFileDrop: vi.fn(async (h: (e: FileDropEvent) => void) => {
      dropHandlers.push(h)
      return () => {}
    }),
    ...overrides,
  }
  return {
    backend,
    drop: (e: FileDropEvent) => dropHandlers.forEach((h) => h(e)),
    push: (j: Job) => progressHandlers.forEach((h) => h(j)),
    step: (a: JobActivity) => activityHandlers.forEach((h) => h(a)),
  }
}
