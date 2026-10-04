/* Portée explicite fournie par le conteneur : aucun calcul d'empreinte backend ici. */
export interface TranscriptNavigationScope { jobId: string; revision: string }
export interface TranscriptNavigationRequest extends TranscriptNavigationScope {
  /** Unique et croissant pendant la vie du conteneur ; nouvel identifiant pour répéter. */
  requestId: number
  action: 'open' | 'listen'
  /** Index dans job.segments (source originale), jamais dans les résultats filtrés. */
  index: number
}
export interface TranscriptNavigation {
  scope: TranscriptNavigationScope
  request: TranscriptNavigationRequest | null
}

export function validTranscriptRequest(navigation: TranscriptNavigation, jobId: string, length: number): boolean {
  const { scope, request } = navigation
  return scope.jobId === jobId && typeof scope.revision === 'string' && scope.revision.length > 0
    && !!request && request.jobId === scope.jobId && request.revision === scope.revision
    && Number.isSafeInteger(request.requestId) && request.requestId > 0
    && (request.action === 'open' || request.action === 'listen')
    && Number.isSafeInteger(request.index) && request.index >= 0 && request.index < length
}
