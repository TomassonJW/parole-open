import { useEffect, useRef, useState } from 'react'
import type { Backend } from '../lib/backend'
import { decodeAudioPacket, playableWav, safeMs, type AudioChunk } from '../lib/audioPacket'

type Request = { id: string; revision?: string; backend: Backend; atMs: number; serial: number }
const FAILURE = 'Extrait audio indisponible ou non vérifiable.'

/** Un seul IPC natif actif ; la dernière intention remplace les intentions en attente. */
export function useAudioChunks(id: string, backend?: Backend, revision?: string) {
  const [source, setSource] = useState<AudioChunk | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const serial = useRef(0)
  const activeId = useRef(id)
  activeId.current = id
  const activeRevision = useRef(revision)
  activeRevision.current = revision
  const activeBackend = useRef(backend)
  activeBackend.current = backend
  const current = useRef<AudioChunk | null>(null)
  const sourceOwner = useRef<{ id: string; revision?: string; backend: Backend } | null>(null)
  const busy = useRef(false)
  const queued = useRef<Request | null>(null)
  const alive = useRef(true)
  const release = () => {
    if (current.current) URL.revokeObjectURL(current.current.src)
    current.current = null
    sourceOwner.current = null
    setSource(null)
  }
  const cancel = () => {
    serial.current++
    queued.current = null
    setLoading(false)
  }
  useEffect(() => {
    alive.current = true
    setError('')
    setLoading(false)
    return () => { alive.current = false; cancel(); release() }
    // The backend/job identity is the lifetime of the native audio owner.
  }, [id, backend, revision])

  async function drain() {
    if (busy.current || !backend) return
    busy.current = true
    try {
      while (queued.current && alive.current) {
        const request = queued.current
        queued.current = null
        try {
          const raw = await request.backend.loadAudioAt(request.id, request.atMs)
          if (!alive.current || activeId.current !== request.id || activeRevision.current !== request.revision || activeBackend.current !== request.backend || serial.current !== request.serial) continue
          const decoded = decodeAudioPacket(raw, request.id, request.atMs)
          const src = URL.createObjectURL(new Blob([playableWav(decoded)], { type: 'audio/wav' }))
          const next: AudioChunk = { ...decoded, src }
          release()
          current.current = next
          sourceOwner.current = { id: request.id, revision: request.revision, backend: request.backend }
          setSource(next)
          setLoading(false)
        } catch {
          if (alive.current && activeId.current === request.id && activeRevision.current === request.revision && activeBackend.current === request.backend && serial.current === request.serial) {
            release(); setLoading(false); setError(FAILURE)
          }
        }
      }
    } finally { busy.current = false }
  }
  function request(atMs: number) {
    if (!backend || !backend.available || !safeMs(atMs)) { setError(FAILURE); return }
    cancel(); release(); setError(''); setLoading(true)
    queued.current = { id, revision, backend, atMs, serial: serial.current }
    void drain()
  }
  return { source: source && sourceOwner.current?.id === id && sourceOwner.current.revision === revision && sourceOwner.current.backend === backend ? source : null, loading, error, setError, request, cancel }
}
