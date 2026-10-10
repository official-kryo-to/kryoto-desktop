import { isValidElement, type ReactNode } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { call, isTauri, on } from '@/lib/bridge'
import type { Inbox } from '@/hooks/useInbox'

/**
 * Menus over the Store.
 *
 * The Store is a native web view on top of the shell, so a menu drawn in the
 * shell's page would open *under* it. In the app, menus are drawn by one more
 * web view inside the main window, stacked above the Store (`menus.rs`). This
 * side sends what to draw and where, runs whatever gets picked, and keeps
 * track of which menu is open so its trigger can light up.
 */

export type PopupItem =
  | { id: string; label: string; hint?: string; icon?: string; danger?: boolean; disabled?: boolean; checked?: boolean }
  | { separator: true }
  | { heading: string }

export type PopupLook = { palette: string; radius: string; font: string | undefined; theme: 'light' | 'dark' }

export type PopupPayload =
  | { menu: string; kind: 'menu'; items: PopupItem[]; minWidth?: number; look: PopupLook }
  | { menu: string; kind: 'inbox'; inbox: Inbox; look: PopupLook }

/** A menu entry the shell hands over: the same shape the page menus use. */
export type NativeEntry =
  | { label: string; onSelect: () => void; hint?: string; disabled?: boolean; danger?: boolean; icon?: ReactNode; checked?: boolean }
  | { separator: true }
  | { heading: string }

/** Where a menu hangs from: its trigger's box, in the main window. */
export type Anchor = { left: number; right: number; bottom: number }

let handlers = new Map<string, () => void>()
let current: string | null = null
const listeners = new Set<(open: string | null) => void>()
const hoverListeners = new Set<(inside: boolean) => void>()
let wired = false
/** When a menu last closed, so the click on its own trigger that closed it does not reopen it. */
let lastClosed: { menu: string | null; at: number } = { menu: null, at: 0 }

function setCurrent(menu: string | null) {
  if (current === menu) return
  current = menu
  listeners.forEach((l) => l(menu))
}

function wire() {
  if (wired || !isTauri()) return
  wired = true
  void on<string>('menu-pick', (id) => handlers.get(id)?.())
  void on<{ menu: string | null }>('menu-closed', ({ menu }) => {
    lastClosed = { menu, at: Date.now() }
    if (!menu || menu === current) setCurrent(null)
  })
  void on<boolean>('menu-hover', (inside) => hoverListeners.forEach((l) => l(inside)))
  // A press anywhere in the shell outside the open menu's trigger closes it.
  // Triggers handle their own press (a second click closes, see MenuButton).
  window.addEventListener(
    'pointerdown',
    (e) => {
      if (!current) return
      const trigger = (e.target as Element | null)?.closest?.('[data-menu]')
      if (trigger?.getAttribute('data-menu') === current) return
      closeMenu()
    },
    true,
  )
  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && current) closeMenu()
  })
}

function look(): PopupLook {
  const d = document.documentElement.dataset
  return { palette: d.palette ?? 'monochrome', radius: d.radius ?? 'pill', font: d.font, theme: document.documentElement.classList.contains('dark') ? 'dark' : 'light' }
}

function iconMarkup(icon: ReactNode): string | undefined {
  if (!isValidElement(icon)) return undefined
  try {
    return renderToStaticMarkup(icon)
  } catch {
    return undefined
  }
}

function anchorOf(r: DOMRect | Anchor): Anchor {
  return { left: r.left, right: r.right, bottom: r.bottom }
}

export function popupOpen() {
  return current
}

/** Whether `menu` closed a moment ago - its trigger was the click that closed it. */
export function justClosed(menu: string) {
  return lastClosed.menu === menu && Date.now() - lastClosed.at < 300
}

export async function openMenu(menu: string, anchor: DOMRect | Anchor, entries: NativeEntry[], align: 'left' | 'right' = 'left', minWidth?: number) {
  wire()
  handlers = new Map()
  const items: PopupItem[] = entries.map((e, i) => {
    if ('separator' in e || 'heading' in e) return e
    const id = `${menu}:${i}`
    handlers.set(id, e.onSelect)
    return { id, label: e.label, hint: e.hint, danger: e.danger, disabled: e.disabled, checked: e.checked, icon: iconMarkup(e.icon) }
  })
  setCurrent(menu)
  await call('menu_open', { scale: window.devicePixelRatio || null, anchor: anchorOf(anchor), right: align === 'right', payload: { menu, kind: 'menu', items, minWidth, look: look() } }).catch(
    () => setCurrent(null),
  )
}

export async function openInbox(
  menu: string,
  anchor: DOMRect | Anchor,
  inbox: Inbox,
  actions: { open: (url: string | null) => void; markRead: () => void; all: () => void },
) {
  wire()
  handlers = new Map()
  inbox.notifications.forEach((n, i) => handlers.set(`${menu}:open:${i}`, () => actions.open(n.url)))
  handlers.set(`${menu}:read`, actions.markRead)
  handlers.set(`${menu}:all`, actions.all)
  setCurrent(menu)
  await call('menu_open', { scale: window.devicePixelRatio || null, anchor: anchorOf(anchor), right: true, payload: { menu, kind: 'inbox', inbox, look: look() } }).catch(() =>
    setCurrent(null),
  )
}

export function closeMenu() {
  if (!current) return
  lastClosed = { menu: current, at: Date.now() }
  setCurrent(null)
  void call('menu_close').catch(() => {})
}

/** Which menu is open, as it changes. */
export function onPopupChange(fn: (open: string | null) => void) {
  wire()
  listeners.add(fn)
  return () => {
    listeners.delete(fn)
  }
}

/** The pointer entering or leaving the menu, for menus that open on hover. */
export function onPopupHover(fn: (inside: boolean) => void) {
  wire()
  hoverListeners.add(fn)
  return () => {
    hoverListeners.delete(fn)
  }
}
