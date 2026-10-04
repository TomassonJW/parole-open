import { describe, expect, it } from 'vitest'
import { searchTranscript, createTranscriptIndex, searchTranscriptIndex } from '../src/lib/transcriptSearch'
import type { Segment } from '../src/lib/types'

const seg = (text: string, start_ms = 0, end_ms = 1000, speaker_id: string | null = 'A'): Segment => ({ text, start_ms, end_ms, speaker_id, translated_text: null })

describe('recherche des occurrences', () => {
  it('normalise les espaces dans et entre les passages sans perdre les offsets', () => {
    expect(searchTranscript([seg('budget  des travaux')], 'budget des travaux')[0].spans).toEqual([{ segmentIndex: 0, start: 0, end: 19 }])
    expect(searchTranscript([seg('budget ', 0, 1000), seg(' des travaux', 1000, 2000)], 'budget des travaux')[0].spans).toEqual([{ segmentIndex: 0, start: 0, end: 6 }, { segmentIndex: 1, start: 1, end: 12 }])
    expect(searchTranscript([seg('budget des travaux')], 'budget des travaux')).toHaveLength(1)
    expect(searchTranscript([seg('transcription')], 'transcriptio')).toHaveLength(1)
    expect(searchTranscript([seg('parfaite parfaites')], 'parfaite')).toHaveLength(2)
    expect(searchTranscript([seg('😀e\u0301  été')], 'é été')[0].spans).toEqual([{ segmentIndex: 0, start: 2, end: 9 }])
  })
  it('réutilise un index et borne le fuzzy sans tronquer les correspondances exactes', () => {
    const long = 'a'.repeat(1000)
    const index = createTranscriptIndex([seg(`${long} ${'mot '.repeat(40)}fin`)], false)
    expect(searchTranscriptIndex(index, long, true)).toHaveLength(1)
    expect(searchTranscriptIndex(index, `${long.slice(0, -1)}b`, true)).toHaveLength(0)
    expect(searchTranscriptIndex(index, 'mot '.repeat(33).trim(), true).length).toBeGreaterThan(0)
  })
  it('conserve les offsets UTF-16 originaux avec accents, ligatures, apostrophes et plusieurs résultats', () => {
    const results = searchTranscript([seg('Été été, l’œuvre et l\'oeuvre 😀')], 'ete')
    expect(results.map(r => [r.kind, r.spans])).toEqual([
      ['exact', [{ segmentIndex: 0, start: 0, end: 3 }]],
      ['exact', [{ segmentIndex: 0, start: 4, end: 7 }]],
    ])
    expect(searchTranscript([seg('aaaa')], 'aa').map(r => r.spans[0].start)).toEqual([0, 1, 2])
    expect(searchTranscript([seg('😀Œuvre')], 'oeuvre')[0].spans).toEqual([{ segmentIndex: 0, start: 2, end: 7 }])
    expect(searchTranscript([seg('l’œuvre et l\'oeuvre')], "L'OEUVRE").map(r => r.spans[0])).toEqual([
      { segmentIndex: 0, start: 0, end: 7 }, { segmentIndex: 0, start: 11, end: 19 },
    ])
  })
  it('franchit uniquement les segments contigus de même voix connue et gap admis', () => {
    const segments = [seg('bonjour', 0, 1000), seg('monde', 1100, 2000), seg('bonjour', 3000, 4000), seg('monde', 6100, 7000)]
    expect(searchTranscript(segments, 'bonjour monde').map(r => r.spans)).toEqual([[{ segmentIndex: 0, start: 0, end: 7 }, { segmentIndex: 1, start: 0, end: 5 }]])
    expect(searchTranscript([seg('bonjour', 0, 1000, null), seg('monde', 1000, 2000, null)], 'bonjour monde')).toEqual([])
    expect(searchTranscript([seg('bonjour', 0, 1000, 'A'), seg('monde', 1000, 2000, 'B')], 'bonjour monde')).toEqual([])
  })
  it.each([null, '', '  '])('ne franchit pas une traduction absente ou vide (%s)', missing => {
    const segments = [
      { ...seg('We should', 0, 1000), translated_text: 'Nous devons' },
      { ...seg('not', 1000, 1500), translated_text: missing },
      { ...seg('publish', 1500, 2000), translated_text: 'publier' },
    ]
    expect(searchTranscript(segments, 'devons publier', true)).toEqual([])
    expect(searchTranscript(segments, 'publier', true)[0].spans).toEqual([{ segmentIndex: 2, start: 0, end: 7 }])
  })
  it('borne les fautes avec transposition et budget par expression, sans assouplir codes et courts', () => {
    expect(searchTranscript([seg('l’œuvrre')], "l'oeuvre").map(r => r.kind)).toEqual(['approx'])
    expect(searchTranscript([seg('transcription parfaite')], 'transcripiton parfaite').map(r => r.kind)).toEqual(['approx'])
    expect(searchTranscript([seg('transcription parfaite')], 'transcripiton parfaute').map(r => r.kind)).toEqual(['approx'])
    expect(searchTranscript([seg('chat 12 AB-45')], 'caht').map(r => r.kind)).toEqual(['approx'])
    expect(searchTranscript([seg('chat 12 AB-45')], '12')).toHaveLength(1)
    expect(searchTranscript([seg('chat 12 AB-45')], '13')).toHaveLength(0)
    expect(searchTranscript([seg('CODE')], 'CDOE')).toHaveLength(0)
    expect(searchTranscript([seg('chat 12 AB-45')], 'AB-46')).toHaveLength(0)
    expect(searchTranscript([seg('abc')], 'acb')).toHaveLength(0)
    expect(searchTranscript([seg('parfaite musique')], 'parfaaaite musiqque')).toHaveLength(0)
  })
  it('ne corrige pas un code source même si la saisie est minuscule', () => {
    expect(searchTranscript([seg('Le code ABCD est affiché.')], 'abce')).toEqual([])
    expect(searchTranscript([seg('Le code ABCD est affiché.')], 'abcd').map(r => r.kind)).toEqual(['exact'])
    expect(searchTranscript([seg('Le code AB_CD est affiché.')], 'ab_ce')).toEqual([])
    expect(searchTranscript([seg('Le code AB-CD est affiché.')], 'ab_ce')).toEqual([])
  })
  it('protège aussi les codes majuscules dont les accents sont décomposés', () => {
    const source = 'Le code A\u0308BCD est affiché.'
    expect(searchTranscript([seg(source)], 'abce')).toEqual([])
    expect(searchTranscript([seg(source)], 'abcd').map(r => r.kind)).toEqual(['exact'])
  })
  it.each([
    ['किं-१२', 'क'],
    ['क्\u200dष-१२', 'क'],
    ['A\u1ab0BCD', 'A'],
  ])('ne coupe pas les marques et jointures internes du code %s', (source, fragment) => {
    expect(searchTranscript([seg(source)], fragment, false, false)).toEqual([])
    const exact = searchTranscript([seg(source)], source, false, false)
    expect(exact).toHaveLength(1)
    expect(exact[0].spans[0]).toEqual({ segmentIndex: 0, start: 0, end: source.length })
  })
  it('respecte les limites des identifiants et nombres sans supprimer les occurrences dans une phrase', () => {
    const text = 'AB-456 AB-45 A12 123 12 AB_45 AB_456 ABCD'
    expect(searchTranscript([seg(text)], 'AB-45').map(r => r.spans[0].start)).toEqual([7])
    expect(searchTranscript([seg(text)], '12').map(r => r.spans[0].start)).toEqual([21])
    expect(searchTranscript([seg(text)], 'AB_45').map(r => r.spans[0].start)).toEqual([24])
    expect(searchTranscript([seg(text)], 'abcd').map(r => r.spans[0].start)).toEqual([37])
    expect(searchTranscript([seg('aaaa 12a a12 12')], 'aa').map(r => r.spans[0].start)).toEqual([0, 1, 2])
  })
  it('ne traite pas une saisie utilisateur comme une regex', () => {
    expect(searchTranscript([seg('a.* et [ok]')], '.*')).toHaveLength(1)
    expect(searchTranscript([seg('a.* et [ok]')], '[')).toHaveLength(1)
  })
})
