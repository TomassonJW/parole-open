import type { Backend } from './backend'
import type { Job } from './types'
import { parsePresentationState, validatePreferences, type PresentationPreferences, type PresentationState } from './transcriptPresentation'

const errorText = (e: unknown) => e instanceof Error ? e.message : String(e)
export const samePreferences = (a: PresentationPreferences, b: PresentationPreferences) =>
  a.schema_version === b.schema_version
  && a.screen.mode === b.screen.mode && a.screen.pause_ms === b.screen.pause_ms && a.screen.show_timestamps === b.screen.show_timestamps
  && a.export.linked === b.export.linked && a.export.content === b.export.content
  && a.export.view.mode === b.export.view.mode && a.export.view.pause_ms === b.export.view.pause_ms
  && a.export.view.show_timestamps === b.export.view.show_timestamps
  && Object.keys(a.speaker_colors).length === Object.keys(b.speaker_colors).length
  && Object.entries(a.speaker_colors).every(([id, color]) => b.speaker_colors[id] === color)
export const acknowledged = (response: PresentationState, revision: number, requested: PresentationPreferences) =>
  response.revision === revision + 1 && response.writable && samePreferences(response.preferences, requested)

export type SaveView = { state: PresentationState | null; draft: PresentationPreferences | null; error: string | null; saving: boolean }
type Listener = (view: SaveView) => void
// Backend identity matters: two providers with the same job id must not share revisions.
const sessions = new WeakMap<Backend, Map<string, SaveSession>>()

export class SaveSession {
  private listeners = new Set<Listener>()
  private view: SaveView = { state: null, draft: null, error: null, saving: false }
  private loading = false
  private blocked = false
  // Only speaker ids survive navigation; no media, text, or full transcript is cached.
  private validationJob: Job
  constructor(private backend: Backend, readonly id: string, job: Job, private entries: Map<string, SaveSession>) {
    this.validationJob = this.minimalJob(job)
    this.load()
  }
  private minimalJob(job: Job): Job {
    return { segments: job.segments.map(({ speaker_id }) => ({ speaker_id })) } as Job
  }
  updateJob(job: Job) { this.validationJob = this.minimalJob(job) }
  read(): SaveView { return this.view }
  subscribe(listener: Listener): () => void {
    this.listeners.add(listener)
    listener(this.view)
    return () => { this.listeners.delete(listener); this.release() }
  }
  private emit() { for (const listener of this.listeners) listener(this.view); this.release() }
  private release() {
    if (!this.listeners.size && !this.loading && !this.view.saving && !this.view.error
      && (!this.view.state || !this.view.draft || samePreferences(this.view.state.preferences, this.view.draft))) {
      if (this.entries.get(this.id) === this) this.entries.delete(this.id)
    }
  }
  private set(patch: Partial<SaveView>) { this.view = { ...this.view, ...patch }; this.emit() }
  private load() {
    if (this.loading) return
    this.loading = true
    void this.backend.loadPresentation(this.id).then(raw => {
      const state = parsePresentationState(raw, this.validationJob)
      this.blocked = false
      this.set({ state, draft: this.view.draft ?? state.preferences, error: null })
    }).catch(e => { this.blocked = true; this.set({ error: errorText(e) }) })
      .finally(() => { this.loading = false; this.release() })
  }
  change(update: (previous: PresentationPreferences) => PresentationPreferences) {
    if (!this.view.draft || !this.view.state?.writable) return
    try {
      const draft = validatePreferences(update(this.view.draft), this.validationJob)
      this.set({ draft, error: this.blocked ? this.view.error : null })
      queueMicrotask(() => this.flush())
    } catch (e) { this.set({ error: errorText(e) }) }
  }
  retry() {
    if (this.view.state && !this.view.state.writable) return
    this.blocked = false
    this.set({ error: null })
    if (!this.view.state) this.load()
    else this.flush()
  }
  private flush() {
    const { state, draft, saving } = this.view
    if (this.loading || this.blocked || saving || !state?.writable || !draft || samePreferences(state.preferences, draft)) return
    try { validatePreferences(draft, this.validationJob) }
    catch (e) { this.blocked = true; this.set({ error: errorText(e) }); return }
    this.set({ saving: true })
    void this.backend.savePresentation(this.id, draft, state.revision).then(raw => {
      const next = parsePresentationState(raw, this.validationJob)
      if (!acknowledged(next, state.revision, draft)) throw new Error('Accusé de sauvegarde incohérent : révision ou préférences différentes.')
      this.set({ state: next, error: null, saving: false })
      queueMicrotask(() => this.flush())
    }).catch(e => {
      const message = errorText(e)
      this.blocked = true
      if (/conflit|conflict|révision|revision/i.test(message)) {
        void this.backend.loadPresentation(this.id).then(raw => {
          const latest = parsePresentationState(raw, this.validationJob)
          this.set({ state: latest, error: latest.writable ? 'Conflit : dernière révision relue, brouillon conservé. Réessayez explicitement.' : 'Conflit : fichier incompatible en lecture seule, brouillon conservé.' })
        }).catch(loadError => this.set({ error: `Conflit : relecture impossible (${errorText(loadError)}). Brouillon conservé.` }))
          .finally(() => this.set({ saving: false }))
      } else this.set({ saving: false, error: message })
    })
  }
}

export function presentationSession(backend: Backend, job: Job): SaveSession {
  let entries = sessions.get(backend)
  if (!entries) { entries = new Map(); sessions.set(backend, entries) }
  let session = entries.get(job.id)
  if (!session) { session = new SaveSession(backend, job.id, job, entries); entries.set(job.id, session) }
  else session.updateJob(job)
  return session
}
