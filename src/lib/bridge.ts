import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

/**
 * The one door to the native side.
 *
 * In the desktop app every call is a Tauri command and every event a Tauri
 * event. In a plain browser (`pnpm dev`, for working on the UI) the same calls
 * are answered by `preview.ts` with a small in-memory library, so every screen
 * can be opened and clicked through without building the app.
 */

export function isTauri() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/**
 * The web chat (chat.kryo.to, src/web) answers the same calls in the
 * browser, with km-core compiled to WebAssembly. It registers itself here;
 * the desktop app never does.
 */
export type Backend = {
  call: (command: string, args: Record<string, unknown>) => Promise<unknown>
  on: (event: string, handler: (payload: unknown) => void) => () => void
}
let web: Backend | null = null
let preview: Promise<typeof import('./preview')> | null = null
function previewBackend() {
  return preview ??= import('./preview').then((backend) => {
    backend.startPreview()
    return backend
  })
}
export function useWebBackend(b: Backend) {
  web = b
}
export function isWeb() {
  return web !== null
}

export function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (web) return web.call(command, args ?? {}) as Promise<T>
  return isTauri() ? invoke<T>(command, args) : previewBackend().then(b => b.previewCall<T>(command, args))
}

export function on<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (web) return Promise.resolve(web.on(event, handler as (p: unknown) => void))
  if (isTauri()) return listen<T>(event, (e) => handler(e.payload))
  return previewBackend().then(b => b.previewBus.on(event, handler as (p: unknown) => void))
}

/** Errors from Rust arrive as strings; everything else as Errors. */
export function errorText(e: unknown): string {
  if (typeof e === 'string') return e
  if (e instanceof Error) return e.message
  try {
    return JSON.stringify(e)
  } catch {
    return 'Something went wrong.'
  }
}
