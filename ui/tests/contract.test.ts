import { beforeEach, describe, expect, it, vi } from 'vitest'

const invoke = vi.fn()
const listen = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a), isTauri: () => true }))
vi.mock('@tauri-apps/api/event', () => ({ listen: (...a: unknown[]) => listen(...a) }))
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }))

const { tauriBackend, BackendError } = await import('../src/lib/backend')
const { normalizeJob } = await import('../src/lib/types')

const rustJob = {
  media_name: 'a.wav',
  duration_ms: 60000,
  completed_chunks: 1,
  stage: 'Transcribing',
  segments: [{ start_ms: 0, end_ms: 1000, text: 'Bonjour', speaker_id: null, translated_text: null }],
  speaker_names: {},
  error: null,
  source_language: 'fr',
}

beforeEach(() => {
  invoke.mockReset()
  listen.mockReset()
})

describe('contrat invoke', () => {
  it('start_job envoie mediaPath, sourceLanguage, targetLanguage, reportLanguage, generateReport', async () => {
    invoke.mockResolvedValue({ id: 'j1', job: rustJob })
    const job = await tauriBackend.startJob({ mediaPath: '/m/a.wav', sourceLanguage: 'fr', targetLanguage: 'en', reportLanguage: 'fr', generateReport: true })
    expect(invoke).toHaveBeenCalledWith('start_job', {
      mediaPath: '/m/a.wav',
      sourceLanguage: 'fr',
      targetLanguage: 'en',
      reportLanguage: 'fr',
      reportModelId: 'baseline',
      generateReport: true,
    })
    expect(job.id).toBe('j1')
    expect(job.source_language).toBe('fr')
    expect(job.chunk_ms).toBe(0)
  })

  it('préserve le modèle enregistré et convertit un ancien travail sans sélection en modèle de base', async () => {
    invoke.mockResolvedValueOnce({ id: 'j1', job: { ...rustJob, report_model_id: 'qwen3-4b-instruct-2507-q4_k_m' } })
    expect((await tauriBackend.resumeJob('j1')).report_model_id).toBe('qwen3-4b-instruct-2507-q4_k_m')
    invoke.mockResolvedValueOnce({ id: 'j1', job: rustJob })
    expect((await tauriBackend.resumeJob('j1')).report_model_id).toBe('baseline')
  })

  it('install_report_model est distinct du téléchargement de base', async () => {
    invoke.mockResolvedValue({ installed: true, missing: [], report_models: [] })
    await tauriBackend.installReportModel('qwen3-4b-instruct-2507-q4_k_m')
    expect(invoke).toHaveBeenCalledExactlyOnceWith('install_report_model', { id: 'qwen3-4b-instruct-2507-q4_k_m' })
  })

  it('resume_job, save_speaker_names et export_job utilisent id', async () => {
    invoke.mockResolvedValue({ id: 'j1', job: rustJob })
    await tauriBackend.resumeJob('j1')
    expect(invoke).toHaveBeenLastCalledWith('resume_job', { id: 'j1' })
    const renamed = await tauriBackend.saveSpeakerNames('j1', { S1: 'Alice' })
    expect(renamed.id).toBe('j1')
    expect(invoke).toHaveBeenLastCalledWith('save_speaker_names', { id: 'j1', names: { S1: 'Alice' } })
    invoke.mockResolvedValue('/out/a.srt')
    const written = await tauriBackend.exportJob('j1', 'srt', 'reunion')
    expect(invoke).toHaveBeenLastCalledWith('export_job', { id: 'j1', format: 'srt', defaultName: 'reunion' })
    expect(written).toBe('/out/a.srt')
  })

  it('list_jobs lit un tableau d’enveloppes', async () => {
    invoke.mockResolvedValue([{ id: 'x', job: rustJob }])
    const jobs = await tauriBackend.listJobs()
    expect(invoke).toHaveBeenCalledWith('list_jobs', undefined)
    expect(jobs.map((j) => j.id)).toEqual(['x'])
  })

  it('model_status et prepare_models', async () => {
    invoke.mockResolvedValue({ installed: false, missing: ['whisper-small'] })
    expect(await tauriBackend.modelStatus()).toEqual({ installed: false, missing: ['whisper-small'] })
    expect(invoke).toHaveBeenLastCalledWith('model_status', undefined)
    invoke.mockResolvedValue(null)
    expect(await tauriBackend.prepareModels()).toBeNull()
    expect(invoke).toHaveBeenLastCalledWith('prepare_models', undefined)
  })

  it('écoute l’événement job-progress', async () => {
    listen.mockResolvedValue(() => {})
    const handler = vi.fn()
    await tauriBackend.onJobUpdated(handler)
    expect(listen.mock.calls[0][0]).toBe('job-progress')
    const cb = listen.mock.calls[0][1] as (e: { payload: unknown }) => void
    cb({ payload: { id: 'j2', job: rustJob } })
    cb({ payload: { nope: true } })
    expect(handler).toHaveBeenCalledTimes(1)
    expect(handler.mock.calls[0][0].id).toBe('j2')
  })

  it('une commande absente devient une erreur française explicite', async () => {
    invoke.mockRejectedValue('Command model_status not found')
    const error = await tauriBackend.modelStatus().catch((e) => e)
    expect(error).toBeInstanceOf(BackendError)
    expect(error.kind).toBe('missing-command')
    expect(error.message).toContain('n’est pas encore disponible'.replace('’', "'"))
  })

  it('une réponse mal formée est refusée au lieu d’être inventée', async () => {
    invoke.mockResolvedValue({ media_name: 'a.wav', stage: 'Ready' })
    const error = await tauriBackend.resumeJob('j1').catch((e) => e)
    expect(error.kind).toBe('invalid-response')
  })
})

