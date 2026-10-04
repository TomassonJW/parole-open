import { useId, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { formatClock } from '../lib/format'
import type { Segment } from '../lib/types'
import { inspectCandidates, type Passage, type TopicCandidates, type TopicEntry } from '../lib/topicCandidates'
import '../topicExplorer.css'

const CATALOG_PAGE = 50
const PASSAGE_PAGE = 100
export interface TopicExplorerProps {
  workScope: string
  candidateScope: string
  sourceRevision: string
  candidateRevision: string
  segments: readonly Segment[]
  speakerNames: Readonly<Record<string, string>>
  candidates?: TopicCandidates | null
  onOpen?: (segmentIndexOriginal: number) => void
  onListen?: (segmentIndexOriginal: number) => void
  currentSegmentIndex?: number | null
}
function PassageText({ text, spans }: { text: string; spans: Passage['spans'] }) {
  const ordered = [...spans].sort((a, b) => a.start - b.start || a.end - b.end)
  const parts: React.ReactNode[] = []
  let cursor = 0
  for (const span of ordered) {
    if (span.start < cursor) continue
    parts.push(text.slice(cursor, span.start))
    parts.push(<mark key={`${span.start}-${span.end}`} className="topic-explorer__mark">{text.slice(span.start, span.end)}</mark>)
    cursor = span.end
  }
  parts.push(text.slice(cursor))
  return <>{parts}</>
}
function ExplorerContent({ segments, speakerNames, candidates, onOpen, onListen, currentSegmentIndex }: TopicExplorerProps) {
  const id = useId()
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<string | null>(null)
  const [catalogPage, setCatalogPage] = useState(0)
  const [passagePage, setPassagePage] = useState(0)
  const [catalogOpen, setCatalogOpen] = useState(false)
  const searchRef = useRef<HTMLInputElement>(null)
  const detailRef = useRef<HTMLHeadingElement>(null)
  const [focusRequest, setFocusRequest] = useState<{ target: 'catalog' | 'detail' } | null>(null)
  useLayoutEffect(() => {
    if (focusRequest) (focusRequest.target === 'catalog' ? searchRef.current : detailRef.current)?.focus()
  }, [focusRequest])
  const result = useMemo(() => candidates ? inspectCandidates(candidates, segments) : null, [candidates, segments])
  const entries = result?.entries ?? []
  const filtered = entries.filter(item => item.name.toLocaleLowerCase('fr').includes(query.trim().toLocaleLowerCase('fr')))
  const currentCatalogPage = Math.min(catalogPage, Math.max(0, Math.ceil(filtered.length / CATALOG_PAGE) - 1))
  const key = (entry: TopicEntry) => `${entry.kind}:${entry.name}`
  const active = entries.find(entry => key(entry) === selected) ?? null
  const currentPassagePage = Math.min(passagePage, Math.max(0, Math.ceil((active?.passages.length ?? 0) / PASSAGE_PAGE) - 1))
  const pageControl = (label: string, page: number, total: number, size: number, set: (value: number) => void) => total > size && <nav className="topic-explorer__pages" aria-label={label}>
    <button type="button" className="button button--secondary" disabled={page === 0} onClick={() => set(page - 1)}>Précédente</button>
    <span>{page * size + 1}–{Math.min((page + 1) * size, total)} sur {total}</span>
    <button type="button" className="button button--secondary" disabled={(page + 1) * size >= total} onClick={() => set(page + 1)}>Suivante</button>
  </nav>
  return <section className="topic-explorer" aria-label="Pistes de sujets">
    <header className="topic-explorer__header"><div><h2>Pistes de sujets</h2><p className="muted">Un rapprochement lexical ne confirme ni classement ni décision. Tous les passages restent à classer.</p></div></header>
    {!candidates ? <p className="empty">Pistes non préparées pour ce travail.</p> : result?.error ? <p role="alert" className="status status--error">{result.error}</p> : segments.length === 0 ? <p className="empty">Aucun passage dans cette transcription.</p> : entries.length === 0 ? <p className="empty">Aucune piste proposée pour ces passages.</p> : <div className="topic-explorer__layout">
      <aside className={`topic-explorer__catalog${catalogOpen ? ' topic-explorer__catalog--open' : ''}`} aria-label="Catalogue des pistes">
        <button className="button button--secondary topic-explorer__toggle" type="button" aria-expanded={catalogOpen} aria-controls={`${id}-catalog`} onClick={() => setCatalogOpen(!catalogOpen)}>{catalogOpen ? 'Masquer le catalogue' : `Choisir une piste (${entries.length})`}</button>
        <div className="topic-explorer__catalog-body" id={`${id}-catalog`}>
          <label htmlFor={`${id}-query`}>Rechercher dans le catalogue des pistes</label>
          <input ref={searchRef} id={`${id}-query`} className="input" type="search" value={query} onChange={e => { setQuery(e.target.value); setCatalogPage(0) }} placeholder="Nom de dossier ou mot…" />
          <p className="small muted" role="status">{filtered.length} piste{filtered.length > 1 ? 's' : ''} sur {entries.length}</p>
          {filtered.length === 0 && <p className="muted">Aucune piste dans ce catalogue pour « {query} ».</p>}
          {pageControl('Pages du catalogue', currentCatalogPage, filtered.length, CATALOG_PAGE, setCatalogPage)}
          {(['folder', 'word', 'link', 'none'] as const).map(kind => {
            const group = filtered.slice(currentCatalogPage * CATALOG_PAGE, (currentCatalogPage + 1) * CATALOG_PAGE).filter(entry => entry.kind === kind)
            if (!group.length) return null
            return <div key={kind} className="topic-explorer__group"><h3>{kind === 'folder' ? 'Dossiers possibles' : kind === 'word' ? 'Mots caractéristiques' : kind === 'link' ? 'Retours lexicaux' : 'Sans piste'}</h3><ul>{group.map(entry => <li key={key(entry)}><button type="button" className={`topic-explorer__choice${key(entry) === selected ? ' topic-explorer__choice--active' : ''}`} aria-pressed={key(entry) === selected} aria-label={kind === 'none' ? 'Sans piste' : `${kind === 'word' ? 'Mot' : kind === 'folder' ? 'Dossier possible' : 'Retour lexical'} ${entry.name}`} onClick={() => { setSelected(key(entry)); setPassagePage(0); setCatalogOpen(false); setFocusRequest({ target: 'detail' }) }}><span>{entry.name}</span><small>{entry.passages.length} passage{entry.passages.length > 1 ? 's' : ''}{entry.occurrences > entry.passages.length ? ` · ${entry.occurrences} occurrences` : ''}</small></button></li>)}</ul></div>
          })}
          {pageControl('Fin du catalogue', currentCatalogPage, filtered.length, CATALOG_PAGE, setCatalogPage)}
        </div>
      </aside>
      <div className="topic-explorer__detail" aria-live="polite">{!active ? <div className="topic-explorer__welcome"><h3>Choisir une piste</h3><p>Ouvrez un dossier possible, un mot caractéristique, un retour lexical ou les passages sans piste pour consulter leurs paroles d’origine.</p></div> : <>
        <header className="topic-explorer__detail-head"><div><p className="small muted">{active.kind === 'folder' ? 'Dossier possible' : active.kind === 'word' ? 'Mot caractéristique' : active.kind === 'link' ? 'Retour lexical' : 'Passages sans piste'}</p><h3 ref={detailRef} tabIndex={-1}>{active.name}</h3><p className="muted">{active.passages.length} passage{active.passages.length > 1 ? 's' : ''}{active.kind !== 'none' ? ` · ${active.occurrences} occurrence${active.occurrences > 1 ? 's' : ''}` : ' sans suggestion'} · ordre d’origine</p></div><button type="button" className="button button--secondary topic-explorer__back" onClick={() => { setSelected(null); setCatalogOpen(true); setFocusRequest({ target: 'catalog' }) }}>Retour au catalogue</button></header>
        {!onListen && <p className="small muted">Écoute indisponible tant que le lecteur partagé n’est pas raccordé.</p>}
        {pageControl('Pages des passages', currentPassagePage, active.passages.length, PASSAGE_PAGE, setPassagePage)}
        <ol className="topic-explorer__passages" start={currentPassagePage * PASSAGE_PAGE + 1}>{active.passages.slice(currentPassagePage * PASSAGE_PAGE, (currentPassagePage + 1) * PASSAGE_PAGE).map(passage => {
          const s = segments[passage.index]
          const assignedName = s.speaker_id && Object.hasOwn(speakerNames, s.speaker_id) ? speakerNames[s.speaker_id] : undefined
          const voice = s.speaker_id ? (typeof assignedName === 'string' ? assignedName.trim() : '') || s.speaker_id : 'Locuteur non attribué'
          const playing = currentSegmentIndex === passage.index
          return <li key={passage.index} data-segment-index={passage.index} className={`topic-explorer__passage${playing ? ' topic-explorer__passage--playing' : ''}`} aria-current={playing ? 'true' : undefined}>
            <div className="topic-explorer__meta">
              <time>{formatClock(s.start_ms, true)} – {formatClock(s.end_ms, true)}</time><span>{voice}</span>
              {passage.occurrences > 1 && <small>{passage.occurrences} occurrences</small>}
              <div className="topic-explorer__actions">
                {onOpen && <button type="button" className="button button--secondary" aria-label={`Voir le passage ${passage.index + 1} dans la transcription`} onClick={() => onOpen(passage.index)}>Voir dans la transcription</button>}
                <button type="button" className="button button--secondary" disabled={!onListen} aria-label={`Écouter le passage ${passage.index + 1}`} onClick={() => onListen?.(passage.index)}>Écouter</button>
              </div>
            </div>
            <p className="topic-explorer__quote"><PassageText text={s.text} spans={passage.spans} /></p>
          </li>
        })}</ol>
        {pageControl('Fin des passages', currentPassagePage, active.passages.length, PASSAGE_PAGE, setPassagePage)}
      </>}</div>
    </div>}
  </section>
}
export function TopicExplorer(props: TopicExplorerProps) {
  if (props.workScope !== props.candidateScope || props.sourceRevision !== props.candidateRevision || !props.workScope || !props.sourceRevision) return <section className="topic-explorer" aria-label="Pistes de sujets"><p role="status" className="status status--error">Pistes d’un autre travail ou d’une autre transcription : aucune parole affichée.</p></section>
  return <ExplorerContent key={`${props.workScope}\u0000${props.sourceRevision}`} {...props} />
}
