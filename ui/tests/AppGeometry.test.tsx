import { act, render } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import App from '../src/App'
import { fakeBackend } from './fakeBackend'

describe('hauteur du bandeau de l’application', () => {
  it.each(['observer', 'resize'])('adapte l’espace du lecteur à la hauteur mesurée (%s)', async mode => {
    let height = 105
    let notify = () => {}
    const disconnect = vi.fn()
    class Observer {
      constructor(callback: ResizeObserverCallback) {
        notify = () => callback([], this as unknown as ResizeObserver)
      }
      observe = vi.fn()
      unobserve = vi.fn()
      disconnect = disconnect
    }
    const previousObserver = Object.getOwnPropertyDescriptor(globalThis, 'ResizeObserver')
    Object.defineProperty(globalThis, 'ResizeObserver', { configurable: true, writable: true, value: mode === 'observer' ? Observer : undefined })
    const original = HTMLElement.prototype.getBoundingClientRect
    const measure = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      return this.classList.contains('topbar') ? new DOMRect(0, 0, 624, height) : original.call(this)
    })
    const { backend } = fakeBackend()
    let view: ReturnType<typeof render> | undefined
    try {
      await act(async () => { view = render(<App backend={backend} />) })
      const app = view!.container.querySelector<HTMLElement>('.app')!
      expect(app.style.getPropertyValue('--reader-top-offset')).toBe('105px')
      height = 113.25
      act(() => { if (mode === 'observer') notify(); else window.dispatchEvent(new Event('resize')) })
      expect(app.style.getPropertyValue('--reader-top-offset')).toBe('114px')
      height = 65
      act(() => { if (mode === 'observer') notify(); else window.dispatchEvent(new Event('resize')) })
      expect(app.style.getPropertyValue('--reader-top-offset')).toBe('65px')
      view!.unmount()
      view = undefined
      if (mode === 'observer') expect(disconnect).toHaveBeenCalledTimes(1)
      const calls = measure.mock.calls.length
      window.dispatchEvent(new Event('resize'))
      expect(measure.mock.calls).toHaveLength(calls)
    } finally {
      view?.unmount()
      measure.mockRestore()
      if (previousObserver) Object.defineProperty(globalThis, 'ResizeObserver', previousObserver)
      else Reflect.deleteProperty(globalThis, 'ResizeObserver')
    }
  })
})
