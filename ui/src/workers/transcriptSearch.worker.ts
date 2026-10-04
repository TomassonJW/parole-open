/// <reference lib="webworker" />
import { createTranscriptIndex, searchTranscriptIndex, type TranscriptIndex } from '../lib/transcriptSearch'
import type { Segment } from '../lib/types'

let index: TranscriptIndex | null = null
let revision = -1
self.onmessage = (event: MessageEvent<{ type: 'init'; revision: number; segments: Segment[]; translated: boolean } | { type: 'search'; revision: number; request: number; query: string; approximate: boolean }>) => {
  const message = event.data
  try {
    if (message.type === 'init') {
      revision = message.revision
      index = createTranscriptIndex(message.segments, message.translated)
      self.postMessage({ type: 'ready', revision })
    } else if (message.revision === revision && index) {
      self.postMessage({ type: 'results', revision, request: message.request, matches: searchTranscriptIndex(index, message.query, message.approximate) })
    }
  } catch {
    self.postMessage({ type: 'error', revision, request: message.type === 'search' ? message.request : undefined })
  }
}
