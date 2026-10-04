import type { ReactNode } from 'react'

export interface HighlightRange { start: number; end: number; active: boolean; approximate: boolean }
export function HighlightedText({ text, ranges }: { text: string; ranges: HighlightRange[] }) {
  const boundaries = new Set([0, text.length])
  ranges.forEach(({ start, end }) => { boundaries.add(start); boundaries.add(end) })
  const points = [...boundaries].sort((a, b) => a - b)
  const pieces: ReactNode[] = []
  for (let i = 0; i < points.length - 1; i++) {
    const start = points[i], end = points[i + 1]
    if (start === end) continue
    const matching = ranges.filter(r => r.start < end && r.end > start)
    const content = text.slice(start, end)
    pieces.push(matching.length ? <mark key={start} className={`transcript-mark${matching.some(r => r.active) ? ' transcript-mark--current' : ''}`} title={matching.some(r => r.approximate) ? 'Correspondance approchante' : 'Correspondance exacte'}>{content}</mark> : content)
  }
  return <>{pieces}</>
}
