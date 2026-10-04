import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach, vi } from 'vitest'
import { createTranscriptIndex, searchTranscriptIndex, type TranscriptIndex } from '../src/lib/transcriptSearch'

// jsdom has no Worker; execute the real pure index/search behind an asynchronous transport.
class LocalSearchWorker {
  onmessage: ((event: MessageEvent) => void) | null = null
  onerror: (() => void) | null = null
  index: TranscriptIndex | null = null
  revision = 0
  terminated = false
  postMessage(message: any) {
    queueMicrotask(() => {
      if (this.terminated) return
      try {
        if (message.type === 'init') {
          this.index = createTranscriptIndex(message.segments, message.translated)
          this.revision = message.revision
          this.onmessage?.({ data: { type: 'ready', revision: this.revision } } as MessageEvent)
        } else if (this.index && message.revision === this.revision) {
          this.onmessage?.({ data: { type: 'results', revision: this.revision, request: message.request, matches: searchTranscriptIndex(this.index, message.query, message.approximate) } } as MessageEvent)
        }
      } catch { this.onerror?.() }
    })
  }
  terminate() { this.terminated = true }
}
vi.stubGlobal('Worker', LocalSearchWorker)

afterEach(() => {
  cleanup()
  window.localStorage.clear()
})
