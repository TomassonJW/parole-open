import { useCallback, useEffect, useRef, useState } from 'react'
import { BackendError, type Backend } from '../lib/backend'
import type { ModelStatus } from '../lib/types'

export type ModelsState =
  | { kind: 'checking' }
  | { kind: 'unavailable' }
  | { kind: 'unknown'; error: BackendError }
  | { kind: 'ready'; status: ModelStatus }

function asBackendError(error: unknown): BackendError {
  if (error instanceof BackendError) return error
  return new BackendError(error instanceof Error ? error.message : String(error), 'failed')
}

export function useModels(backend: Backend) {
  const [state, setState] = useState<ModelsState>(() => (backend.available ? { kind: 'checking' } : { kind: 'unavailable' }))
  const [preparing, setPreparing] = useState(false)
  const [progress, setProgress] = useState<{ received: number; total: number; model?: string } | null>(null)
  const [prepareError, setPrepareError] = useState<BackendError | null>(null)
  const mounted = useRef(true)

  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])

  const check = useCallback(async () => {
    if (!backend.available) {
      setState({ kind: 'unavailable' })
      return
    }
    setState((s) => (s.kind === 'ready' ? s : { kind: 'checking' }))
    try {
      const status = await backend.modelStatus()
      if (mounted.current) setState({ kind: 'ready', status })
    } catch (error) {
      if (mounted.current) setState({ kind: 'unknown', error: asBackendError(error) })
    }
  }, [backend])

  useEffect(() => {
    void check()
  }, [check])

  /** Téléchargement déclenché uniquement par une action explicite de l'utilisateur. */
  const prepare = useCallback(async () => {
    setPreparing(true)
    setProgress(null)
    setPrepareError(null)
    let unlisten: (() => void) | undefined
    try {
      unlisten = await backend.onModelProgress?.((value) => { if (mounted.current) setProgress(value) })
      const status = await backend.prepareModels()
      if (!mounted.current) return
      if (status) setState({ kind: 'ready', status })
      else await check()
    } catch (error) {
      if (mounted.current) {
        setPrepareError(asBackendError(error))
        void check()
      }
    } finally {
      unlisten?.()
      if (mounted.current) setPreparing(false)
    }
  }, [backend, check])

  const install = useCallback(async (id: string) => {
    setPreparing(true)
    setProgress(null)
    setPrepareError(null)
    let unlisten: (() => void) | undefined
    try {
      unlisten = await backend.onModelProgress?.((value) => { if (mounted.current) setProgress(value) })
      const status = await backend.installReportModel(id)
      if (mounted.current) setState({ kind: 'ready', status })
    } catch (error) {
      if (mounted.current) setPrepareError(asBackendError(error))
    } finally {
      unlisten?.()
      if (mounted.current) setPreparing(false)
    }
  }, [backend])

  return { state, preparing, progress, prepareError, prepare, install, check }
}
