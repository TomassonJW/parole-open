import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

describe('paquet Windows privé', () => {
  it('préserve les instructions vectorielles du moteur Windows sur le processeur cible', () => {
    const build = readFileSync(resolve(process.cwd(), '../scripts/build-native.sh'), 'utf8')
    expect(build).toContain('GGML_AVX2=ON')
    expect(build).toContain('GGML_FMA=ON')
    expect(build).toContain('GGML_SSE42=ON')
  })

  it('ne mélange pas les moteurs Linux dans la candidate Windows', () => {
    const config = JSON.parse(readFileSync(resolve(process.cwd(), '../src-tauri/tauri.windows.conf.json'), 'utf8'))
    const resources: string[] = config.bundle.resources
    for (const asset of ['native/whisper-cli.exe', 'native/ffmpeg.exe', 'native/ffprobe.exe', 'native/llama-completion.exe', 'native/onnxruntime.dll', 'native/sherpa-onnx-c-api.dll', 'native/embedding.onnx', 'native/segmentation.onnx', 'native/THIRD_PARTY_LICENSES.md']) {
      expect(resources).toContain(asset)
    }
    expect(resources).not.toContain('native/*')
    expect(resources).not.toContain('native/whisper-cli')
  })

  it('impose le français aux dialogues de l’installateur', () => {
    const config = JSON.parse(readFileSync(resolve(process.cwd(), '../src-tauri/tauri.conf.json'), 'utf8'))
    expect(config.bundle.windows.nsis.languages).toEqual(['French'])
    expect(config.bundle.windows.nsis.customLanguageFiles).toEqual({ French: 'installer/French.nsh' })
    const translations = readFileSync(resolve(process.cwd(), '../src-tauri/installer/French.nsh'), 'utf8')
    expect([...translations.matchAll(/^LangString\s+\w+\s+\$\{LANG_FRENCH\}/gm)]).toHaveLength(27)
    expect(translations).not.toMatch(/déja|continer|s'éxécuter/i)
  })
})
