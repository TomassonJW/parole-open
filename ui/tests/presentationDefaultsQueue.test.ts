import { describe, expect, it, vi } from 'vitest'
import { defaultsSession, type DefaultsView } from '../src/lib/presentationDefaultsQueue'
import { defaultPreferences, type PresentationPreferences, type PresentationState } from '../src/lib/transcriptPresentation'
import { fakeBackend } from './fakeBackend'

const defer = <T,>() => { let resolve!: (v: T) => void; let reject!: (e: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
const pref = (pause: number): PresentationPreferences => ({ ...defaultPreferences(), screen: { ...defaultPreferences().screen, pause_ms: pause } })
const state = (revision = 0, preferences = defaultPreferences()): PresentationState => ({ revision, preferences, warning: null, writable: true })

describe('file des choix explicites pour les futurs documents', () => {
  it('sérialise les demandes et enregistre le dernier choix sans couleurs ni mutation tardive', async () => {
    const read = defer<PresentationState>()
    let stored = state()
    const { backend } = fakeBackend({ loadPresentationDefaults: vi.fn().mockReturnValueOnce(read.promise).mockImplementation(async () => stored),
      savePresentationDefaults: vi.fn(async (p, r) => { stored = state(r + 1, p); return stored }) })
    const session = defaultsSession(backend)
    const first = pref(2500); first.speaker_colors = { A: '#123456' }
    const writing = session.request(first)
    first.screen.pause_ms = 9000
    void session.request(pref(3000)); void session.request(pref(3500))
    expect(backend.loadPresentationDefaults).toHaveBeenCalledTimes(1)
    expect(backend.savePresentationDefaults).not.toHaveBeenCalled()
    read.resolve(state()); await writing
    await vi.waitFor(() => expect(backend.savePresentationDefaults).toHaveBeenCalledTimes(2))
    expect(backend.savePresentationDefaults).toHaveBeenNthCalledWith(1, pref(2500), 0)
    expect(backend.savePresentationDefaults).toHaveBeenNthCalledWith(2, pref(3500), 1)
    expect(stored.preferences).toEqual(pref(3500))
  })
  it('garde une panne de lecture et son brouillon sans relancer avant un choix explicite', async () => {
    const { backend } = fakeBackend()
    vi.mocked(backend.loadPresentationDefaults).mockRejectedValueOnce(new Error('Lecture interrompue'))
    const session = defaultsSession(backend)
    await session.request(pref(4000))
    let view!: DefaultsView
    const unsubscribe = defaultsSession(backend).subscribe(v => { view = v })
    expect(view.error).toContain('Lecture interrompue')
    expect(view.saved).toBe(false)
    expect(backend.loadPresentationDefaults).toHaveBeenCalledTimes(1)
    expect(backend.savePresentationDefaults).not.toHaveBeenCalled()
    await session.retry()
    expect(backend.savePresentationDefaults).toHaveBeenCalledExactlyOnceWith(pref(4000), 0)
    expect(view).toEqual({ error: null, saved: true, saving: false })
    unsubscribe()
  })
  it('refuse le fichier de défauts en lecture seule sans écriture', async () => {
    const { backend } = fakeBackend({ loadPresentationDefaults: vi.fn(async () => ({ ...state(), writable: false, warning: 'Version future conservée' })) })
    const session = defaultsSession(backend)
    let view!: DefaultsView
    session.subscribe(v => { view = v })
    await session.request(pref(4500))
    expect(view.error).toBe('Version future conservée')
    expect(view.saved).toBe(false)
    expect(backend.savePresentationDefaults).not.toHaveBeenCalled()
  })
  it.each(['révision', 'préférences'] as const)('ne déclare pas le succès sur un accusé différent : %s', async kind => {
    const { backend } = fakeBackend({ savePresentationDefaults: vi.fn(async () => kind === 'révision' ? state(0, pref(5000)) : state(1, pref(2000))) })
    const session = defaultsSession(backend)
    let view!: DefaultsView
    session.subscribe(v => { view = v })
    await session.request(pref(5000))
    expect(view.error).toContain('Accusé des défauts incohérent')
    expect(view.saved).toBe(false)
    expect(backend.savePresentationDefaults).toHaveBeenCalledTimes(1)
  })
  it('sépare les choix de deux connexions et refuse les préférences invalides avant toute écriture', async () => {
    const a = fakeBackend().backend; const b = fakeBackend().backend
    await defaultsSession(a).request(pref(5500))
    expect(b.loadPresentationDefaults).not.toHaveBeenCalled()
    let view!: DefaultsView
    defaultsSession(b).subscribe(v => { view = v })
    await defaultsSession(b).request(pref(5501))
    expect(view.error).not.toBeNull()
    expect(view.saved).toBe(false)
    expect(b.loadPresentationDefaults).not.toHaveBeenCalled()
    expect(b.savePresentationDefaults).not.toHaveBeenCalled()
  })
})
