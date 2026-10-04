import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { formatClock } from '../lib/format'
import { sourceSeconds, transcriptMs, type PlaybackSource } from '../lib/playbackTime'
import { useTranscriptSearch } from '../hooks/useTranscriptSearch'
import { useAudioChunks } from '../hooks/useAudioChunks'
import type { Backend } from '../lib/backend'
import { speakerLabel } from '../lib/format'
import type { Job } from '../lib/types'
import { validTranscriptRequest, type TranscriptNavigation } from '../lib/transcriptNavigation'
import { HighlightedText, type HighlightRange } from './HighlightedText'
import '../transcriptPresentation.css'

const PAGE = 100
export type TranscriptPresentation = {
  options: { mode: 'fluid' | 'detailed'; pause_ms: number; show_timestamps: boolean }
  blocks: { segment_indices: number[]; start_ms: number; end_ms: number; speaker_id: string | null }[]
  speakerColors: Record<string, string>
}
export function TranscriptExplorer({ job, translated, playbackSource: providedSource, backend, navigation, contextBrowsing = false, presentation }: { job: Job; translated: boolean; playbackSource?: PlaybackSource | null; backend?: Backend; navigation?: TranscriptNavigation; contextBrowsing?: boolean; presentation?: TranscriptPresentation }) {
  const revision = navigation?.scope.jobId === job.id ? navigation.scope.revision : undefined
  const chunks = useAudioChunks(job.id, backend, revision)
  const playbackSource = backend ? chunks.source : providedSource
  const canListen = !!(backend?.available || playbackSource)
  const intent = useRef(false)
  const boundaryTriggered = useRef(false)
  const [query, setQuery] = useState('')
  const [approximate, setApproximate] = useState(true)
  const [selected, setSelected] = useState(0)
  const [revealRequest, setRevealRequest] = useState(0)
  const [requestedPage, setPage] = useState(0)
  const [externalTarget, setExternalTarget] = useState<number | null>(null)
  const [externalFocus, setExternalFocus] = useState(0)
  const lastRequest = useRef(0)
  const [clock, setClock] = useState<number | null>(null)
  const [playing, setPlaying] = useState(false)
  const [follow, setFollow] = useState(false)
  const [followPaused, setFollowPaused] = useState(false)
  const [followIndex, setFollowIndex] = useState<number | null>(null)
  const [onlyResults, setOnlyResults] = useState(false)
  const [speed, setSpeed] = useState(1)
  const [error, setError] = useState('')
  const audio = useRef<HTMLAudioElement>(null)
  const playbackActive = useRef(false)
  const input = useRef<HTMLInputElement>(null)
  const followControl = useRef<HTMLInputElement>(null)
  const root = useRef<HTMLDivElement>(null)
  const tools = useRef<HTMLDivElement>(null)
  const pendingSeek = useRef<number | null>(null)
  const sourceGeneration = useRef(0)
  const id = useId()
  const { matches, error: searchError, pending } = useTranscriptSearch(job, translated, query, approximate)
  const highlights = useMemo(() => {
    const bySegment = new Map<number, HighlightRange[]>()
    matches.forEach((match, matchIndex) => match.spans.forEach(span => {
      const list = bySegment.get(span.segmentIndex) ?? []
      list.push({ start: span.start, end: span.end, active: matchIndex === Math.min(selected, matches.length - 1), approximate: match.kind === 'approx' })
      bySegment.set(span.segmentIndex, list)
    }))
    return bySegment
  }, [matches, selected])
  const allIndices = useMemo(() => job.segments.map((_, index) => index), [job.segments])
  const matchingIndices = useMemo(() => [...new Set(matches.flatMap(match => match.spans.map(span => span.segmentIndex)))].sort((a, b) => a - b), [matches])
  const visibleIndices = useMemo(() => {
    if (!onlyResults || !query.trim()) return allIndices
    if (!presentation) return externalTarget !== null && externalTarget >= 0 && externalTarget < job.segments.length
      ? [...new Set([...matchingIndices, externalTarget])].sort((a, b) => a - b)
      : matchingIndices
    const selectedIndices = new Set(matchingIndices)
    if (externalTarget !== null && externalTarget >= 0 && externalTarget < job.segments.length) selectedIndices.add(externalTarget)
    return presentation.blocks.flatMap(block => block.segment_indices.some(index => selectedIndices.has(index)) ? block.segment_indices : [])
  }, [onlyResults, query, matchingIndices, allIndices, externalTarget, job.segments.length, presentation])
  const positions = useMemo(() => new Map(visibleIndices.map((index, position) => [index, position])), [visibleIndices])
  const count = Math.ceil(visibleIndices.length / PAGE)
  const page = Math.min(requestedPage, Math.max(0, count - 1))
  const pageIndices = visibleIndices.slice(page * PAGE, (page + 1) * PAGE)
  const pageSet = new Set(pageIndices)
  const current = matches[Math.min(selected, matches.length - 1)]
  function reveal(element: Element | null | undefined) {
    element?.scrollIntoView?.({ block: 'center', behavior: 'instant' })
    const target = element?.getBoundingClientRect()
    const toolbar = tools.current?.getBoundingClientRect()
    if (target && toolbar && target.height > 0 && toolbar.height > 0) {
      const floor = toolbar.bottom + 12
      const passage = element?.closest('.segment')?.getBoundingClientRect()
      const top = passage && target.bottom - passage.top <= window.innerHeight - floor ? passage.top : target.top
      if (top < floor) window.scrollBy({ top: top - floor, behavior: 'instant' })
    }
  }
  useEffect(() => { if (selected >= matches.length && matches.length) setSelected(matches.length - 1) }, [matches, selected])
  useEffect(() => {
    if (!current || externalTarget !== null) return
    const element = root.current?.querySelector(`.segment[data-segment-index="${current.spans[0].segmentIndex}"] mark.transcript-mark--current`)
    reveal(element)
  }, [current, page, revealRequest, externalTarget])
  useEffect(() => {
    if (!follow || followPaused || followIndex === null) return
    reveal(root.current?.querySelector(`.segment[data-segment-index="${followIndex}"]`))
  }, [follow, followPaused, followIndex, page])
  function select(index: number) {
    if (!matches.length) return
    if (follow) setFollowPaused(true)
    const next = (index + matches.length) % matches.length
    setExternalTarget(null)
    setSelected(next)
    setRevealRequest(value => value + 1)
    const target = matches[next].spans[0].segmentIndex
    setPage(Math.floor((positions.get(target) ?? 0) / PAGE))
  }
  function changeQuery(value: string) {
    if (follow) setFollowPaused(true)
    setExternalTarget(null)
    setQuery(value); setSelected(0)
  }
  // New results and filter changes may reveal a match, except when an explicit
  // return to playback has just chosen the audio page in the same render.
  useEffect(() => { if (externalTarget === null && current && !(follow && !followPaused)) setPage(Math.floor((positions.get(current.spans[0].segmentIndex) ?? 0) / PAGE)) }, [matches, positions, externalTarget])
  useEffect(() => {
    function shortcut(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') {
        event.preventDefault(); input.current?.focus(); input.current?.select()
      }
    }
    document.addEventListener('keydown', shortcut)
    return () => document.removeEventListener('keydown', shortcut)
  }, [])
  useEffect(() => { setPage(0); setQuery(''); setSelected(0); setFollowIndex(null); setFollowPaused(false); setExternalTarget(null) }, [job.id, translated, revision])
  useEffect(() => { if (contextBrowsing) setFollowPaused(true) }, [contextBrowsing, job.id, translated, revision])
  useEffect(() => {
    const candidate = navigation?.request
    if (!candidate || !Number.isSafeInteger(candidate.requestId) || candidate.requestId <= lastRequest.current) return
    lastRequest.current = candidate.requestId // Une requête rejetée ne redevient pas valide après coup.
    if (!navigation || !validTranscriptRequest(navigation, job.id, job.segments.length)) return
    const index = candidate.index
    setExternalTarget(index)
    setPage(Math.floor((positions.get(index) ?? index) / PAGE))
    if (follow) setFollowPaused(true)
    setExternalFocus(n => n + 1)
    if (candidate.action === 'listen') listen(index)
  // Intention identifiée par sa séquence, pas par l'identité de l'objet prop.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [navigation?.request?.requestId, navigation?.scope.jobId, navigation?.scope.revision, job.id, job.segments])
  useLayoutEffect(() => {
    if (externalTarget === null) return
    const position = positions.get(externalTarget)
    if (position !== undefined) setPage(Math.floor(position / PAGE))
  }, [externalTarget, positions])
  useLayoutEffect(() => {
    if (!externalFocus || externalTarget === null) return
    const element = root.current?.querySelector<HTMLElement>(`.segment[data-segment-index="${externalTarget}"]`)
    if (!element) return
    element.focus({ preventScroll: true })
    reveal(element)
  }, [externalFocus, externalTarget, page])
  useLayoutEffect(() => {
    const element = audio.current
    sourceGeneration.current++
    if (element && (playbackActive.current || !element.paused)) element.pause()
    playbackActive.current = false
    boundaryTriggered.current = false
    pendingSeek.current = null
    if (element) {
      const at = backend && chunks.source ? chunks.source.requestedAtMs - chunks.source.timelineOffsetMs : 0
      if (element.readyState === 0) pendingSeek.current = at / 1000
      else element.currentTime = at / 1000
    }
    setClock(backend && chunks.source ? chunks.source.requestedAtMs : null)
    setPlaying(false); setError('')
    return () => { sourceGeneration.current++; if (element && (playbackActive.current || !element.paused)) element.pause(); playbackActive.current = false; pendingSeek.current = null }
  }, [job.id, revision, playbackSource?.src, playbackSource?.timelineOffsetMs, playbackSource?.durationMs])
  useEffect(() => { if (audio.current) audio.current.playbackRate = speed }, [job.id, playbackSource?.src, speed])
  async function play() {
    const element = audio.current
    if (!element) { if (backend?.available && !chunks.loading) { intent.current = true; chunks.request(clock ?? 0) } return }
    setError('')
    const generation = sourceGeneration.current
    try { await element.play() } catch { if (generation === sourceGeneration.current) { setPlaying(false); setError('Lecture impossible pour ce média local.') } }
  }
  function pause() {
    intent.current = false
    chunks.cancel()
    audio.current?.pause()
    setPlaying(false)
  }
  function request(ms: number, autoplay = true) {
    if (!backend) return
    audio.current?.pause()
    intent.current = autoplay
    chunks.request(ms)
  }
  function seek(ms: number) {
    if (backend) ms = Math.max(0, Math.floor(ms))
    const element = audio.current
    if (backend && (!playbackSource || ms < playbackSource.timelineOffsetMs || ms >= (chunks.source?.playableEndMs ?? 0))) {
      request(ms, playing); return
    }
    if (!element || !playbackSource) return
    const seconds = sourceSeconds(ms, playbackSource)
    if (element.readyState === 0) pendingSeek.current = seconds
    else element.currentTime = seconds
    setClock(transcriptMs(seconds, playbackSource))
  }
  function listen(index: number) {
    if (!canListen) return
    const at = Math.max(0, job.segments[index].start_ms - 2000)
    if (backend) { request(at); return }
    seek(at); void play()
  }
  function nextChunk() {
    const chunk = chunks.source
    if (!backend || !chunk || boundaryTriggered.current) return
    boundaryTriggered.current = true
    audio.current?.pause()
    if (chunk.nextMs === null) {
      intent.current = false
      playbackActive.current = false
      setPlaying(false)
      return
    }
    if (chunk.playableEndMs !== chunk.nextMs) {
      setPlaying(false); setError('Interruption entre les extraits audio vérifiés. Choisissez un passage pour reprendre.'); return
    }
    request(chunk.nextMs)
  }
  function resumeFollowing() {
    setExternalTarget(null)
    followControl.current?.focus({ preventScroll: true })
    const ms = audio.current && playbackSource ? transcriptMs(audio.current.currentTime, playbackSource) : clock
    const index = job.segments.findIndex(s => ms !== null && s.start_ms <= ms && ms < s.end_ms)
    setFollowIndex(index === -1 ? null : index)
    if (index !== -1) setPage(Math.floor(index / PAGE))
    setOnlyResults(false)
    setFollowPaused(false)
  }
  function updateClock() {
    if (audio.current && playbackSource) {
      const ms = transcriptMs(audio.current.currentTime, playbackSource)
      setClock(ms)
      if (follow && !followPaused) {
        const index = job.segments.findIndex(s => s.start_ms <= ms && ms < s.end_ms)
        if (index !== -1 && index !== followIndex) { setFollowIndex(index); setPage(Math.floor(index / PAGE)) }
      }
    }
  }
  return (
    <div ref={root} className="explorer" onKeyDownCapture={e => { if (follow && ['PageDown', 'PageUp', 'ArrowDown', 'ArrowUp', 'Home', 'End', ' '].includes(e.key) && e.target instanceof HTMLElement && !['INPUT', 'SELECT', 'TEXTAREA'].includes(e.target.tagName)) setFollowPaused(true) }}>
      <div ref={tools} className="explorer__tools">
      <div className="explorer__search">
        <label htmlFor={`${id}-search`}>Rechercher dans la transcription</label>
        <div className="explorer__search-row">
          <input ref={input} id={`${id}-search`} className="input" type="search" value={query} onChange={e => changeQuery(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); select(selected + (e.shiftKey ? -1 : 1)) } }} placeholder="Mot ou expression…" />
          <button className="button button--secondary" type="button" onClick={() => select(selected - 1)} disabled={!matches.length} aria-label="Occurrence précédente">↑</button>
          <button className="button button--secondary" type="button" onClick={() => select(selected + 1)} disabled={!matches.length} aria-label="Occurrence suivante">↓</button>
          <span className="explorer__count" role="status">{query.trim() ? `${matches.length ? Math.min(selected + 1, matches.length) : 0} / ${matches.length}` : '0 / 0'}</span>
        </div>
        <label className="explorer__option"><input type="checkbox" checked={approximate} onChange={e => { if (follow) setFollowPaused(true); setApproximate(e.target.checked); setSelected(0) }} /> Tolérer les fautes</label>
        <label className="explorer__option"><input type="checkbox" checked={onlyResults} onChange={e => { setOnlyResults(e.target.checked); if (e.target.checked && follow) setFollowPaused(true) }} /> {presentation?.options.mode === 'fluid' ? 'Blocs contenant un résultat' : 'N’afficher que les résultats'}</label>
        {query.trim() && pending && <p className="muted">Recherche en cours…</p>}
        {searchError && <p role="alert" className="status status--error">{searchError}</p>}
        {query.trim() && !pending && !searchError && matches.length === 0 && <p className="muted">Aucune occurrence pour « {query.trim()} ».</p>}
        <div className="explorer__feedback">
        {current && <p className="small muted">{current.kind === 'exact' ? 'Correspondance exacte' : 'Correspondance approchante'}</p>}
        <details className="explorer__help small muted"><summary>Aide à la recherche</summary><p>Ctrl/Cmd+F pour rechercher, Entrée pour l’occurrence suivante, Maj+Entrée pour la précédente. Fautes tolérées : 32 mots et 64 caractères par mot maximum ; recherche exacte sans cette limite.</p></details>
        </div>
      </div>
      <div className={`explorer__audio${canListen ? '' : ' explorer__audio--unavailable'}`} aria-label="Lecteur audio local">
        {canListen ? <>
          {playbackSource && <audio key={playbackSource.src} ref={audio} src={playbackSource.src} preload="metadata" onLoadedMetadata={event => { if (audio.current !== event.currentTarget || event.currentTarget.getAttribute('src') !== playbackSource.src) return; if (pendingSeek.current !== null) { event.currentTarget.currentTime = pendingSeek.current; pendingSeek.current = null; updateClock() } if (backend && intent.current) { intent.current = false; void play() } }} onTimeUpdate={event => { if (audio.current !== event.currentTarget || event.currentTarget.getAttribute('src') !== playbackSource.src) return; updateClock(); if (backend && chunks.source && transcriptMs(event.currentTarget.currentTime, playbackSource) >= chunks.source.playableEndMs) nextChunk() }} onPlay={event => { if (audio.current !== event.currentTarget || event.currentTarget.getAttribute('src') !== playbackSource.src) return; playbackActive.current = true; setPlaying(true) }} onPause={event => { if (audio.current !== event.currentTarget || event.currentTarget.getAttribute('src') !== playbackSource.src) return; playbackActive.current = false; setPlaying(false) }} onEnded={event => { if (audio.current !== event.currentTarget || event.currentTarget.getAttribute('src') !== playbackSource.src) return; playbackActive.current = false; setPlaying(false); nextChunk() }} onError={event => { if (audio.current !== event.currentTarget || event.currentTarget.getAttribute('src') !== playbackSource.src) return; playbackActive.current = false; setPlaying(false); setError('Média local indisponible.') }} />}
          <div className="explorer__audio-controls">
            <button type="button" className="button button--secondary" onClick={() => { if (playing || chunks.loading) pause(); else void play() }}>{playing || chunks.loading ? 'Pause' : 'Lire'}</button>
            <button type="button" className="button button--secondary" disabled={!playbackSource} onClick={() => seek((clock ?? playbackSource?.timelineOffsetMs ?? 0) - 10000)} aria-label="Reculer de 10 secondes">−10 s</button>
            <button type="button" className="button button--secondary" disabled={!playbackSource} onClick={() => seek((clock ?? playbackSource?.timelineOffsetMs ?? 0) + 10000)} aria-label="Avancer de 10 secondes">+10 s</button>
            <label>Vitesse <select aria-label="Vitesse de lecture" value={speed} onChange={e => { const value = Number(e.target.value); setSpeed(value); if (audio.current) audio.current.playbackRate = value }}><option value="0.75">0,75×</option><option value="1">1×</option><option value="1.25">1,25×</option><option value="1.5">1,5×</option><option value="2">2×</option></select></label>
            <label><input ref={followControl} type="checkbox" checked={follow} onChange={e => { setFollow(e.target.checked); if (e.target.checked) resumeFollowing(); else setFollowPaused(false) }} /> Suivre la lecture</label>
            {follow && followPaused && <button type="button" className="button button--secondary" onClick={resumeFollowing}>Revenir à la lecture</button>}
            {translated && <span className="small muted">Audio original</span>}
            <span className="explorer__clock">{formatClock(clock ?? playbackSource?.timelineOffsetMs ?? 0, true)}</span>
          </div>
          {chunks.loading && <p role="status" className="small muted">Chargement de l’extrait vérifié…</p>}
          {(error || chunks.error) && <p role="alert" className="status status--error">{chunks.error || error}</p>}
        </> : <p className="muted">Lecture audio pas encore disponible.</p>}
      </div>
        {count > 1 && <nav className="explorer__pages" aria-label="Pages de transcription"><button type="button" className="button button--secondary" disabled={page === 0} onClick={() => { setFollowPaused(true); setPage(page - 1) }}>Page précédente</button><span>{page * PAGE + 1}–{Math.min((page + 1) * PAGE, visibleIndices.length)} sur {visibleIndices.length}</span><button type="button" className="button button--secondary" disabled={page >= count - 1} onClick={() => { setFollowPaused(true); setPage(page + 1) }}>Page suivante</button></nav>}
      </div>
      {job.segments.length === 0 ? <p className="empty">Le moteur n'a renvoyé aucun passage pour cet enregistrement.</p> : <>
        <ol className="transcript" start={presentation ? undefined : page * PAGE + 1} onWheelCapture={() => { if (follow) setFollowPaused(true) }} onTouchMoveCapture={() => { if (follow) setFollowPaused(true) }}>
          {presentation ? presentation.blocks.map((block, blockIndex) => {
            const indices = block.segment_indices.filter(index => pageSet.has(index))
            if (!indices.length) return null
            const first = indices[0]
            const continued = first !== block.segment_indices[0]
            const speaker = job.segments[first]
            const color = block.speaker_id ? presentation.speakerColors[block.speaker_id] : undefined
            return <li key={blockIndex} className="transcript-block">
              <div className="transcript-block__meta">
                {presentation.options.show_timestamps && <time className="segment__time">{formatClock(block.start_ms, job.duration_ms >= 3600000)}</time>}
                <span className={`segment__speaker${block.speaker_id ? '' : ' segment__speaker--none'}`}><span className="transcript-block__swatch" style={color && /^#[0-9a-fA-F]{6}$/.test(color) ? { backgroundColor: color } : undefined} aria-hidden="true" />{speakerLabel(job, speaker)}</span>
                {continued && <span className="transcript-block__continuation">Suite du bloc</span>}
                <button type="button" className="button button--secondary segment__listen" disabled={!canListen} onClick={() => listen(block.segment_indices[0])} aria-label={`Écouter le bloc ${blockIndex + 1}`}>Écouter</button>
              </div>
              <p className="transcript-block__passages">{indices.map(index => {
                const segment = job.segments[index]
                const text = translated ? segment.translated_text ?? '[traduction manquante]' : segment.text
                const ranges: HighlightRange[] = translated && segment.translated_text === null ? [] : highlights.get(index) ?? []
                const active = clock !== null && segment.start_ms <= clock && clock < segment.end_ms
                return <span key={index} tabIndex={-1} data-segment-index={index} className={`segment${active ? ' segment--playing' : ''}`} aria-current={active ? 'true' : undefined}>
                  <span className="segment__text"><HighlightedText text={text} ranges={ranges} /></span>
                  <button type="button" className="button button--secondary transcript-block__listen-passage" disabled={!canListen} onClick={() => listen(index)} aria-label={`Écouter ce passage ${index + 1}`} title={`Écouter ce passage ${index + 1}`} />{' '}
                </span>
              })}</p>
            </li>
          }) : pageIndices.map(index => {
            const segment = job.segments[index]
            const text = translated ? segment.translated_text ?? '[traduction manquante]' : segment.text
            const ranges: HighlightRange[] = translated && segment.translated_text === null ? [] : highlights.get(index) ?? []
            const active = clock !== null && segment.start_ms <= clock && clock < segment.end_ms
            return <li key={index} tabIndex={-1} data-segment-index={index} className={`segment${active ? ' segment--playing' : ''}`} aria-current={active ? 'true' : undefined}>
              <div className="segment__meta"><time className="segment__time">{formatClock(segment.start_ms, job.duration_ms >= 3600000)}</time><span className={`segment__speaker${segment.speaker_id ? '' : ' segment__speaker--none'}`}>{speakerLabel(job, segment)}</span><button type="button" className="button button--secondary segment__listen" disabled={!canListen} onClick={() => listen(index)} aria-label={`Écouter ce passage ${index + 1}`}>Écouter</button></div>
              <p className="segment__text"><HighlightedText text={text} ranges={ranges} /></p>
            </li>
          })}
        </ol>
        {count > 1 && <nav className="explorer__pages" aria-label="Fin de page"><button type="button" className="button button--secondary" disabled={page >= count - 1} onClick={() => { setFollowPaused(true); setPage(page + 1) }}>Page suivante</button></nav>}
      </>}
    </div>
  )
}
