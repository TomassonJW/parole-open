import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { StartForm } from '../src/components/StartForm'
import { normalizeModelStatus } from '../src/lib/types'

const retired = {
  id: 'qwen3-4b-instruct-2507-q4_k_m', name: 'Ancien modèle 4B',
  file: 'archive.gguf', bytes: 100, sha256: 'a'.repeat(64),
  url: 'https://example.invalid/model', installed: true,
  available_for_new_jobs: false,
}
const baseline = { ...retired, id: 'baseline', name: 'Modèle de base', available_for_new_jobs: true }

function props() {
  return {
    path: '/fiction/reunion.wav', busy: false, blockedReason: null,
    modelStatus: { installed: true, core_installed: true, missing: [] },
    reportModels: [baseline, retired], installError: null, installProgress: null,
    onInstall: vi.fn(), onStart: vi.fn(), onCancel: vi.fn(),
  }
}

describe('modèles retirés des nouveaux usages', () => {
  it('ne propose ni le choix ni un téléchargement du modèle retiré', async () => {
    const p = props()
    const user = userEvent.setup()
    render(<StartForm {...p} />)
    await user.click(screen.getByLabelText(/Générer un compte rendu/))
    expect(screen.queryByRole('option', { name: 'Ancien modèle 4B' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Modèle du compte rendu')).toHaveValue('baseline')
    expect(screen.queryByRole('button', { name: /Installer ce modèle/ })).not.toBeInTheDocument()
    expect(p.onInstall).not.toHaveBeenCalled()
  })

  it('préserve la permission explicite du moteur et refuse un type invalide', () => {
    const status = normalizeModelStatus({ installed: true, missing: [], report_models: [retired] })
    expect(status.report_models?.[0]).toMatchObject({ available_for_new_jobs: false })
    expect(() => normalizeModelStatus({ installed: true, missing: [],
      report_models: [{ ...retired, available_for_new_jobs: 'yes' }],
    })).toThrow(/Catalogue/)
  })

  it('retire aussi une ancienne option sans permission explicite', async () => {
    const p = props()
    const { available_for_new_jobs: _permission, ...legacy } = retired
    const user = userEvent.setup()
    render(<StartForm {...p} reportModels={[baseline, legacy]} />)
    await user.click(screen.getByLabelText(/Générer un compte rendu/))
    expect(screen.queryByRole('option', { name: 'Ancien modèle 4B' })).not.toBeInTheDocument()
  })
})
