export interface PlaybackSource { src: string; timelineOffsetMs: number; durationMs: number }
export function transcriptMs(audioSeconds: number, source: PlaybackSource): number {
  return audioSeconds * 1000 + source.timelineOffsetMs
}
export function sourceSeconds(ms: number, source: PlaybackSource): number {
  return Math.max(0, Math.min(source.durationMs / 1000, (ms - source.timelineOffsetMs) / 1000))
}
