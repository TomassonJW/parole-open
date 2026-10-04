import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { BackendError, type Backend, type JobActivity } from '../lib/backend'
import { baseName } from '../lib/format'
import type { ExportFormat, Job, StartOptions } from '../lib/types'

/** Intervalle de relecture de l'état réel pendant un traitement (aucune progression simulée). */
export const POLL_INTERVAL_MS = 1500

export interface PendingStart {
  path: string
  name: string
}

export interface JobsState {
  jobs: Job[]
  loaded: boolean
  listError: BackendError | null
  pendingStart: PendingStart | null
  resuming: ReadonlySet<string>
  activity: JobActivity | null
}

function upsert(list: Job[], job: Job): Job[] {
  const index = list.findIndex((j) => j.id === job.id)
  if (index === -1) return [job, ...list]
  // La réponse au lancement peut arriver après l'événement « en cours ».
  if (job.stage === 'Ready' && list[index].stage !== 'Ready') return list
  const next = list.slice()
  next[index] = job
  return next
}

function asBackendError(error: unknown): BackendError {
  if (error instanceof BackendError) return error
  return new BackendError(error instanceof Error ? error.message : String(error), 'failed')
}

export function useJobs(backend: Backend) {
  const [jobs, setJobs] = useState<Job[]>([])
  const [loaded, setLoaded] = useState(false)
  const [listError, setListError] = useState<BackendError | null>(null)
  const [pendingStart, setPendingStart] = useState<PendingStart | null>(null)
  const [resuming, setResuming] = useState<ReadonlySet<string>>(new Set())
  const [activity, setActivity] = useState<JobActivity | null>(null)
  const eventSequence = useRef(0)
  const lastEventByJob = useRef(new Map<string, number>())
  const mounted = useRef(true)

  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])

  const refresh = useCallback(async () => {
    try {
      const list = await backend.listJobs()
      if (!mounted.current) return
      setJobs(list)
      setListError(null)
    } catch (error) {
      if (!mounted.current) return
      setListError(asBackendError(error))
    } finally {
      if (mounted.current) setLoaded(true)
    }
  }, [backend])

  useEffect(() => {
    void refresh()
  }, [refresh])

  // Mises à jour poussées par le moteur, si celui-ci les émet.
  useEffect(() => {
    let unlisten: (() => void) | null = null
    let cancelled = false
    backend
      .onJobUpdated((job) => {
        lastEventByJob.current.set(job.id, ++eventSequence.current)
        setJobs((list) => upsert(list, job))
      })
      .then((fn) => {
        if (cancelled) fn()
        else unlisten = fn
      })
      .catch(() => {})
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [backend])

  useEffect(() => {
    if (!backend.onJobActivity) return
    let cancelled = false
    let unlisten: (() => void) | null = null
    backend.onJobActivity((item) => { if (mounted.current) setActivity(item) })
      .then((fn) => { if (cancelled) fn(); else unlisten = fn })
      .catch(() => {})
    return () => { cancelled = true; unlisten?.() }
  }, [backend])

  // Relecture de l'état réel tant qu'un traitement est actif.
  const active =
    pendingStart !== null || resuming.size > 0 || jobs.some((j) => ['Transcribing', 'Translating', 'Reporting'].includes(j.stage))
  const stopPolling = listError?.kind === 'missing-command' || listError?.kind === 'unavailable'
  useEffect(() => {
    if (!active || stopPolling) return
    const timer = window.setInterval(() => void refresh(), POLL_INTERVAL_MS)
    return () => window.clearInterval(timer)
  }, [active, stopPolling, refresh])

  const startJob = useCallback(
    async (options: StartOptions): Promise<Job> => {
      setPendingStart({ path: options.mediaPath, name: baseName(options.mediaPath) })
      const launchSequence = eventSequence.current
      try {
        const job = await backend.startJob(options)
        const active = job.stage === 'Ready' ? { ...job, stage: 'Transcribing' as const } : job
        if (mounted.current) setJobs((list) => (lastEventByJob.current.get(active.id) ?? 0) > launchSequence && list.some((item) => item.id === active.id)
          ? list : upsert(list, active))
        return active
      } catch (error) {
        throw asBackendError(error)
      } finally {
        if (mounted.current) setPendingStart(null)
      }
    },
    [backend],
  )

  const resumeJob = useCallback(
    async (id: string): Promise<Job> => {
      setResuming((set) => new Set(set).add(id))
      const launchSequence = eventSequence.current
      try {
        const job = await backend.resumeJob(id)
        const active = job.stage === 'Interrupted' ? { ...job, stage: 'Transcribing' as const, error: null } : job
        if (mounted.current) setJobs((list) => (lastEventByJob.current.get(id) ?? 0) > launchSequence && list.some((item) => item.id === id)
          ? list : upsert(list, active))
        return active
      } catch (error) {
        void refresh()
        throw asBackendError(error)
      } finally {
        if (mounted.current)
          setResuming((set) => {
            const next = new Set(set)
            next.delete(id)
            return next
          })
      }
    },
    [backend, refresh],
  )

  const saveSpeakerNames = useCallback(
    async (id: string, names: Record<string, string>): Promise<Job> => {
      try {
        const job = await backend.saveSpeakerNames(id, names)
        if (mounted.current) setJobs((list) => upsert(list, job))
        return job
      } catch (error) {
        throw asBackendError(error)
      }
    },
    [backend],
  )

  const exportJob = useCallback(
    async (job: Job, format: ExportFormat, defaultName: string): Promise<string | null> => {
      try {
        return await backend.exportJob(job.id, format, defaultName)
      } catch (error) {
        throw asBackendError(error)
      }
    },
    [backend],
  )

  const state: JobsState = useMemo(
    () => ({ jobs, loaded, listError, pendingStart, resuming, activity }),
    [jobs, loaded, listError, pendingStart, resuming, activity],
  )

  return { state, refresh, startJob, resumeJob, saveSpeakerNames, exportJob }
}
