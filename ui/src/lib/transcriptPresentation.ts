import type { Job } from './types'

export interface ViewOptions { mode: 'fluid' | 'detailed'; pause_ms: number; show_timestamps: boolean }
export interface ExportOptions { linked: boolean; view: ViewOptions; content: 'complete' | 'transcript' }
export interface PresentationPreferences { schema_version: 1; screen: ViewOptions; export: ExportOptions; speaker_colors: Record<string, string> }
export interface PresentationState { preferences: PresentationPreferences; revision: number; warning: string | null; writable: boolean }
export interface TranscriptBlock { segment_indices: number[]; start_ms: number; end_ms: number; speaker_id: string | null }
export interface PresentationSnapshot { schema_version: 1; job_id: string; source_revision: string; options: ViewOptions; blocks: TranscriptBlock[] }

export const DEFAULT_VIEW: ViewOptions = { mode: 'fluid', pause_ms: 2000, show_timestamps: true }
export function defaultPreferences(): PresentationPreferences {
  return { schema_version: 1, screen: { ...DEFAULT_VIEW }, export: { linked: true, view: { ...DEFAULT_VIEW }, content: 'complete' }, speaker_colors: {} }
}
const record = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v)
const keys = (v: Record<string, unknown>, expected: string[]) => Object.keys(v).length === expected.length && expected.every(k => Object.hasOwn(v, k))
function invalid(): never { throw new Error('Réponse de présentation invalide ou incompatible.') }
export function validateView(value: unknown): ViewOptions {
  if (!record(value) || !keys(value, ['mode', 'pause_ms', 'show_timestamps']) || !['fluid', 'detailed'].includes(value.mode as string)
    || !Number.isSafeInteger(value.pause_ms) || (value.pause_ms as number) < 0 || (value.pause_ms as number) > 10000
    || (value.pause_ms as number) % 500 !== 0 || typeof value.show_timestamps !== 'boolean') invalid()
  return value as unknown as ViewOptions
}
export function validatePreferences(value: unknown, job?: Job, defaults = false): PresentationPreferences {
  if (!record(value) || !keys(value, ['schema_version', 'screen', 'export', 'speaker_colors']) || value.schema_version !== 1 || !record(value.export) || !keys(value.export, ['linked', 'view', 'content'])
    || typeof value.export.linked !== 'boolean' || !['complete', 'transcript'].includes(value.export.content as string) || !record(value.speaker_colors)) invalid()
  validateView(value.screen); validateView(value.export.view)
  const ids = job ? new Set(job.segments.map(s => s.speaker_id).filter((id): id is string => !!id)) : null
  for (const [id, color] of Object.entries(value.speaker_colors)) {
    if (!id.trim() || new TextEncoder().encode(id).length > 128 || /\p{Cc}/u.test(id)
      || typeof color !== 'string' || !/^#[0-9a-fA-F]{6}$/.test(color) || (ids && !ids.has(id)) || defaults) invalid()
  }
  return value as unknown as PresentationPreferences
}
export function parsePresentationState(value: unknown, job?: Job, defaults = false): PresentationState {
  if (!record(value) || !keys(value, ['preferences', 'revision', 'warning', 'writable']) || !Number.isSafeInteger(value.revision) || (value.revision as number) < 0
    || (value.warning !== null && typeof value.warning !== 'string') || typeof value.writable !== 'boolean') invalid()
  validatePreferences(value.preferences, job, defaults)
  return value as unknown as PresentationState
}
export async function sourceRevision(job: Job): Promise<string> {
  const tuples = job.segments.map(s => [s.start_ms, s.end_ms, s.text, s.speaker_id ?? null, s.translated_text ?? null])
  const bytes = new TextEncoder().encode(JSON.stringify(tuples))
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return Array.from(new Uint8Array(digest), b => b.toString(16).padStart(2, '0')).join('')
}
export function validateSnapshot(value: unknown, job: Job, revision: string, requested: ViewOptions): PresentationSnapshot {
  if (!record(value) || !keys(value, ['schema_version', 'job_id', 'source_revision', 'options', 'blocks']) || value.schema_version !== 1 || value.job_id !== job.id
    || value.source_revision !== revision || !/^[0-9a-f]{64}$/.test(value.source_revision as string)
    || !Array.isArray(value.blocks)) invalid()
  const options = validateView(value.options)
  if (options.mode !== requested.mode || options.pause_ms !== requested.pause_ms || options.show_timestamps !== requested.show_timestamps) invalid()
  let next = 0
  for (const raw of value.blocks) {
    if (!record(raw) || !keys(raw, ['segment_indices', 'start_ms', 'end_ms', 'speaker_id']) || !Array.isArray(raw.segment_indices) || raw.segment_indices.length === 0
      || !Number.isSafeInteger(raw.start_ms) || !Number.isSafeInteger(raw.end_ms) || (raw.start_ms as number) < 0 || (raw.end_ms as number) < 0
      || (raw.speaker_id !== null && typeof raw.speaker_id !== 'string')) invalid()
    if ((raw.end_ms as number) < (raw.start_ms as number) && raw.segment_indices.length !== 1) invalid()
    for (const index of raw.segment_indices) {
      if (index !== next || next >= job.segments.length || job.segments[next].speaker_id !== raw.speaker_id) invalid()
      if (index !== raw.segment_indices[0]) {
        const previous = job.segments[index - 1]
        const segment = job.segments[index]
        if (options.mode !== 'fluid' || !raw.speaker_id?.trim() || previous.start_ms > previous.end_ms
          || segment.start_ms > segment.end_ms || previous.end_ms > segment.start_ms
          || segment.start_ms - previous.end_ms > options.pause_ms) invalid()
      }
      next++
    }
    if (raw.start_ms !== job.segments[raw.segment_indices[0]].start_ms || raw.end_ms !== job.segments[next - 1].end_ms) invalid()
  }
  if (next !== job.segments.length) invalid()
  return value as unknown as PresentationSnapshot
}
export const effectiveExportView = (p: PresentationPreferences): ViewOptions => p.export.linked ? p.screen : p.export.view
