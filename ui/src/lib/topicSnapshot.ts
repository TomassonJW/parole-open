import type { Job, Segment } from './types'
import { inspectCandidates, type TopicCandidates, type TopicEntry } from './topicCandidates'

export interface TopicSnapshot { schema_version: 1; job_id: string; source_revision: string; candidates: TopicCandidates | null }
export interface VerifiedTopicSnapshot extends TopicSnapshot { entries: TopicEntry[] }
const ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/
const HASH = /^[0-9a-f]{64}$/
const whole = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0 && !Object.is(value, -0)
const utf8 = (value: string): number => {
  // Une chaîne JS contenant un substitut isolé n'existe pas dans le JSON UTF-8 Rust.
  if (/([\uD800-\uDBFF](?![\uDC00-\uDFFF]))|((?<![\uD800-\uDBFF])[\uDC00-\uDFFF])/.test(value)) throw invalid()
  return new TextEncoder().encode(value).length
}
const invalid = () => new Error('Source ou pistes sauvegardées non vérifiables.')
// Rust compare les octets UTF-8, pas les unités UTF-16 ni une collation de langue.
function compareRustText(left: string, right: string): number {
  const a = new TextEncoder().encode(left)
  const b = new TextEncoder().encode(right)
  for (let i = 0; i < Math.min(a.length, b.length); i++) {
    if (a[i] !== b[i]) return a[i] - b[i]
  }
  return a.length - b.length
}

/** Sérialisation byte-for-byte du tuple serde_json Rust ; ne pas sérialiser la fixture triée. */
export function topicSource(job: Pick<Job, 'id' | 'segments'>): string {
  if (!ID.test(job.id) || !Array.isArray(job.segments) || job.segments.length > 2000) throw invalid()
  const segments = job.segments.map((s: Segment) => {
    if (!s || !whole(s.start_ms) || !whole(s.end_ms) || s.end_ms < s.start_ms
      || typeof s.text !== 'string' || typeof s.speaker_id !== 'string' && s.speaker_id !== null
      || typeof s.translated_text !== 'string' && s.translated_text !== null
      || utf8(s.text) > 4096
      || (s.speaker_id !== null && utf8(s.speaker_id) > 256)
      || (s.translated_text !== null && utf8(s.translated_text) > 16384)) throw invalid()
    return { start_ms: s.start_ms, end_ms: s.end_ms, text: s.text, speaker_id: s.speaker_id, translated_text: s.translated_text }
  })
  const compact = JSON.stringify(['parole-topic-source-v1', job.id, segments])
  if (utf8(compact) > 64 * 1024 * 1024) throw invalid()
  return compact
}

export async function sourceRevision(job: Pick<Job, 'id' | 'segments'>): Promise<string> {
  const source = topicSource(job)
  if (!globalThis.crypto?.subtle?.digest) throw new Error('Vérification cryptographique locale indisponible.')
  const hash = await globalThis.crypto.subtle.digest('SHA-256', new TextEncoder().encode(source))
  return Array.from(new Uint8Array(hash), b => b.toString(16).padStart(2, '0')).join('')
}

/** Ne renvoie jamais des pistes tant que la portée et les preuves n'ont pas été contrôlées. */
export async function verifyTopicSnapshot(raw: unknown, job: Pick<Job, 'id' | 'segments'>): Promise<VerifiedTopicSnapshot> {
  const revision = await sourceRevision(job)
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) throw invalid()
  const r = raw as Record<string, unknown>
  if (r.schema_version !== 1 || r.job_id !== job.id || !HASH.test(r.source_revision as string) || r.source_revision !== revision
    || !Object.hasOwn(r, 'candidates')) throw invalid()
  if (r.candidates === null) return { schema_version: 1, job_id: job.id, source_revision: revision, candidates: null, entries: [] }
  const inspected = inspectCandidates(r.candidates, job.segments)
  if (inspected.error) throw invalid()
  const candidates = r.candidates as TopicCandidates
  // Une liste « sans piste » omise ou une proposition incohérente ne devient jamais navigable.
  const suggested = new Set<number>()
  for (const item of [...candidates.words, ...candidates.possible_folders]) {
    for (const proof of item.evidence) suggested.add(proof.segment_index)
  }
  const missing = job.segments.map((_, i) => i).filter(i => !suggested.has(i))
  if (JSON.stringify(missing) !== JSON.stringify(candidates.without_suggestion)) throw invalid()
  for (const [i, word] of candidates.words.entries()) {
    if (typeof word.term !== 'string' || (i && compareRustText(candidates.words[i - 1].term, word.term) >= 0)) throw invalid()
    const distinct = new Set(word.evidence.map(e => e.segment_index)).size
    if (word.lexical_weight !== Math.floor(10000 / distinct)) throw invalid()
    for (const proof of word.evidence) {
      const bytes = new TextEncoder().encode(job.segments[proof.segment_index].text)
      const text = new TextDecoder().decode(bytes.slice(proof.byte_start, proof.byte_end))
      if ((/[0-9]/.test(text) ? text : text.toLowerCase()) !== word.term) throw invalid()
    }
  }
  for (const [i, folder] of candidates.possible_folders.entries()) {
    if (typeof folder.name !== 'string' || (i && compareRustText(candidates.possible_folders[i - 1].name, folder.name) >= 0)) throw invalid()
  }
  const linked = candidates.words.filter(w => new Set(w.evidence.map(e => e.segment_index)).size > 1)
  if (linked.length !== candidates.links.length || linked.some((w, i) => w.term !== candidates.links[i].shared_term || JSON.stringify(w.evidence) !== JSON.stringify(candidates.links[i].evidence))) throw invalid()
  return { schema_version: 1, job_id: job.id, source_revision: revision, candidates, entries: inspected.entries }
}
