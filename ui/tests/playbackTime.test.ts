import { describe, expect, it } from 'vitest'
import { sourceSeconds, transcriptMs } from '../src/lib/playbackTime'

describe('horloge média et transcription', () => {
  it('convertit avec décalage et borne aux limites du média', () => {
    const source = { src: 'blob:fiction', timelineOffsetMs: 5000, durationMs: 30000 }
    expect(transcriptMs(1.25, source)).toBe(6250)
    expect(sourceSeconds(4000, source)).toBe(0)
    expect(sourceSeconds(37000, source)).toBe(30)
    expect(sourceSeconds(7250, source)).toBe(2.25)
  })
})
