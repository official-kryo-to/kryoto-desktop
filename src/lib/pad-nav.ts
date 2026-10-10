import { useEffect, useRef } from 'react'
import { on } from '@/lib/bridge'
import { backOf, confirmOf, padApi, type PadAxesEvent, type PadButtonEvent, type PadHaptic } from '@/lib/pad'
import type { PadControl, PadFamily } from '@/lib/pad-art'

/**
 * Moving around Kryoto with a controller, the way a console menu works:
 *
 * - D-pad or left stick: move to the nearest thing in that direction (held:
 *   keeps moving).
 * - A (on Nintendo pads, the A on the right): press what is selected.
 * - B: close the open menu or dialog, else go back.
 * - LB / RB: the previous or next page in the top bar.
 * - Home: Big Picture, in or out.
 *
 * A page that has its own idea of back, page and Home (Big Picture) claims
 * them first: `kryo-pad-back`, `kryo-pad-tab` and `kryo-pad-guide` are sent on
 * the window as cancelable events, and only an unclaimed one does the default.
 * - Right stick: scroll.
 *
 * With `haptics`, the pad answers: a light tick as the selection moves, a
 * firmer pulse on select, back and a page change, a bump when there is
 * nothing further that way.
 *
 * While the Store shows (`forward`), the d-pad, A, B, Y and the sticks go to
 * its page instead (kryo.to moves around by itself, as `kryo-pad` events);
 * the bumpers and Home stay with the app.
 *
 * Presses only arrive while Kryoto is in front (pad.rs), so a game being
 * played never drives it. `paused` hands the pad to the page (Settings >
 * Controller, where every button is being tried out); holding back for a
 * moment still leaves.
 */

const FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled]):not([type="hidden"])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
  '[role="menuitem"]',
  '[role="option"]',
].join(',')

type Dir = 'up' | 'down' | 'left' | 'right'
const DPAD: Partial<Record<PadControl, Dir>> = { dpup: 'up', dpdown: 'down', dpleft: 'left', dpright: 'right' }

function shown(el: HTMLElement): boolean {
  const r = el.getBoundingClientRect()
  if (r.width < 2 || r.height < 2) return false
  // Within reach: on screen, or a screen away (it is scrolled to).
  if (r.bottom < -innerHeight || r.top > innerHeight * 2 || r.right < 0 || r.left > innerWidth) return false
  if (el.closest('[inert], [aria-hidden="true"]')) return false
  const s = getComputedStyle(el)
  return s.visibility !== 'hidden' && s.display !== 'none'
}

/** Where moving may go: inside the open menu or dialog, else anywhere. */
function scope(): ParentNode {
  const open = [...document.querySelectorAll<HTMLElement>('[role="menu"], [role="dialog"], [role="listbox"], [aria-modal="true"]')].filter(shown)
  return open.at(-1) ?? document
}

function nearest(from: DOMRect | null, dir: Dir, list: HTMLElement[]): HTMLElement | null {
  if (!from) return list.sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top || a.getBoundingClientRect().left - b.getBoundingClientRect().left)[0] ?? null
  const cx = from.left + from.width / 2
  const cy = from.top + from.height / 2
  let best: HTMLElement | null = null
  let bestScore = Infinity
  for (const el of list) {
    const r = el.getBoundingClientRect()
    const x = r.left + r.width / 2
    const y = r.top + r.height / 2
    const horizontal = dir === 'left' || dir === 'right'
    const ahead = dir === 'right' ? r.left - from.right : dir === 'left' ? from.left - r.right : dir === 'down' ? r.top - from.bottom : from.top - r.bottom
    // Must be on that side (a little overlap allowed, for tight rows).
    const centre = dir === 'right' ? x - cx : dir === 'left' ? cx - x : dir === 'down' ? y - cy : cy - y
    if (centre <= 4 || ahead < -8) continue
    const overlaps = horizontal ? r.top < from.bottom && r.bottom > from.top : r.left < from.right && r.right > from.left
    const side = overlaps ? 0 : horizontal ? Math.abs(y - cy) : Math.abs(x - cx)
    const score = Math.max(ahead, 0) + side * 2.5
    if (score < bestScore) {
      bestScore = score
      best = el
    }
  }
  return best
}

