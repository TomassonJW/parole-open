import { useEffect, useRef, useState } from 'react'
import type { Job } from '../lib/types'
import type { TranscriptMatch } from '../lib/transcriptSearch'

interface SearchState { key: string; matches: TranscriptMatch[]; error: string; pending: boolean }
export function useTranscriptSearch(job: Job, translated: boolean, query: string, approximate: boolean) {
  const worker = useRef<Worker | null>(null)
  const sequence = useRef(0)
  const request = useRef(0)
  const [state, setState] = useState<SearchState>({ key: '', matches: [], error: '', pending: false })
  const key = `${job.id}:${translated}:${query}:${approximate}`
  useEffect(() => {
    const revision = ++sequence.current
    worker.current?.terminate()
    worker.current = null
    if (typeof Worker === 'undefined') {
      setState({ key, matches: [], pending: false, error: 'Recherche indisponible dans cet environnement.' })
      return
    }
    try {
      const instance = new Worker(new URL('../workers/transcriptSearch.worker.ts', import.meta.url), { type: 'module' })
      worker.current = instance
      instance.onmessage = (event: MessageEvent<{ type: string; revision: number; request?: number; matches?: TranscriptMatch[] }>) => {
        const message = event.data
        if (message.revision !== sequence.current || worker.current !== instance) return
        if (message.type === 'error' && (message.request === undefined || message.request === request.current)) {
          setState({ key: keyRef.current, matches: [], pending: false, error: 'La recherche a échoué. Réessayez.' })
        } else if (message.type === 'results' && message.request === request.current) {
          setState({ key: keyRef.current, matches: message.matches ?? [], pending: false, error: '' })
        }
      }
      instance.onerror = () => {
        if (worker.current === instance && revision === sequence.current) setState({ key: keyRef.current, matches: [], pending: false, error: 'La recherche a échoué. Réessayez.' })
      }
      instance.postMessage({ type: 'init', revision, segments: job.segments, translated })
    } catch {
      worker.current?.terminate()
      worker.current = null
      setState({ key, matches: [], pending: false, error: 'Recherche indisponible dans cet environnement.' })
    }
    return () => { worker.current?.terminate(); worker.current = null; ++sequence.current }
  }, [job.id, job.segments, translated])
  const keyRef = useRef(key)
  keyRef.current = key
  useEffect(() => {
    const revision = sequence.current
    const serial = ++request.current
    if (!query.trim() || !worker.current) return
    setState({ key, matches: [], error: '', pending: true })
    const timeout = setTimeout(() => {
      if (revision !== sequence.current) return
      try { worker.current?.postMessage({ type: 'search', revision, request: serial, query, approximate }) }
      catch { setState({ key, matches: [], pending: false, error: 'La recherche a échoué. Réessayez.' }) }
    }, 60)
    return () => clearTimeout(timeout)
  }, [job.id, job.segments, translated, query, approximate])
  return {
    matches: state.key === key ? state.matches : [],
    error: state.error && (state.key === key || !worker.current) ? state.error : '',
    pending: !!query.trim() && state.key !== key || (state.key === key && state.pending),
  }
}
