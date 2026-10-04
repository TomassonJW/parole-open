import type { Segment } from './types'

export interface MatchSpan { segmentIndex: number; start: number; end: number }
export interface TranscriptMatch { kind: 'exact' | 'approx'; spans: MatchSpan[] }
interface Character { segmentIndex: number; start: number; end: number }
interface Run { text: string; chars: (Character | null)[]; protectedTokens: { start: number; end: number }[] }

// Conservative: an all-caps word may be ordinary speech, but correcting it
// as a code risks returning an entirely different identifier.
function protectedSourceToken(token: string): boolean {
  return /[\p{N}_-]/u.test(token) || /^(?:\p{Lu}[\p{M}\u200c\u200d]*){2,}$/u.test(token.normalize('NFC'))
}
const identifier = /[\p{L}\p{N}][\p{L}\p{M}\p{N}\u200c\u200d]*(?:[-_][\p{L}\p{N}][\p{L}\p{M}\p{N}\u200c\u200d]*)*/gu

const graphemes = typeof Intl.Segmenter === 'function' ? new Intl.Segmenter(undefined, { granularity: 'grapheme' }) : null
function* units(text: string): Iterable<{ segment: string; index: number }> {
  if (graphemes) { yield* graphemes.segment(text); return }
  // Conservative fallback for older Webviews: preserve surrogate pairs and combining marks.
  for (const match of text.matchAll(/\P{M}\p{M}*|\p{M}+/gu)) yield { segment: match[0], index: match.index }
}
function fold(value: string): string {
  return value.replace(/œ/gi, m => m === 'Œ' ? 'OE' : 'oe').replace(/æ/gi, m => m === 'Æ' ? 'AE' : 'ae')
    .normalize('NFD').replace(/[\u0300-\u036f]/g, '').replace(/[\u2018\u2019\u02bc]/g, "'").toLowerCase()
}
function append(run: Run, text: string, segmentIndex: number) {
  const base = run.text.length
  for (const { segment, index } of units(text)) {
    if (/^\s+$/u.test(segment)) {
      if (run.text && !run.text.endsWith(' ')) { run.text += ' '; run.chars.push(null) }
      continue
    }
    const normalized = fold(segment)
    run.text += normalized
    for (let i = 0; i < normalized.length; i++) run.chars.push({ segmentIndex, start: index, end: index + segment.length })
  }
  // Source casing is retained via the UTF-16 character map; compute once at indexing.
  for (const match of run.text.slice(base).matchAll(identifier)) {
    const start = base + match.index!
    const end = start + match[0].length
    const first = run.chars[start]
    const last = run.chars[end - 1]
    if (first && last && protectedSourceToken(text.slice(first.start, last.end))) {
      run.protectedTokens.push({ start, end })
    }
  }
}
function runs(segments: Segment[], translated: boolean): Run[] {
  const result: Run[] = []
  segments.forEach((segment, index) => {
    const previous = segments[index - 1]
    const text = translated ? segment.translated_text ?? '' : segment.text
    const previousText = translated ? previous?.translated_text ?? '' : previous?.text ?? ''
    const joins = previous && previous.speaker_id !== null && previous.speaker_id === segment.speaker_id
      && text.trim() !== '' && previousText.trim() !== ''
      && segment.start_ms - previous.end_ms >= 0 && segment.start_ms - previous.end_ms <= 2000
    let run = joins ? result[result.length - 1] : undefined
    if (!run) { run = { text: '', chars: [], protectedTokens: [] }; result.push(run) }
    else if (run.text && !run.text.endsWith(' ')) { run.text += ' '; run.chars.push(null) }
    append(run, text, index)
  })
  return result
}
function spans(run: Run, start: number, end: number): MatchSpan[] {
  const result: MatchSpan[] = []
  for (let i = start; i < end; i++) {
    const char = run.chars[i]
    if (!char) continue
    const last = result[result.length - 1]
    if (last && last.segmentIndex === char.segmentIndex) last.end = Math.max(last.end, char.end)
    else result.push({ ...char })
  }
  return result
}
function distance(a: string, b: string, limit: number): number {
  if (Math.abs(a.length - b.length) > limit) return limit + 1
  // Damerau-Levenshtein optimal-string-alignment; transposition costs one.
  let previousPrevious: number[] = []
  let previous = Array.from({ length: b.length + 1 }, (_, i) => i)
  for (let i = 1; i <= a.length; i++) {
    const row = [i]
    for (let j = 1; j <= b.length; j++) {
      row[j] = Math.min(previous[j] + 1, row[j - 1] + 1, previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1))
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) row[j] = Math.min(row[j], previousPrevious[j - 2] + 1)
    }
    previousPrevious = previous; previous = row
  }
  return previous[b.length]
}
function fuzzyLimit(token: string): number {
  return /^[a-z]{4,}$/u.test(token) ? token.length >= 8 ? 2 : 1 : 0
}
function overlapsProtected(run: Run, start: number, end: number, requireWhole: boolean): boolean {
  const tokens = run.protectedTokens
  let left = 0, right = tokens.length
  while (left < right) {
    const middle = (left + right) >>> 1
    if (tokens[middle].end <= start) left = middle + 1
    else right = middle
  }
  for (let i = left; i < tokens.length && tokens[i].start < end; i++) {
    if (!requireWhole || start > tokens[i].start || end < tokens[i].end) return true
  }
  return false
}
export interface TranscriptIndex { runs: Run[] }
export function createTranscriptIndex(segments: Segment[], translated = false): TranscriptIndex { return { runs: runs(segments, translated) } }
export function searchTranscriptIndex(index: TranscriptIndex, query: string, approximate = true): TranscriptMatch[] {
  const needle = fold(query.trim()).replace(/\s+/gu, ' ')
  const codeLike = /[\p{N}_-]/u.test(query) || /^(?:[A-Z]{2,}[-_]?[A-Z0-9]*)(?:\s+[A-Z]{2,}[-_]?[A-Z0-9]*)*$/u.test(query.trim())
  if (!needle) return []
  const found: TranscriptMatch[] = []
  for (const run of index.runs) {
    for (let start = run.text.indexOf(needle); start !== -1; start = run.text.indexOf(needle, start + 1)) {
      if (overlapsProtected(run, start, start + needle.length, true)) continue
      const parts = spans(run, start, start + needle.length)
      if (parts.length) found.push({ kind: 'exact', spans: parts })
    }
    if (approximate && !codeLike && /^[\p{L}\p{N}]+(?:['\s]+[\p{L}\p{N}]+)*$/u.test(needle)) {
      const tokens = [...needle.matchAll(/[\p{L}\p{N}]+/gu)]
      const words = tokens.map(m => m[0])
      if (words.length <= 32 && !words.some(w => /\d/u.test(w) || w.length > 64)) {
        const candidates = [...run.text.matchAll(/[\p{L}\p{N}]+/gu)]
        for (let i = 0; i <= candidates.length - words.length; i++) {
          let budget = 0
          for (let j = 0; j < words.length; j++) {
            const actual = candidates[i + j][0]
            if (overlapsProtected(run, candidates[i + j].index!, candidates[i + j].index! + actual.length, false)) { budget = 3; break }
            if (j) {
              const queryGap = needle.slice(tokens[j - 1].index! + words[j - 1].length, tokens[j].index!)
              const actualGap = run.text.slice(candidates[i + j - 1].index! + candidates[i + j - 1][0].length, candidates[i + j].index!)
              if (queryGap !== actualGap) { budget = 3; break }
            }
            const allowed = fuzzyLimit(words[j])
            const edit = actual.length > 64 ? 3 : distance(words[j], actual, Math.min(allowed, 2 - budget))
            if (edit > allowed) { budget = 3; break }
            budget += edit
          }
          if (budget < 1 || budget > 2) continue
          const start = candidates[i].index!
          const end = candidates[i + words.length - 1].index! + candidates[i + words.length - 1][0].length
          const parts = spans(run, start, end)
          if (parts.length) found.push({ kind: 'approx', spans: parts })
        }
      }
    }
  }
  const unique = new Map<string, TranscriptMatch>()
  const exactRanges = new Map<number, { start: number; end: number }[]>()
  for (const match of found) if (match.kind === 'exact') for (const span of match.spans) {
    const ranges = exactRanges.get(span.segmentIndex) ?? []
    ranges.push(span)
    exactRanges.set(span.segmentIndex, ranges)
  }
  for (const [index, ranges] of exactRanges) {
    ranges.sort((a, b) => a.start - b.start)
    const merged: typeof ranges = []
    for (const range of ranges) {
      const previous = merged[merged.length - 1]
      if (previous && range.start <= previous.end) previous.end = Math.max(previous.end, range.end)
      else merged.push({ ...range })
    }
    exactRanges.set(index, merged)
  }
  for (const match of found) {
    if (match.kind === 'approx' && match.spans.some(span => {
      const ranges = exactRanges.get(span.segmentIndex) ?? []
      let left = 0, right = ranges.length
      while (left < right) {
        const middle = (left + right) >>> 1
        if (ranges[middle].end <= span.start) left = middle + 1
        else right = middle
      }
      return left < ranges.length && ranges[left].start < span.end
    })) continue
    const key = JSON.stringify(match.spans)
    if (!unique.has(key) || match.kind === 'exact') unique.set(key, match)
  }
  return [...unique.values()].sort((a, b) => a.spans[0].segmentIndex - b.spans[0].segmentIndex || a.spans[0].start - b.spans[0].start || (a.kind === 'exact' ? -1 : 1))
}
export function searchTranscript(segments: Segment[], query: string, translated = false, approximate = true): TranscriptMatch[] {
  return searchTranscriptIndex(createTranscriptIndex(segments, translated), query, approximate)
}