/** Move the selection; false when there is nothing that way. */
function move(dir: Dir): boolean {
  const list = [...scope().querySelectorAll<HTMLElement>(FOCUSABLE)].filter(shown)
  const current = document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null
  const target = nearest(current && list.includes(current) ? current.getBoundingClientRect() : null, dir, list.filter((e) => e !== current))
  if (!target) return false
  document.documentElement.classList.add('kryo-pad')
  target.focus({ preventScroll: true })
  target.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' })
  return true
}

function press() {
  const el = document.activeElement
  if (!(el instanceof HTMLElement) || el === document.body) {
    move('down')
    return
  }
  if (el instanceof HTMLSelectElement) {
    // A select cannot be opened from script: step to its next option.
    el.selectedIndex = (el.selectedIndex + 1) % Math.max(el.options.length, 1)
    el.dispatchEvent(new Event('change', { bubbles: true }))
    return
  }
  if (el instanceof HTMLInputElement && !['checkbox', 'radio', 'button', 'submit'].includes(el.type)) return
  el.click()
}

/** Offer a press to the page first; true when something claimed it. */
function claimed(name: string, detail?: unknown): boolean {
  return !window.dispatchEvent(new CustomEvent(name, { cancelable: true, detail }))
}

/** Close what is open (Escape, as every menu here listens for); else `back`. */
function goBack(back: () => void) {
  if (claimed('kryo-pad-back')) return
  if (scope() !== document) {
    const target = document.activeElement ?? document.body
    target.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true }))
    return
  }
  back()
}

/** The scrollable box the selection is in, or the biggest one on screen. */
function scroller(): HTMLElement | null {
  const scrolls = (el: HTMLElement) => {
    const s = getComputedStyle(el)
    return /(auto|scroll)/.test(s.overflowY + s.overflowX) && (el.scrollHeight > el.clientHeight + 1 || el.scrollWidth > el.clientWidth + 1)
  }
  for (let el = document.activeElement as HTMLElement | null; el && el !== document.body; el = el.parentElement) if (scrolls(el)) return el
  let best: HTMLElement | null = null
  let area = 0
  for (const el of document.querySelectorAll<HTMLElement>('main *')) {
    if (!scrolls(el) || !shown(el)) continue
    const r = el.getBoundingClientRect()
    if (r.width * r.height > area) {
      area = r.width * r.height
      best = el
    }
  }
  return best
}

