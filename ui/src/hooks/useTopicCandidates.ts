import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { Backend } from '../lib/backend'
import type { Job } from '../lib/types'
import { topicSource, verifyTopicSnapshot, type VerifiedTopicSnapshot } from '../lib/topicSnapshot'

export type TopicStatus = 'inactive' | 'loading' | 'absent' | 'ready' | 'preparing' | 'stale' | 'error' | 'unavailable'
type Scope = { backend: Backend; id: string; source: string; job: Pick<Job, 'id' | 'segments'> }
type State = { scope: Scope | null; status: TopicStatus; snapshot: VerifiedTopicSnapshot | null; error: string | null }
const empty: State = { scope: null, status: 'inactive', snapshot: null, error: null }
function errorState(error: unknown): Pick<State, 'status' | 'error'> {
  const message = error instanceof Error ? error.message : 'Pistes sauvegardées indisponibles.'
  return { status: /cryptograph.*indisponible/i.test(message) ? 'unavailable' : /non vérifiables|illisible/i.test(message) ? 'stale' : 'error', error: message }
}

/** Contrôleur non branché à la vue ; seul prepare() peut écrire le cache natif. */
export function useTopicCandidates(backend: Backend, job: Job | null, enabled: boolean) {
  let source: string | null = null
  let sourceError: unknown = null
  if (enabled && job) {
    try { source = topicSource(job) } catch (e) { sourceError = e }
  }
  const active = enabled && job && backend.available && source !== null
  // Le rendu reste pur : une tentative suspendue ne modifie jamais la portée engagée.
  const scope = useMemo<Scope | null>(() => active && job && source !== null
    ? { backend, id: job.id, source, job: { id: job.id, segments: job.segments.map(s => ({ ...s })) } }
    : null, [active, backend, source])
  const scopeRef = useRef<Scope | null>(null)
  const [state, setState] = useState<State>(empty)
  const queue = useRef<Promise<void>>(Promise.resolve())
  const generation = useRef(0)
  const pending = useRef(false)
  const mounted = useRef(false)
  // Masquer les anciennes données dès le rendu, sans changer les commandes engagées.
  const effective = state.scope === scope ? state : { ...empty, status: active ? 'loading' as const : !enabled ? 'inactive' as const : !backend.available ? 'unavailable' as const : sourceError ? 'stale' as const : 'inactive' as const, error: sourceError ? errorState(sourceError).error : null }
  useLayoutEffect(() => {
    mounted.current = true
    return () => { mounted.current = false; generation.current++ }
  }, [])
  useLayoutEffect(() => {
    scopeRef.current = scope
    generation.current++
    pending.current = false
    if (!scope) { setState({ ...empty, status: !enabled ? 'inactive' : !backend.available ? 'unavailable' : sourceError ? 'stale' : 'inactive', error: sourceError ? errorState(sourceError).error : null }); return }
    const token = generation.current
    setState({ scope, status: 'loading', snapshot: null, error: null })
    const run = async () => {
      if (scopeRef.current !== scope || generation.current !== token || !mounted.current) return
      try {
        const raw = await scope.backend.loadTopicCandidates(scope.id)
        const checked = await verifyTopicSnapshot(raw, scope.job)
        if (scopeRef.current !== scope || generation.current !== token || !mounted.current) return
        setState({ scope, status: checked.candidates === null ? 'absent' : 'ready', snapshot: checked, error: null })
      } catch (error) {
        if (scopeRef.current === scope && generation.current === token && mounted.current)
          setState({ scope, snapshot: null, ...errorState(error) })
      }
    }
    queue.current = queue.current.then(run, run)
    // La file sérialise aussi après rejet, sans réessai automatique.
    return () => {
      if (scopeRef.current === scope) scopeRef.current = null
      generation.current++
    }
  }, [scope, enabled, backend, sourceError !== null])

  const prepare = useCallback(async () => {
    const current = scope
    if (!current || scopeRef.current !== current || pending.current || !mounted.current || !enabled) return
    pending.current = true
    const token = ++generation.current
    setState({ scope: current, status: 'preparing', snapshot: null, error: null })
    const run = async () => {
      if (scopeRef.current !== current || generation.current !== token || !mounted.current) return
      try {
        const raw = await current.backend.prepareTopicCandidates(current.id)
        const checked = await verifyTopicSnapshot(raw, current.job)
        if (scopeRef.current !== current || generation.current !== token || !mounted.current) return
        if (checked.candidates === null) throw new Error('Préparation des pistes non confirmée.')
        setState({ scope: current, status: 'ready', snapshot: checked, error: null })
      } catch (error) {
        if (scopeRef.current === current && generation.current === token && mounted.current)
          setState({ scope: current, snapshot: null, ...errorState(error) })
      } finally {
        if (scopeRef.current === current && generation.current === token) pending.current = false
      }
    }
    queue.current = queue.current.then(run, run)
    await queue.current
  }, [scope, enabled])
  return { ...effective, prepare }
}
