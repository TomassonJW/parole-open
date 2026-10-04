import { useCallback, useEffect, useRef, useState } from 'react'
import type { Backend } from '../lib/backend'
import type { Job } from '../lib/types'
import { presentationSession, samePreferences, type SaveSession, type SaveView } from '../lib/presentationSaveQueue'
import { defaultsSession, emptyDefaults, type DefaultsSaveSession, type DefaultsView } from '../lib/presentationDefaultsQueue'
import { defaultPreferences, sourceRevision, validateSnapshot, type PresentationPreferences, type PresentationSnapshot } from '../lib/transcriptPresentation'

const errorText = (e: unknown) => e instanceof Error ? e.message : String(e)
const empty: SaveView = { state: null, draft: null, error: null, saving: false }
export type ReturnTypePresentation = ReturnType<typeof usePresentation>
export function usePresentation(backend: Backend | undefined, job: Job | null) {
  const [view, setView] = useState<SaveView>(empty)
  const [snapshot, setSnapshot] = useState<PresentationSnapshot | null>(null)
  const [previewError, setPreviewError] = useState<string | null>(null)
  const [previewing, setPreviewing] = useState(false)
  const [defaultsView, setDefaultsView] = useState<DefaultsView>(emptyDefaults)
  const defaults = useRef<{ backend: Backend; session: DefaultsSaveSession } | null>(null)
  const [attempt, setAttempt] = useState(0)
  const [loadAttempt, setLoadAttempt] = useState(0)
  const [revision, setRevision] = useState<string | null>(null)
  const [sourceSegments, setSourceSegments] = useState<Job['segments'] | null>(null)
  const generation = useRef(0)
  const previewSequence = useRef(0)
  const active = useRef<{ backend: Backend; id: string; session: SaveSession } | null>(null)

  useEffect(() => {
    setDefaultsView(emptyDefaults)
    if (!backend) { defaults.current = null; return }
    const owner = { backend, session: defaultsSession(backend) }
    defaults.current = owner
    const unsubscribe = owner.session.subscribe(setDefaultsView)
    return () => { unsubscribe(); if (defaults.current === owner) defaults.current = null }
  }, [backend])

  useEffect(() => {
    const token = ++generation.current
    previewSequence.current++
    setView(empty); setSnapshot(null); setPreviewError(null); setPreviewing(false)
    active.current = null
    if (!backend || !job) return
    const session = presentationSession(backend, job)
    active.current = { backend, id: job.id, session }
    const unsubscribe = session.subscribe(next => { if (generation.current === token) setView(next) })
    return () => {
      generation.current++
      previewSequence.current++
      unsubscribe()
      if (active.current?.session === session) active.current = null
    }
  }, [backend, job?.id, loadAttempt])

  const options = view.draft?.screen ?? defaultPreferences().screen
  useEffect(() => {
    if (!backend || !job || !options) { setRevision(null); setSourceSegments(null); setSnapshot(null); return }
    active.current?.session.updateJob(job)
    const sequence = ++previewSequence.current
    setRevision(null); setSourceSegments(job.segments); setSnapshot(null); setPreviewError(null); setPreviewing(true)
    // Start the backend request immediately: a slow SHA must not leave a visible
    // loading state with no request to observe (and stale responses stay gated).
    void Promise.all([sourceRevision(job), backend.previewPresentation(job.id, options)]).then(([next, raw]) => {
      if (previewSequence.current !== sequence) return
      setSnapshot(validateSnapshot(raw, job, next, options)); setRevision(next); setPreviewing(false)
    }).catch(e => {
      if (previewSequence.current !== sequence) return
      setSnapshot(null); setPreviewing(false); setPreviewError(errorText(e))
    })
    return () => { previewSequence.current++ }
  }, [backend, job?.id, job?.segments, options?.mode, options?.pause_ms, options?.show_timestamps, attempt])

  const change = useCallback((update: (previous: PresentationPreferences) => PresentationPreferences) => {
    active.current?.session.change(update)
  }, [])
  const retry = useCallback(() => {
    const session = active.current?.session
    if (!session) return
    if (session.read().state && !session.read().state?.writable) return
    if (!session.read().state && !session.read().error) setLoadAttempt(n => n + 1)
    else session.retry()
    setAttempt(n => n + 1)
  }, [])
  const saveDefaults = useCallback(async () => {
    const owner = active.current
    const draft = owner?.session.read().draft
    if (owner && draft && defaults.current?.backend === owner.backend) await defaults.current.session.request(draft)
  }, [])
  const retryDefaults = useCallback(() => { void defaults.current?.session.retry() }, [])
  const defaultsActive = defaults.current?.backend === backend
  const defaultsStatus = !defaultsActive ? null : defaultsView.saving ? 'Enregistrement des options par défaut…'
    : defaultsView.saved ? 'Options enregistrées pour les futurs documents, sans couleurs.' : null
  const snapshotMatches = !!snapshot && !!view.draft && snapshot.options.mode === view.draft.screen.mode && snapshot.options.pause_ms === view.draft.screen.pause_ms && snapshot.options.show_timestamps === view.draft.screen.show_timestamps
  const activeMatches = active.current?.backend === backend && active.current?.id === job?.id
  return { state: activeMatches ? view.state : null, draft: activeMatches ? view.draft ?? defaultPreferences() : defaultPreferences(),
    dirty: activeMatches && !!view.state && !!view.draft && !samePreferences(view.state.preferences, view.draft),
    snapshot: activeMatches && sourceSegments === job?.segments && revision === snapshot?.source_revision && snapshotMatches ? snapshot : null,
    error: activeMatches ? view.error : null, defaultsError: defaultsActive ? defaultsView.error : null,
    previewError, saving: activeMatches && view.saving, previewing, defaultsStatus, change, retry, saveDefaults, retryDefaults }
}
