import type { Job } from '../lib/types'
import type { ReturnTypePresentation } from '../hooks/usePresentation'
import { DEFAULT_VIEW, type ViewOptions } from '../lib/transcriptPresentation'
import { speakerIds, storedSpeakerName } from '../lib/format'
import '../presentationSettings.css'

type Props = { job: Job; presentation: ReturnTypePresentation; onClose: () => void }
const presets: { name: string; view: ViewOptions }[] = [
  { name: 'Lecture fluide', view: DEFAULT_VIEW },
  { name: 'Détaillé', view: { mode: 'detailed', pause_ms: 2000, show_timestamps: true } },
  { name: 'Sans horaires', view: { ...DEFAULT_VIEW, show_timestamps: false } },
]
export function PresentationPanel({ job, presentation: p, onClose }: Props) {
  const { draft, state, change } = p
  const active = presets.find(({ view }) => JSON.stringify(view) === JSON.stringify(draft.screen))?.name ?? 'Personnalisé'
  const exportView = draft.export.linked ? draft.screen : draft.export.view
  const setScreen = (view: ViewOptions) => change(previous => ({ ...previous, screen: view }))
  const setExport = (view: ViewOptions) => change(previous => ({ ...previous, export: { ...previous.export, view } }))
  const disabled = !state?.writable
  return <section id="presentation-panel" className="panel presentation" aria-labelledby="presentation-title">
    <div className="panel__head"><h2 id="presentation-title" className="panel__title">Présentation</h2><button type="button" className="button button--secondary" onClick={onClose}>Fermer</button></div>
    {!state && !p.error && <p role="status">Chargement des réglages…</p>}
    {state?.warning && <p className="status" role="alert">{state.warning}</p>}
    {state && !state.writable && <p className="status" role="alert">Réglages protégés : aucune modification possible.</p>}
    <fieldset disabled={disabled} className="presentation__group"><legend>Lecture à l’écran</legend>
      <label className="field__label" htmlFor="presentation-preset">Préréglage</label>
      <select id="presentation-preset" className="input" value={active} onChange={e => { const preset = presets.find(item => item.name === e.target.value); if (preset) setScreen({ ...preset.view }) }}>
        {presets.map(({ name }) => <option key={name}>{name}</option>)}<option value="Personnalisé" disabled>Personnalisé</option>
      </select>
      <label className="field__label" htmlFor="presentation-pause">Pause maximale entre passages : {(draft.screen.pause_ms / 1000).toLocaleString('fr-FR')} s</label>
      <input id="presentation-pause" type="range" min="0" max="10000" step="500" value={draft.screen.pause_ms} disabled={draft.screen.mode === 'detailed'} onChange={e => setScreen({ ...draft.screen, pause_ms: Number(e.target.value) })} />
      <label className="presentation__check"><input type="checkbox" checked={draft.screen.show_timestamps} onChange={e => setScreen({ ...draft.screen, show_timestamps: e.target.checked })} /> Horaires à l’écran</label>
      <button type="button" className="button button--secondary" onClick={() => change(previous => ({ ...previous, screen: { ...DEFAULT_VIEW }, export: { linked: true, view: { ...DEFAULT_VIEW }, content: 'complete' } }))}>Réinitialiser les options de lecture et d’export</button>
    </fieldset>
    <fieldset disabled={disabled} className="presentation__group"><legend>Export</legend>
      <label className="presentation__check"><input type="checkbox" checked={!draft.export.linked} onChange={e => change(previous => ({ ...previous, export: { ...previous.export, linked: !e.target.checked, view: { ...previous.screen } } }))} /> Choix distincts pour l’export</label>
      {!draft.export.linked && <>
        <label className="presentation__check"><input type="checkbox" checked={exportView.show_timestamps} onChange={e => setExport({ ...exportView, show_timestamps: e.target.checked })} /> Horaires dans l’export</label>
        <label className="field__label" htmlFor="presentation-export-mode">Mise en forme export</label>
        <select id="presentation-export-mode" className="input" value={exportView.mode} onChange={e => setExport({ ...exportView, mode: e.target.value as ViewOptions['mode'] })}><option value="fluid">Lecture fluide</option><option value="detailed">Détaillé</option></select>
        <label className="field__label" htmlFor="presentation-export-pause">Pause export : {(exportView.pause_ms / 1000).toLocaleString('fr-FR')} s</label>
        <input id="presentation-export-pause" type="range" min="0" max="10000" step="500" value={exportView.pause_ms} disabled={exportView.mode === 'detailed'} onChange={e => setExport({ ...exportView, pause_ms: Number(e.target.value) })} />
      </>}
      <label className="presentation__check"><input type="checkbox" checked={draft.export.content === 'transcript'} onChange={e => change(previous => ({ ...previous, export: { ...previous.export, content: e.target.checked ? 'transcript' : 'complete' } }))} /> Transcription seule</label>
      <p className="muted small">Complet inclut traduction et compte rendu ; transcription seule les exclut.</p>
    </fieldset>
    <fieldset disabled={disabled} className="presentation__group"><legend>Couleurs des locuteurs</legend>
      {speakerIds(job).length === 0 && <p className="muted small">Aucun locuteur identifié.</p>}
      {speakerIds(job).map(id => <div className="presentation__color" key={id}><span className="presentation__color-name">{storedSpeakerName(job.speaker_names, id) || id} <span className="muted small">({id})</span></span>
        <span className="presentation__palette">{['#2563eb', '#b45309', '#15803d', '#a21caf'].map(color => <button key={color} type="button" className="presentation__swatch" style={{ backgroundColor: color }} aria-label={`Attribuer ${color} à ${id}`} onClick={() => change(previous => ({ ...previous, speaker_colors: { ...previous.speaker_colors, [id]: color } }))} />)}</span>
        <input type="color" aria-label={`Couleur de ${id}`} value={draft.speaker_colors[id] ?? '#64748b'} onChange={e => change(previous => ({ ...previous, speaker_colors: { ...previous.speaker_colors, [id]: e.target.value } }))} />
      </div>)}
      <button type="button" className="button button--secondary" onClick={() => change(previous => ({ ...previous, speaker_colors: {} }))}>Réinitialiser les couleurs</button>
      <p className="muted small">Les noms restent visibles. Couleurs à l’écran et dans Word seulement ; TXT et Markdown restent sans couleur.</p>
    </fieldset>
    <button type="button" className="button button--secondary" disabled={disabled} onClick={() => void p.saveDefaults()}>Utiliser ces options pour les futurs documents (sans couleurs)</button>
    {p.defaultsStatus && <p role="status">{p.defaultsStatus}</p>}
    {p.defaultsError && <p role="alert">Options pour les futurs documents non enregistrées : {p.defaultsError} <button type="button" className="button button--secondary" onClick={p.retryDefaults}>Réessayer les options pour les futurs documents</button></p>}
    <p className="muted small" role="status">{p.saving ? 'Enregistrement…' : state ? (p.error || p.dirty ? `Modifications non enregistrées · dernière révision ${state.revision}` : `Enregistré : révision ${state.revision}`) : ''}</p>
    {p.previewing && <p role="status">Actualisation de l’aperçu…</p>}
    {p.previewError && <p role="alert">Aperçu indisponible : {p.previewError}</p>}
    {p.error && <p role="alert">Réglages non enregistrés : {p.error} <button type="button" className="button button--secondary" onClick={p.retry}>Réessayer</button></p>}
  </section>
}
