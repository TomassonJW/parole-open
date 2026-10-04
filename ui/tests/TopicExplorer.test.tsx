import producerFixture from './fixtures/topicCandidates.generated.json'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { TopicExplorer } from '../src/components/TopicExplorer'
import { byteBoundaries, type TopicCandidates } from '../src/lib/topicCandidates'
import type { Segment } from '../src/lib/types'

// Written by src-core/tests/topic_candidates_transport.rs using the real producer, not hand-shaped.
const fixture = producerFixture as {
  scope: string; segments: Segment[]; speaker_names: Record<string, string>; candidates: TopicCandidates; empty_candidates: TopicCandidates
}
const props = { workScope: fixture.scope, candidateScope: fixture.scope, sourceRevision: 'source-v1', candidateRevision: 'source-v1', segments: fixture.segments, speakerNames: fixture.speaker_names, candidates: fixture.candidates }

describe('pistes issues du préparateur Rust', () => {
  it('demande de voir le passage original sans déclencher son écoute', async () => {
    const open = vi.fn()
    const listen = vi.fn()
    const user = userEvent.setup()
    const { container } = render(<TopicExplorer {...props} onOpen={open} onListen={listen} />)
    await user.click(screen.getByRole('button', { name: /mot orchidée/i }))
    const row = container.querySelector('[data-segment-index="5"]') as HTMLElement
    expect(row.querySelector('.topic-explorer__quote')?.textContent).toBe(fixture.segments[5].text)
    await user.click(within(row).getByRole('button', { name: 'Voir le passage 6 dans la transcription' }))
    expect(open).toHaveBeenCalledExactlyOnceWith(5)
    expect(listen).not.toHaveBeenCalled()
    expect(container.querySelector('audio')).toBeNull()
    expect(row.querySelector('.topic-explorer__quote')?.textContent).toBe(fixture.segments[5].text)
  })

  it('permet de voir le passage au clavier même sans écoute disponible', async () => {
    const open = vi.fn()
    const user = userEvent.setup()
    const { container } = render(<TopicExplorer {...props} onOpen={open} />)
    await user.click(screen.getByRole('button', { name: /mot orchidée/i }))
    const row = container.querySelector('[data-segment-index="4"]') as HTMLElement
    const button = within(row).getByRole('button', { name: 'Voir le passage 5 dans la transcription' })
    expect(within(row).getByRole('button', { name: /écouter/i })).toBeDisabled()
    button.focus()
    await user.keyboard('{Enter}')
    expect(open).toHaveBeenCalledExactlyOnceWith(4)
    expect(button).toHaveFocus()
    expect(container.querySelector('audio')).toBeNull()
  })

  it('ne propose pas une ouverture absente et conserve une écoute indépendante', async () => {
    const user = userEvent.setup()
    const open = vi.fn()
    const listen = vi.fn()
    const { rerender } = render(<TopicExplorer {...props} onListen={listen} />)
    await user.click(screen.getByRole('button', { name: /mot orchidée/i }))
    expect(screen.queryByRole('button', { name: /voir le passage/i })).not.toBeInTheDocument()
    rerender(<TopicExplorer {...props} onOpen={open} onListen={listen} />)
    await user.click(screen.getByRole('button', { name: 'Écouter le passage 6' }))
    expect(listen).toHaveBeenCalledExactlyOnceWith(5)
    expect(open).not.toHaveBeenCalled()
  })

  it.each(['travail', 'révision', 'preuve'] as const)('retire les actions quand la portée ou les paroles ne sont plus valides : %s', async variant => {
    const user = userEvent.setup()
    const open = vi.fn()
    const listen = vi.fn()
    const { rerender } = render(<TopicExplorer {...props} onOpen={open} onListen={listen} />)
    await user.click(screen.getByRole('button', { name: /mot orchidée/i }))
    expect(screen.getByRole('button', { name: 'Voir le passage 6 dans la transcription' })).toBeEnabled()
    const bad = structuredClone(fixture.candidates)
    bad.words[0].evidence[0].citation += ' texte périmé'
    rerender(<TopicExplorer {...props} onOpen={open} onListen={listen}
      workScope={variant === 'travail' ? 'autre-travail' : props.workScope}
      sourceRevision={variant === 'révision' ? 'autre-révision' : props.sourceRevision}
      candidates={variant === 'preuve' ? bad : props.candidates} />)
    expect(screen.queryByRole('button', { name: /voir le passage/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /écouter le passage/i })).not.toBeInTheDocument()
    expect(open).not.toHaveBeenCalled()
    expect(listen).not.toHaveBeenCalled()
  })

  it('sélectionne une piste puis restitue les passages complets, voix et indices originaux à écouter', async () => {
    const listen = vi.fn()
    const { container } = render(<TopicExplorer {...props} onListen={listen} />)
    expect(screen.getByText(/rapprochement lexical ne confirme ni classement ni décision/i)).toBeInTheDocument()
    expect(screen.getByText('Dossiers possibles')).toBeInTheDocument()
    expect(screen.getByText('Mots caractéristiques')).toBeInTheDocument()
    await userEvent.setup().type(screen.getByRole('searchbox', { name: /catalogue/i }), 'orchidée')
    await userEvent.setup().click(screen.getByRole('button', { name: /mot orchidée/i }))
    const rows = container.querySelectorAll('.topic-explorer__passage')
    expect([...rows].map(row => Number(row.getAttribute('data-segment-index')))).toEqual([1, 4, 5])
    expect(rows[0]).toHaveTextContent(fixture.segments[1].text)
    expect(rows[0]).toHaveTextContent('Benoît')
    expect(rows[0]).toHaveTextContent('2 occurrences')
    await userEvent.setup().click(within(rows[2] as HTMLElement).getByRole('button', { name: /écouter/i }))
    expect(listen).toHaveBeenCalledExactlyOnceWith(5)
  })

  it.each(['__proto__', 'constructor', 'toString'])('affiche une voix sans nom enregistré pour la clé %s', async speakerId => {
    const segments = structuredClone(fixture.segments)
    segments[1].speaker_id = speakerId
    const listen = vi.fn()
    const { container } = render(<TopicExplorer {...props} segments={segments} speakerNames={{}} onListen={listen} />)
    await userEvent.setup().click(screen.getByRole('button', { name: /mot orchidée/i }))
    const row = container.querySelector('[data-segment-index="1"]') as HTMLElement
    expect(row).not.toBeNull()
    expect(within(row).getByText(speakerId, { exact: true })).toBeInTheDocument()
    expect(row.querySelector('.topic-explorer__quote')?.textContent).toBe(segments[1].text)
    await userEvent.setup().click(within(row).getByRole('button', { name: /écouter/i }))
    expect(listen).toHaveBeenCalledExactlyOnceWith(1)
  })

  it('ne présente pas un nom hérité comme un nom enregistré', async () => {
    const inheritedNames: Record<string, string> = Object.create({ B: 'Nom non enregistré' })
    const { container } = render(<TopicExplorer {...props} speakerNames={inheritedNames} />)
    await userEvent.setup().click(screen.getByRole('button', { name: /mot orchidée/i }))
    const row = container.querySelector('[data-segment-index="1"]') as HTMLElement
    expect(within(row).getByText('B', { exact: true })).toBeInTheDocument()
    expect(row).not.toHaveTextContent('Nom non enregistré')
  })

  it.each(['__proto__', 'constructor', 'toString'])('conserve le nom explicitement enregistré pour la clé %s', async speakerId => {
    const segments = structuredClone(fixture.segments)
    segments[1].speaker_id = speakerId
    const names = Object.fromEntries([[speakerId, ' Camille ']])
    const { container } = render(<TopicExplorer {...props} segments={segments} speakerNames={names} />)
    await userEvent.setup().click(screen.getByRole('button', { name: /mot orchidée/i }))
    const row = container.querySelector('[data-segment-index="1"]') as HTMLElement
    expect(within(row).getByText('Camille', { exact: true })).toBeInTheDocument()
    expect(row.querySelector('.topic-explorer__quote')?.textContent).toBe(segments[1].text)
    expect(segments[1].speaker_id).toBe(speakerId)
  })

  it('ne transforme pas les retours lexicaux discontinus en continuité ou rattachement transitif', async () => {
    const { container } = render(<TopicExplorer {...props} />)
    await userEvent.setup().click(screen.getByRole('button', { name: /retour lexical orchidée/i }))
    expect([...container.querySelectorAll('.topic-explorer__passage')].map(row => Number(row.getAttribute('data-segment-index')))).toEqual([1, 4, 5])
    expect(screen.queryByText(/écoute indisponible/i)).toBeInTheDocument()
    expect(screen.getAllByRole('button', { name: /écouter/i })[0]).toBeDisabled()
    expect(container.querySelector('audio')).toBeNull()
  })

  it('garde le dossier court sans mot et distingue les passages sans piste', async () => {
    const user = userEvent.setup()
    const { container } = render(<TopicExplorer {...props} />)
    await user.click(screen.getByRole('button', { name: /dossier possible AB/i }))
    expect([...container.querySelectorAll('.topic-explorer__passage')].map(row => row.getAttribute('data-segment-index'))).toEqual(['0', '6'])
    await user.click(screen.getByRole('button', { name: /^sans piste$/i }))
    expect([...container.querySelectorAll('.topic-explorer__passage')].map(row => row.getAttribute('data-segment-index'))).toEqual(['3'])
    expect(container.querySelector('.topic-explorer__quote')).toHaveTextContent(fixture.segments[3].text)
  })

  it('conserve exactement Unicode composé/décomposé, emoji et HTML littéral ; refuse une coupure UTF-8', async () => {
    const user = userEvent.setup()
    const { container, rerender } = render(<TopicExplorer {...props} />)
    await user.click(screen.getByRole('button', { name: /mot café/i }))
    expect(container.querySelector('.topic-explorer__quote')?.textContent).toBe(fixture.segments[1].text)
    expect(container.querySelector('.topic-explorer__mark')?.textContent).toBe('café')
    const bad = structuredClone(fixture.candidates)
    const entry = bad.words.find(w => w.term === 'café')!
    entry.evidence[0].byte_end -= 1 // split a UTF-8 combining mark
    rerender(<TopicExplorer {...props} candidates={bad} />)
    expect(screen.getByRole('alert')).toHaveTextContent(/preuves.*incohérentes/i)
    expect(container.querySelector('.topic-explorer__quote')).toBeNull()
  })

  it('convertit les bornes emoji et accent sans couper un substitut, et échappe les paroles HTML', async () => {
    expect([...byteBoundaries('😀é').entries()]).toEqual([[0, 0], [4, 2], [5, 3], [7, 4]])
    const text = '😀 é <img src=x onerror=alert(1)>'
    const segments: Segment[] = [{ start_ms: 0, end_ms: 900, text, speaker_id: null, translated_text: null }]
    const candidates: TopicCandidates = { schema_version: 1, words: [{ term: '😀', lexical_weight: 1, evidence: [{ segment_index: 0, start_ms: 0, end_ms: 900, citation: text, byte_start: 0, byte_end: 4 }] }], links: [], possible_folders: [], without_suggestion: [] }
    const { container } = render(<TopicExplorer {...props} segments={segments} candidates={candidates} />)
    await userEvent.setup().click(screen.getByRole('button', { name: /mot 😀/i }))
    expect(container.querySelector('.topic-explorer__mark')?.textContent).toBe('😀')
    expect(container.querySelector('.topic-explorer__quote')?.textContent).toBe(text)
    expect(container.querySelector('img')).toBeNull()
  })

  it.each(['citation', 'time', 'index', 'offset'] as const)('refuse sans fuite une preuve falsifiée : %s', variant => {
    const bad = structuredClone(fixture.candidates)
    const evidence = bad.words[0].evidence[0]
    if (variant === 'citation') evidence.citation += ' périmée'
    if (variant === 'time') evidence.start_ms += 1
    if (variant === 'index') evidence.segment_index = 999
    if (variant === 'offset') evidence.byte_start = 999
    const { container } = render(<TopicExplorer {...props} candidates={bad} />)
    expect(screen.getByRole('alert')).toBeInTheDocument()
    expect(container.querySelector('.topic-explorer__quote')).toBeNull()
  })

  it('refuse les doublons de preuve et les indices sans piste contradictoires', () => {
    const duplicate = structuredClone(fixture.candidates)
    duplicate.words[0].evidence.push({ ...duplicate.words[0].evidence[0] })
    const { rerender } = render(<TopicExplorer {...props} candidates={duplicate} />)
    expect(screen.getByRole('alert')).toBeInTheDocument()
    const contradictory = structuredClone(fixture.candidates)
    contradictory.without_suggestion.push(1)
    rerender(<TopicExplorer {...props} candidates={contradictory} />)
    expect(screen.getByRole('alert')).toBeInTheDocument()
  })

  it('borne la portée au travail et à la révision puis réinitialise sélection et recherche', async () => {
    const user = userEvent.setup()
    const { rerender, container } = render(<TopicExplorer {...props} />)
    await user.click(screen.getByRole('button', { name: /mot orchidée/i }))
    rerender(<TopicExplorer {...props} workScope="autre-travail" />)
    expect(screen.getByRole('status')).toHaveTextContent(/autre travail/i)
    expect(container.querySelector('.topic-explorer__quote')).toBeNull()
    rerender(<TopicExplorer {...props} sourceRevision="nouvelle-source" />)
    expect(container.querySelector('.topic-explorer__quote')).toBeNull()
    rerender(<TopicExplorer {...props} workScope="autre-travail" candidateScope="autre-travail" sourceRevision="source-v2" candidateRevision="source-v2" />)
    expect(screen.getByText('Choisir une piste')).toBeInTheDocument()
  })

  it('ouvre une piste au clavier et surligne le passage courant du lecteur partagé', async () => {
    const user = userEvent.setup()
    const { container } = render(<TopicExplorer {...props} currentSegmentIndex={4} />)
    const button = screen.getByRole('button', { name: /mot orchidée/i })
    button.focus()
    await user.keyboard('{Enter}')
    expect(button).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByRole('heading', { name: 'orchidée' })).toHaveFocus()
    expect(container.querySelector('[data-segment-index="4"]')).toHaveAttribute('aria-current', 'true')
    await user.click(button)
    expect(screen.getByRole('heading', { name: 'orchidée' })).toHaveFocus()
  })

  it('rend le focus au catalogue après le retrait du bouton de retour', async () => {
    const user = userEvent.setup()
    render(<TopicExplorer {...props} />)
    const search = screen.getByRole('searchbox', { name: /catalogue/i })
    expect(search).not.toHaveFocus()
    await user.click(screen.getByRole('button', { name: /mot orchidée/i }))
    await user.click(screen.getByRole('button', { name: 'Retour au catalogue' }))
    expect(search).toHaveFocus()
  })

  it('distingue absence, sortie vide et corruption de schéma', () => {
    const { rerender } = render(<TopicExplorer {...props} candidates={null} />)
    expect(screen.getByText(/non préparées/i)).toBeInTheDocument()
    rerender(<TopicExplorer {...props} segments={[]} candidates={fixture.empty_candidates} />)
    expect(screen.getByText(/aucun passage/i)).toBeInTheDocument()
    rerender(<TopicExplorer {...props} candidates={{ ...fixture.candidates, schema_version: 999 }} />)
    expect(screen.getByRole('alert')).toBeInTheDocument()
  })

  it('pagine catalogue et passages tout en gardant les comptes globaux et le callback original', async () => {
    // Corpus local fictif d'effort ; le transport ci-dessus reste celui du vrai producteur.
    const segments: Segment[] = Array.from({ length: 235 }, (_, i) => ({ text: `mot${i} ${'long '.repeat(20)}`, start_ms: i * 1000, end_ms: (i + 1) * 1000, speaker_id: null, translated_text: null }))
    const evidence = segments.map((s, i) => ({ segment_index: i, start_ms: s.start_ms, end_ms: s.end_ms, citation: s.text, byte_start: 0, byte_end: `mot${i}`.length }))
    const words = segments.slice(0, 120).map((_, i) => ({ term: `mot${i}`, lexical_weight: 1, evidence: [evidence[i]] }))
    const candidates: TopicCandidates = { schema_version: 1, words, links: [], possible_folders: [], without_suggestion: [] }
    const listen = vi.fn()
    const open = vi.fn()
    const user = userEvent.setup()
    const { container, rerender } = render(<TopicExplorer {...props} segments={segments} candidates={candidates} onOpen={open} onListen={listen} />)
    expect(screen.getByText('120 pistes sur 120')).toBeInTheDocument()
    expect(container.querySelectorAll('.topic-explorer__choice')).toHaveLength(50)
    await user.click(within(screen.getByRole('navigation', { name: 'Pages du catalogue' })).getByRole('button', { name: 'Suivante' }))
    expect(container.querySelectorAll('.topic-explorer__choice')).toHaveLength(50)
    await user.click(screen.getByRole('button', { name: /^mot mot50$/i }))
    expect(container.querySelectorAll('.topic-explorer__passage')).toHaveLength(1)
    await user.click(screen.getByRole('button', { name: 'Voir le passage 51 dans la transcription' }))
    expect(open).toHaveBeenCalledExactlyOnceWith(50)
    expect(listen).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: /écouter le passage 51/i }))
    expect(listen).toHaveBeenCalledWith(50)
    const many: TopicCandidates = { ...candidates, words: [{ term: 'mot', lexical_weight: 1, evidence: evidence.map(e => ({ ...e, byte_end: 3 })) }] }
    rerender(<TopicExplorer {...props} segments={segments} candidates={many} onOpen={open} onListen={listen} />)
    await user.click(screen.getByRole('button', { name: /^mot mot$/i }))
    expect(screen.getByText(/235 passages.*235 occurrences/)).toBeInTheDocument()
    expect(container.querySelectorAll('.topic-explorer__passage')).toHaveLength(100)
    await user.click(within(screen.getByRole('navigation', { name: 'Pages des passages' })).getByRole('button', { name: 'Suivante' }))
    expect(container.querySelectorAll('.topic-explorer__passage')).toHaveLength(100)
    await user.click(screen.getByRole('button', { name: 'Voir le passage 101 dans la transcription' }))
    expect(open).toHaveBeenNthCalledWith(2, 100)
    expect(open).toHaveBeenCalledTimes(2)
    expect(listen).toHaveBeenCalledTimes(1)
    await user.click(screen.getByRole('button', { name: /écouter le passage 101/i }))
    expect(listen).toHaveBeenLastCalledWith(100)
  })
})
