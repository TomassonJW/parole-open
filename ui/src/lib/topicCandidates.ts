import type { Segment } from './types'

export interface Evidence { segment_index: number; start_ms: number; end_ms: number; citation: string; byte_start: number; byte_end: number }
export interface TopicCandidates {
  schema_version: number
  words: { term: string; lexical_weight: number; evidence: Evidence[] }[]
  links: { shared_term: string; evidence: Evidence[] }[]
  possible_folders: { name: string; evidence: Evidence[] }[]
  without_suggestion: number[]
}
export interface Passage { index: number; spans: { start: number; end: number }[]; occurrences: number }
export interface TopicEntry { kind: 'folder' | 'word' | 'link' | 'none'; name: string; passages: Passage[]; occurrences: number }

// Rust counts UTF-8 bytes; JS slices UTF-16 code units. Only code-point boundaries are valid.
export function byteBoundaries(text: string): Map<number, number> {
  const boundaries = new Map<number, number>([[0, 0]])
  let bytes = 0, units = 0
  for (const char of text) {
    bytes += new TextEncoder().encode(char).length
    units += char.length
    boundaries.set(bytes, units)
  }
  return boundaries
}

export function inspectCandidates(value: unknown, segments: readonly Segment[]): { entries: TopicEntry[]; error?: string } {
  const invalid = { entries: [] as TopicEntry[], error: 'Preuves des pistes incohérentes : aucune parole affichée.' }
  if (!value || typeof value !== 'object') return invalid
  const c = value as TopicCandidates
  if (c.schema_version !== 1 || !Array.isArray(c.words) || !Array.isArray(c.links) || !Array.isArray(c.possible_folders) || !Array.isArray(c.without_suggestion)) return invalid
  const whole = (n: number) => Number.isSafeInteger(n) && n >= 0
  const validSegment = (s: Segment) => s && typeof s.text === 'string' && whole(s.start_ms) && whole(s.end_ms) && s.end_ms >= s.start_ms
  if (!segments.every(validSegment)) return invalid
  const maps = segments.map(s => byteBoundaries(s.text))
  const entries: TopicEntry[] = []
  const suggestedIndices = new Set<number>()
  const add = (kind: TopicEntry['kind'], name: string, evidence: Evidence[]) => {
    if (typeof name !== 'string' || !name.trim() || !Array.isArray(evidence) || !evidence.length) return false
    const byIndex = new Map<number, Passage>()
    const seenSpans = new Set<string>()
    for (const e of evidence) {
      if (!e || !whole(e.segment_index) || e.segment_index >= segments.length || !whole(e.start_ms) || !whole(e.end_ms) || !whole(e.byte_start) || !whole(e.byte_end)) return false
      const segment = segments[e.segment_index]
      const start = maps[e.segment_index].get(e.byte_start), end = maps[e.segment_index].get(e.byte_end)
      if (e.citation !== segment.text || e.start_ms !== segment.start_ms || e.end_ms !== segment.end_ms || start === undefined || end === undefined || start >= end) return false
      if (kind === 'folder' && segment.text.slice(start, end) !== name) return false
      if ((kind === 'word' || kind === 'link') && segment.text.slice(start, end).toLowerCase() !== name.toLowerCase()) return false
      const spanKey = `${e.segment_index}:${e.byte_start}:${e.byte_end}`
      if (seenSpans.has(spanKey)) return false
      seenSpans.add(spanKey)
      suggestedIndices.add(e.segment_index)
      const passage = byIndex.get(e.segment_index) ?? { index: e.segment_index, spans: [], occurrences: 0 }
      passage.spans.push({ start, end }); passage.occurrences++
      byIndex.set(e.segment_index, passage)
    }
    entries.push({ kind, name, passages: [...byIndex.values()].sort((a, b) => a.index - b.index), occurrences: evidence.length })
    return true
  }
  for (const f of c.possible_folders) if (!f || !add('folder', f.name, f.evidence)) return invalid
  for (const w of c.words) if (!w || !whole(w.lexical_weight) || !add('word', w.term, w.evidence)) return invalid
  for (const l of c.links) if (!l || !add('link', l.shared_term, l.evidence)) return invalid
  if (c.without_suggestion.some(n => !whole(n) || n >= segments.length || suggestedIndices.has(n)) || new Set(c.without_suggestion).size !== c.without_suggestion.length) return invalid
  if (c.without_suggestion.length) entries.push({ kind: 'none', name: 'Sans piste', passages: [...c.without_suggestion].sort((a, b) => a - b).map(index => ({ index, spans: [], occurrences: 0 })), occurrences: 0 })
  return { entries }
}