export function usePadNavigation({
  enabled,
  haptics,
  paused,
  family,
  onBack,
  onTab,
  onGuide,
  forward = false,
}: {
  enabled: boolean
  haptics: boolean
  paused: boolean
  family: PadFamily
  onBack: () => void
  onTab: (step: -1 | 1) => void
  /** Home on the pad: Big Picture. */
  onGuide?: () => void
  /** The Store's page is showing: it gets the moving around. */
  forward?: boolean
}) {
  // The latest of everything, without re-subscribing on each render.
  const live = useRef({ haptics, paused, family, onBack, onTab, onGuide, forward })
  live.current = { haptics, paused, family, onBack, onTab, onGuide, forward }

  useEffect(() => {
    if (!enabled) return
    let cancelled = false
    const stops: (() => void)[] = []
    const keep = (fn: () => void) => (cancelled ? fn() : stops.push(fn))
    const repeat = new Map<Dir, { wait: number; every?: number }>()
    let backHeld: number | undefined
    let stickDir: Dir | null = null
    let lastPad = 0
    // One bump at an edge, not one per repeat while the stick is held there.
    let atEdge = false

    const feel = (kind: PadHaptic) => {
      if (live.current.haptics) void padApi.haptic(lastPad, kind).catch(() => {})
    }
    const step = (dir: Dir) => {
      if (move(dir)) {
        atEdge = false
        feel('tick')
      } else if (!atEdge) {
        atEdge = true
        feel('edge')
      }
    }
    const hold = (dir: Dir) => {
      if (repeat.has(dir)) return
      atEdge = false
      step(dir)
      const r: { wait: number; every?: number } = { wait: 0 }
      r.wait = window.setTimeout(() => (r.every = window.setInterval(() => step(dir), 110)), 380)
      repeat.set(dir, r)
    }
    const release = (dir: Dir) => {
      const r = repeat.get(dir)
      if (!r) return
      window.clearTimeout(r.wait)
      window.clearInterval(r.every)
      repeat.delete(dir)
    }

    void on<PadButtonEvent>('pad-button', ({ id, control, pressed }) => {
      lastPad = id
      const { paused, family, onBack, onTab, onGuide, forward } = live.current
      // The pad is in use: the hint bar and the selection's ring show.
      document.documentElement.classList.add('kryo-pad')
      if (!paused && forward && !['leftshoulder', 'rightshoulder', 'guide'].includes(control)) {
        void padApi.forward({ type: 'button', control, pressed, family }).catch(() => {})
        if (pressed) feel('tick')
        return
      }
      if (paused) {
        // Hold back to leave a page that has the pad.
        if (control === backOf(family)) {
          window.clearTimeout(backHeld)
          if (pressed) backHeld = window.setTimeout(() => onBack(), 800)
        }
        return
      }
      const dir = DPAD[control]
      if (dir) return pressed ? hold(dir) : release(dir)
      if (!pressed) return
      if (control === confirmOf(family)) {
        press()
        feel('select')
      } else if (control === backOf(family)) {
        goBack(onBack)
        feel('select')
      } else if (control === 'leftshoulder' || control === 'rightshoulder') {
        const step = control === 'leftshoulder' ? -1 : 1
        if (!claimed('kryo-pad-tab', step)) onTab(step)
        feel('select')
      } else if (control === 'guide') {
        if (!claimed('kryo-pad-guide')) onGuide?.()
        feel('select')
      }
    }).then(keep)

    void on<PadAxesEvent>('pad-axes', ({ id, lx, ly, rx, ry }) => {
      if (live.current.paused) return
      if (Math.max(Math.abs(lx), Math.abs(ly)) > 0.6) lastPad = id
      if (live.current.forward) {
        void padApi.forward({ type: 'axes', lx, ly, rx, ry, family: live.current.family }).catch(() => {})
        return
      }
      // The left stick as a d-pad, with some give before it lets go.
      const mag = Math.max(Math.abs(lx), Math.abs(ly))
      const dir: Dir | null =
        mag > 0.6 ? (Math.abs(lx) > Math.abs(ly) ? (lx > 0 ? 'right' : 'left') : ly > 0 ? 'up' : 'down') : mag < 0.35 ? null : stickDir
      if (dir !== stickDir) {
        if (stickDir) release(stickDir)
        if (dir) hold(dir)
        stickDir = dir
      }
      // The right stick scrolls.
      if (Math.abs(ry) > 0.2 || Math.abs(rx) > 0.2) {
        const box = scroller()
        box?.scrollBy({ top: -ry * 28, left: rx * 28 })
      }
    }).then(keep)

    // The pad's focus ring goes as soon as the mouse is used again.
    const pointer = () => document.documentElement.classList.remove('kryo-pad')
    window.addEventListener('pointerdown', pointer)
    window.addEventListener('pointermove', pointer)

    return () => {
      cancelled = true
      stops.forEach((f) => f())
      for (const d of [...repeat.keys()]) release(d)
      window.clearTimeout(backHeld)
      window.removeEventListener('pointerdown', pointer)
      window.removeEventListener('pointermove', pointer)
      document.documentElement.classList.remove('kryo-pad')
    }
  }, [enabled])
}