describe('normalizeJob', () => {
  it('conserve les noms propres réservés reçus en JSON sans muter la source', () => {
    const names = JSON.parse('{"__proto__":"  Éva  ","constructor":"Noé","toString":"Léa","S1":"Ana","nombre":4,"vide":null}')
    const raw = { id: 'a', job: { ...rustJob, speaker_names: names } }
    const before = JSON.stringify(raw)
    const normalized = normalizeJob(raw)
    expect(Object.hasOwn(names, '__proto__')).toBe(true)
    for (const key of ['__proto__', 'constructor', 'toString', 'S1']) {
      expect(Object.hasOwn(normalized.speaker_names, key)).toBe(true)
      expect(normalized.speaker_names[key]).toBe(names[key])
    }
    expect(Object.keys(normalized.speaker_names).sort()).toEqual(['S1', '__proto__', 'constructor', 'toString'])
    expect(JSON.parse(JSON.stringify(normalized.speaker_names))['__proto__']).toBe('  Éva  ')
    expect(JSON.stringify(raw)).toBe(before)
    expect(normalized.segments).toEqual(rustJob.segments)
  })
  it('écarte les noms hérités et non textuels sans perdre une clé réservée valide', () => {
    const names = Object.create({ S2: 'Nom hérité' })
    Object.defineProperties(names, {
      ['__proto__']: { value: 'Éva', enumerable: true },
      S1: { value: 'Ana', enumerable: true },
      constructor: { value: { nom: 'Objet interdit' }, enumerable: true },
      toString: { value: false, enumerable: true },
    })
    const normalized = normalizeJob({ id: 'a', job: { ...rustJob, speaker_names: names } })
    expect(Object.keys(normalized.speaker_names).sort()).toEqual(['S1', '__proto__'])
    expect(Object.hasOwn(normalized.speaker_names, 'S2')).toBe(false)
    expect(Object.hasOwn(normalized.speaker_names, 'constructor')).toBe(false)
    expect(Object.hasOwn(normalized.speaker_names, 'toString')).toBe(false)
    expect(normalized.speaker_names['__proto__']).toBe('Éva')
  })
  it('préserve une voix réservée en liste, au retour de sauvegarde et dans un événement', async () => {
    const names = JSON.parse('{"__proto__":"Éva","S1":"Ana"}')
    const envelope = { id: 'a', job: { ...rustJob, speaker_names: names } }
    invoke.mockResolvedValueOnce([envelope])
    const listed = await tauriBackend.listJobs()
    expect(invoke).toHaveBeenLastCalledWith('list_jobs', undefined)
    expect(Object.hasOwn(listed[0].speaker_names, '__proto__')).toBe(true)
    invoke.mockResolvedValueOnce(envelope)
    const saved = await tauriBackend.saveSpeakerNames('a', listed[0].speaker_names)
    expect(invoke).toHaveBeenLastCalledWith('save_speaker_names', { id: 'a', names })
    expect(Object.hasOwn(saved.speaker_names, '__proto__')).toBe(true)
    expect(saved.speaker_names['__proto__']).toBe('Éva')
    listen.mockResolvedValue(() => {})
    const handler = vi.fn()
    await tauriBackend.onJobUpdated(handler)
    const callback = listen.mock.calls[0][1] as (e: { payload: unknown }) => void
    callback({ payload: envelope })
    expect(handler).toHaveBeenCalledOnce()
    const received = handler.mock.calls[0][0]
    expect(Object.hasOwn(received.speaker_names, '__proto__')).toBe(true)
    expect(received.speaker_names['__proto__']).toBe('Éva')
    expect(received.segments).toEqual(rustJob.segments)
  })
  it('rejette un état inconnu', () => {
    expect(() => normalizeJob({ id: 'a', job: { ...rustJob, stage: 'Done' } })).toThrow(/inconnu/)
  })
  it('conserve la version du rapport et traite les anciens travaux comme du texte brut', () => {
    expect(normalizeJob({ id: 'a', job: rustJob }).report_format_version).toBe(0)
    expect(normalizeJob({ id: 'a', job: { ...rustJob, report_format_version: 1 } }).report_format_version).toBe(1)
    expect(normalizeJob({ id: 'a', job: { ...rustJob, report_format_version: 2 } }).report_format_version).toBe(0)
  })
  it('ignore les segments invalides', () => {
    const job = normalizeJob({ id: 'a', job: { ...rustJob, segments: [{ text: 'ok' }, { foo: 1 }] } })
    expect(job.segments).toHaveLength(1)
  })
})
