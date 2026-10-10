import { useCallback, useEffect, useState, useSyncExternalStore } from 'react'
import { on } from '@/lib/bridge'
import { padApi, type PadAxesEvent, type PadButtonEvent, type PadConfig, type PadInfo } from '@/lib/pad'
import type { PadControl } from '@/lib/pad-art'

/**
 * The pads plugged in and Settings > Controller, shared by everything that
 * shows or uses them (the shell's navigation, the settings pane). Filled once
 * the shell starts the feature (`useControllerFeature`).
 */
type Store = {
  pads: PadInfo[]
  config: PadConfig | null
  on: boolean
  /** A pad to open Settings > Controller on (its notification was clicked). */
  requested: { guid: string } | null
}
let store: Store = { pads: [], config: null, on: false, requested: null }
const subs = new Set<() => void>()
const set = (patch: Partial<Store>) => {
  store = { ...store, ...patch }
  subs.forEach((f) => f())
}
const subscribe = (f: () => void) => {
  subs.add(f)
  return () => subs.delete(f)
}

export function usePadStore(): Store {
  return useSyncExternalStore(subscribe, () => store)
}

/** Open Settings > Controller on this pad. */
export function requestPad(guid: string) {
  set({ requested: { guid } })
}

export function clearPadRequest() {
  set({ requested: null })
}

/** Save Settings > Controller; the native side returns what it kept. */
export async function savePadConfig(next: PadConfig) {
  set({ config: next })
  set({ config: await padApi.save(next) })
}

/**
 * Turn controller support on while the account has the `controller` flag,
 * and off again when it loses it (or signs out).
 */
export function useControllerFeature(enabled: boolean) {
  useEffect(() => {
    if (!enabled) return
    let cancelled = false
    let stop: (() => void) | undefined
    // Subscribe before SDL starts; an already-connected pad can arrive at once.
    void on<PadInfo[]>('pad-list', (pads) => {
      if (cancelled) return
      set({ pads })
      void padApi.config().then(config => { if (!cancelled) set({ config }) }).catch(() => {})
    }).then(async fn => {
      if (cancelled) return fn()
      stop = fn
      set({ on: true })
      await padApi.start()
      const config = await padApi.config()
      if (!cancelled) set({ config })
    }).catch(() => {})
    return () => {
      cancelled = true
      stop?.()
      set({ pads: [], on: false })
      void padApi.stop().catch(() => {})
    }
  }, [enabled])
}

/** What is held down on one pad right now, and where its sticks are. */
export function usePadInput(id: number | null) {
  const [pressed, setPressed] = useState<ReadonlySet<PadControl>>(new Set())
  const [axes, setAxes] = useState<PadAxesEvent | null>(null)
  const [last, setLast] = useState<{ control: PadControl; at: number } | null>(null)
  useEffect(() => {
    setPressed(new Set())
    setAxes(null)
    setLast(null)
    if (id === null) return
    let cancelled = false
    const stops: (() => void)[] = []
    const keep = (fn: () => void) => (cancelled ? fn() : stops.push(fn))
    void on<PadButtonEvent>('pad-button', (e) => {
      if (e.id !== id) return
      setPressed((cur) => {
        const next = new Set(cur)
        if (e.pressed) next.add(e.control)
        else next.delete(e.control)
        return next
      })
      if (e.pressed) setLast({ control: e.control, at: Date.now() })
    }).then(keep)
    void on<PadAxesEvent>('pad-axes', (e) => e.id === id && setAxes(e)).then(keep)
    return () => {
      cancelled = true
      stops.forEach((f) => f())
    }
  }, [id])
  const clearLast = useCallback(() => setLast(null), [])
  return { pressed, axes, last, clearLast }
}
