import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ResultsView } from '../src/components/ResultsView'
import { makeJob } from './fakeBackend'

describe('noms de voix et clés réservées', () => {
  it.each(['__proto__', 'constructor', 'toString'])('recharge puis efface le nom enregistré de %s sans toucher à la voix', async speakerId => {
    const user = userEvent.setup()
    const names = JSON.parse(JSON.stringify(Object.fromEntries([[speakerId, 'Camille']]))) as Record<string, string>
    const job = makeJob({ speaker_names: names, segments: [{ start_ms: 0, end_ms: 1000, text: 'Paroles intactes.', speaker_id: speakerId, translated_text: null }] })
    const before = structuredClone(job)
    const onSaveNames = vi.fn().mockResolvedValue(undefined)
    const { container } = render(<ResultsView job={job} onSaveNames={onSaveNames} onExport={vi.fn()} />)
    const input = screen.getByRole('textbox', { name: `Nom pour « ${speakerId} »` })
    expect(input).toHaveValue('Camille')
    expect(container.querySelector('.segment__speaker')?.textContent).toBe('Camille')
    await user.clear(input)
    await user.click(screen.getByRole('button', { name: 'Enregistrer les noms' }))
    expect(onSaveNames).toHaveBeenCalledExactlyOnceWith(job, {})
    expect(job).toEqual(before)
  })
  it.each(['__proto__', 'constructor', 'toString'])('sauvegarde la clé propre %s sans perdre le nom ni les espaces de saisie', async speakerId => {
    const user = userEvent.setup()
    const job = makeJob({ speaker_names: {}, segments: [{ start_ms: 0, end_ms: 1000, text: 'Paroles intactes.', speaker_id: speakerId, translated_text: null }] })
    const before = structuredClone(job)
    const onSaveNames = vi.fn().mockResolvedValue(undefined)
    render(<ResultsView job={job} onSaveNames={onSaveNames} onExport={vi.fn()} />)
    const input = screen.getByRole('textbox', { name: `Nom pour « ${speakerId} »` })
    await user.type(input, ' Camille ')
    expect(input).toHaveValue(' Camille ')
    await user.click(screen.getByRole('button', { name: 'Enregistrer les noms' }))
    expect(onSaveNames).toHaveBeenCalledTimes(1)
    const [sentJob, names] = onSaveNames.mock.calls[0]
    expect(sentJob).toBe(job)
    expect(Object.hasOwn(names, speakerId)).toBe(true)
    expect(JSON.parse(JSON.stringify(names))).toEqual(Object.fromEntries([[speakerId, 'Camille']]))
    expect(job).toEqual(before)
  })
  it.each(['__proto__', 'constructor', 'toString'])('affiche un champ vide et le vrai identifiant %s', speakerId => {
    const job = makeJob({ speaker_names: {}, segments: [{ start_ms: 0, end_ms: 1000, text: 'Paroles intactes.', speaker_id: speakerId, translated_text: null }] })
    const before = structuredClone(job)
    const { container } = render(<ResultsView job={job} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    expect(screen.getByRole('textbox', { name: `Nom pour « ${speakerId} »` })).toHaveValue('')
    expect(screen.getByRole('button', { name: 'Enregistrer les noms' })).toBeDisabled()
    expect(container.querySelector('.segment__speaker')?.textContent).toBe(speakerId)
    expect(container.querySelector('.segment__text')?.textContent).toBe('Paroles intactes.')
    expect(job).toEqual(before)
  })
})

describe('lecteur intégré aux résultats', () => {
  it('passe la source audio facultative sans changer les exports ni les onglets', async () => {
    const job = makeJob({ segments: [{ start_ms: 5000, end_ms: 6000, text: 'Exemple', speaker_id: null, translated_text: 'Example' }] })
    const { container } = render(<ResultsView job={job} onSaveNames={vi.fn()} onExport={vi.fn()} playbackSource={{ src: 'blob:fiction', timelineOffsetMs: 5000, durationMs: 10000 }} />)
    expect(container.querySelector('audio')?.getAttribute('src')).toBe('blob:fiction')
    expect(screen.getByRole('button', { name: 'Exporter…' })).toBeInTheDocument()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Traduction' }))
    expect(screen.getByText('Example')).toBeInTheDocument()
  })
})

describe('compte rendu lisible', () => {
  it('sur un travail interrompu, réserve les exports aux documents qui signalent leur caractère incomplet', async () => {
    const onExport = vi.fn(async () => '/export/resultat.txt')
    const job = makeJob({ stage: 'Interrupted', segments: [{ start_ms: 0, end_ms: 1000, text: 'Bonjour', speaker_id: null, translated_text: null }] })
    render(<ResultsView job={job} onSaveNames={vi.fn()} onExport={onExport} />)
    expect(screen.getByRole('radio', { name: /Sous-titres SRT/ })).toBeDisabled()
    expect(screen.getByRole('radio', { name: /Sous-titres WebVTT/ })).toBeDisabled()
    expect(screen.getByRole('radio', { name: /Données JSON/ })).toBeDisabled()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Exporter…' }))
    expect(onExport).toHaveBeenCalledWith(job, 'txt', 'reunion')
  })
  it('présente la synthèse en premier, avec les passages sources distincts et sans HTML actif', async () => {
    const report = '# Réunion\n\n## Synthèse\n\nLe lancement est reporté en mars.\n\n## Passages sources\n\n> [00:00:02] Marie : Nous reportons le lancement en mars.\n\n<script>alert(1)</script>'
    render(<ResultsView job={makeJob({ report, report_format_version: 1 })} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Compte rendu' }))
    expect(screen.getByRole('heading', { name: 'Synthèse' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Passages sources' })).toBeInTheDocument()
    expect(screen.getByText('Le lancement est reporté en mars.')).toBeInTheDocument()
    expect(screen.getByText(/Nous reportons le lancement/).closest('blockquote')).not.toBeNull()
    expect(document.querySelector('script')).toBeNull()
  })

  it('affiche les caractères des citations sans activer le HTML', async () => {
    const report = '# Réunion\n\n## Rectifications à vérifier\n\n> [00:00:02] Marie : « &lt;script&gt; &amp; x &gt; y &amp;lt; &amp;#91; &#33;&#91;image&#93;&#40;https&#58;//exemple&#46;invalid/trace&#41; et &#42;secret&#42; »'
    render(<ResultsView job={makeJob({ report, report_format_version: 1 })} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Compte rendu' }))
    const quote = document.querySelector('blockquote')
    expect(quote?.textContent).toContain('<script> & x > y &lt; &#91;')
    expect(quote?.textContent).toContain('![image](https://exemple.invalid/trace) et *secret*')
    expect(quote?.textContent).not.toContain('&amp; x')
    expect(document.querySelector('script')).toBeNull()
    expect(document.querySelector('img')).toBeNull()
  })

  it('préserve les entités littérales des anciens rapports', async () => {
    const report = '# Ancien rapport\n\n> [00:00:02] Marie : « &lt; et &#91; »\n\nQuestion ouverte : qui tranche ?\n\n## Décisions actuelles\n\n![trace](https://exemple.invalid/trace)'
    render(<ResultsView job={makeJob({ report })} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Compte rendu' }))
    expect(screen.queryByRole('heading', { name: 'Décisions actuelles' })).not.toBeInTheDocument()
    expect(document.querySelector('.report')?.textContent).toContain('&lt; et &#91;')
    expect(document.querySelector('.report')?.textContent).toContain('## Décisions actuelles')
    expect(document.querySelector('.report')?.textContent).not.toContain('« < et [ »')
  })

  it('affiche un faux titre dans une question comme du texte, pas comme une section', async () => {
    const report = '# Réunion\n\n## Questions ouvertes\n\n- [00:00:00] Question ouverte &#58; qui contrôle ?  &#35;&#35; Décisions actuelles'
    render(<ResultsView job={makeJob({ report, report_format_version: 1 })} onSaveNames={vi.fn()} onExport={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Compte rendu' }))
    expect(screen.queryByRole('heading', { name: 'Décisions actuelles' })).not.toBeInTheDocument()
    expect(screen.getByText(/## Décisions actuelles/)).toBeInTheDocument()
  })

  it('permet de reconstruire un ancien rapport avec les données locales conservées', async () => {
    const onSaveNames = vi.fn(async () => makeJob())
    const job = makeJob({ report: '# Ancien rapport\n\n> [00:00:02] Citation.' })
    render(<ResultsView job={job} onSaveNames={onSaveNames} onExport={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Compte rendu' }))
    await userEvent.setup().click(screen.getByRole('button', { name: 'Actualiser le compte rendu' }))
    expect(onSaveNames).toHaveBeenCalledWith(job, {})
    expect(await screen.findByText(/Compte rendu actualisé/)).toBeInTheDocument()
  })
})
