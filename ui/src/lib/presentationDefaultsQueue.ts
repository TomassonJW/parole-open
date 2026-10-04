import type { Backend } from './backend'
import { acknowledged } from './presentationSaveQueue'
import { parsePresentationState, validatePreferences, type PresentationPreferences } from './transcriptPresentation'

export type DefaultsView = { saving: boolean; saved: boolean; error: string | null }
export const emptyDefaults: DefaultsView = { saving: false, saved: false, error: null }
const sessions = new WeakMap<Backend, DefaultsSaveSession>()

// Global intent belongs to the backend session, not to a document/component.
// Only a copied preference set is retained, never a job, media or transcript.
export class DefaultsSaveSession {
  private listeners = new Set<(view: DefaultsView) => void>()
  private view: DefaultsView = emptyDefaults
  private desired: PresentationPreferences | null = null
  private running = false
  private blocked = false
  constructor(private backend: Backend) {}
  subscribe(listener: (view: DefaultsView) => void) {
    this.listeners.add(listener); listener(this.view)
    return () => { this.listeners.delete(listener) }
  }
  private set(view: DefaultsView) {
    this.view = view
    for (const listener of this.listeners) listener(view)
  }
  request(draft: PresentationPreferences) {
    try {
      const safe = validatePreferences({ ...draft, speaker_colors: {} }, undefined, true)
      this.desired = { ...safe, screen: { ...safe.screen }, export: { ...safe.export, view: { ...safe.export.view } }, speaker_colors: {} }
      this.blocked = false
      this.set({ saving: this.running, saved: false, error: null })
      return this.flush()
    } catch (e) {
      this.set({ saving: this.running, saved: false, error: e instanceof Error ? e.message : String(e) })
    }
  }
  retry() {
    if (!this.desired || this.running) return
    this.blocked = false
    return this.flush()
  }
  private async flush(): Promise<void> {
    if (this.running || this.blocked || !this.desired) return
    const requested = this.desired
    this.running = true
    this.set({ saving: true, saved: false, error: null })
    try {
      const loaded = parsePresentationState(await this.backend.loadPresentationDefaults(), undefined, true)
      if (!loaded.writable) throw new Error(loaded.warning ?? 'Réglages par défaut non modifiables.')
      const response = parsePresentationState(await this.backend.savePresentationDefaults(requested, loaded.revision), undefined, true)
      if (!acknowledged(response, loaded.revision, requested)) throw new Error('Accusé des défauts incohérent : révision ou préférences différentes.')
      if (this.desired === requested) this.desired = null
      this.set({ saving: false, saved: !this.desired, error: null })
    } catch (e) {
      // No silent retry/rebase: retain the latest intent for explicit retry.
      this.blocked = true
      this.set({ saving: false, saved: false, error: e instanceof Error ? e.message : String(e) })
    } finally {
      this.running = false
      if (!this.blocked && this.desired) void this.flush()
    }
  }
}

export function defaultsSession(backend: Backend): DefaultsSaveSession {
  let session = sessions.get(backend)
  if (!session) { session = new DefaultsSaveSession(backend); sessions.set(backend, session) }
  return session
}
