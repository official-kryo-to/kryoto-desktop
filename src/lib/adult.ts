import { useSyncExternalStore } from 'react'

/**
 * Whether adult games' art is shown, the client-wide switch behind
 * Settings > Interface. Off by default, as on kryo.to: a game the site marks
 * adult is drawn blurred everywhere its art appears - the library, the game
 * page, downloads - until the reader turns this on.
 */
let show = false
const listeners = new Set<() => void>()

export function setShowAdult(next: boolean) {
  if (show === next) return
  show = next
  listeners.forEach((l) => l())
}

export function useShowAdult(): boolean {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => show,
  )
}

/**
 * Whether adult games are left out altogether: the account's "Hide" on
 * kryo.to. Lists of games that come from kryo.to (Community, the games saved
 * on the site, Add a game's search) leave them out; games already on this PC
 * stay in the library, blurred - they are yours, and the library is a list
 * of what is installed, not a catalog.
 */
let hide = false
const hideListeners = new Set<() => void>()

export function setHideAdult(next: boolean) {
  if (hide === next) return
  hide = next
  hideListeners.forEach((l) => l())
}

export function useHideAdult(): boolean {
  return useSyncExternalStore(
    (l) => {
      hideListeners.add(l)
      return () => hideListeners.delete(l)
    },
    () => hide,
  )
}

/** A list from kryo.to without its adult games, when the reader hides them. */
export function withoutAdult<T extends { nsfw?: boolean | null }>(items: readonly T[], hideAdult: boolean): T[] {
  return hideAdult ? items.filter((i) => !i.nsfw) : (items as T[])
}

/** The classes that hide an adult game's art. */
export function adultBlur(nsfw: boolean | undefined, showAdult: boolean): string {
  return nsfw && !showAdult ? 'blur-2xl scale-110 saturate-50' : ''
}
