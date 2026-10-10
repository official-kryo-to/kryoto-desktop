import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/lib/utils'

/**
 * Every `title` in the client as Kryoto's own tooltip, the same one kryo.to
 * draws (kryo.to's components/ui/global-tooltip.tsx; keep the two in step).
 *
 * A mutation observer turns each `title` into `data-kryo-tooltip` as it
 * appears, so the browser's own grey box never shows and no component needs
 * to know. It waits a moment before showing (crossing a row of buttons is not
 * asking about each one), goes away on any click, key, scroll or wheel, and
 * never shows for a finger. SVG elements work too: `data-kryo-tooltip` on a
 * `<g>` is a tooltip like any other.
 */
const HOVER_DELAY_MS = 450

type Active = { target: Element; text: string }

export function GlobalTooltip() {
  const [active, setActive] = useState<Active | null>(null)
  const [present, setPresent] = useState<Active | null>(null)
  const [leaving, setLeaving] = useState(false)
  const [position, setPosition] = useState({ left: 0, top: 0 })
  const ref = useRef<HTMLDivElement>(null)
  const activeRef = useRef<Active | null>(null)
  activeRef.current = active

  useEffect(() => {
    if (active) {
      setPresent(active)
      setLeaving(false)
      return
    }
    if (!present) return
    setLeaving(true)
    const t = setTimeout(() => {
      setPresent(null)
      setLeaving(false)
    }, 80)
    return () => clearTimeout(t)
  }, [active, present])

  useEffect(() => {
    const prepare = (root: ParentNode | Element) => {
      const els: Element[] = []
      if (root instanceof Element && root.hasAttribute('title')) els.push(root)
      els.push(...root.querySelectorAll('[title]'))
      for (const el of els) {
        const title = el.getAttribute('title')?.trim()
        if (!title) continue
        el.setAttribute('data-kryo-tooltip', title)
        el.removeAttribute('title')
        const empty = !el.textContent?.trim()
        if (empty && !el.hasAttribute('aria-label') && (el instanceof HTMLButtonElement || el instanceof HTMLAnchorElement)) {
          el.setAttribute('aria-label', title)
        }
      }
    }
    prepare(document)
    const observer = new MutationObserver((records) => {
      for (const r of records) {
        if (r.type === 'attributes') prepare(r.target as Element)
        for (const n of r.addedNodes) if (n instanceof Element) prepare(n)
      }
    })
    observer.observe(document.documentElement, { subtree: true, childList: true, attributes: true, attributeFilter: ['title'] })

    const targetOf = (v: EventTarget | null) => (v instanceof Element ? v.closest('[data-kryo-tooltip]') : null)
    let timer: ReturnType<typeof setTimeout> | null = null
    const cancel = () => {
      if (timer != null) clearTimeout(timer)
      timer = null
    }
    const open = (target: Element, text: string, delay: number) => {
      cancel()
      if (delay <= 0) return setActive({ target, text })
      timer = setTimeout(() => {
        timer = null
        if (target.isConnected) setActive({ target, text })
      }, delay)
    }
    const onOver = (e: PointerEvent) => {
      if (e.pointerType === 'touch' || e.pointerType === 'pen') return
      const t = targetOf(e.target)
      const text = t?.getAttribute('data-kryo-tooltip')
      if (t && text) open(t, text, activeRef.current ? 90 : HOVER_DELAY_MS)
    }
    const onFocus = (e: Event) => {
      const t = targetOf(e.target)
      const text = t?.getAttribute('data-kryo-tooltip')
      if (!t || !text) return
      try {
        // Keyboard and controller focus, not a click's.
        if (!t.matches(':focus-visible') && !document.documentElement.classList.contains('kryo-pad')) return
      } catch {
        return
      }
      open(t, text, 0)
    }
    const hide = (e: Event) => {
      const from = targetOf(e.target)
      const to = targetOf('relatedTarget' in e ? ((e as PointerEvent).relatedTarget as EventTarget | null) : null)
      if (from && from === to) return
      cancel()
      setActive((cur) => (cur?.target === from ? null : cur))
    }
    const dismiss = () => {
      cancel()
      setActive(null)
    }
    document.addEventListener('pointerover', onOver, true)
    document.addEventListener('pointerout', hide, true)
    document.addEventListener('focusin', onFocus, true)
    document.addEventListener('focusout', hide, true)
    document.addEventListener('pointerdown', dismiss, true)
    document.addEventListener('keydown', dismiss, true)
    document.addEventListener('wheel', dismiss, { capture: true, passive: true })
    document.addEventListener('scroll', dismiss, true)
    return () => {
      observer.disconnect()
      cancel()
      document.removeEventListener('pointerover', onOver, true)
      document.removeEventListener('pointerout', hide, true)
      document.removeEventListener('focusin', onFocus, true)
      document.removeEventListener('focusout', hide, true)
      document.removeEventListener('pointerdown', dismiss, true)
      document.removeEventListener('keydown', dismiss, true)
      document.removeEventListener('wheel', dismiss, true)
      document.removeEventListener('scroll', dismiss, true)
    }
  }, [])

  useLayoutEffect(() => {
    const el = ref.current
    const cur = active ?? present
    if (!cur || !el || !cur.target.isConnected) return
    const a = cur.target.getBoundingClientRect()
    const w = el.offsetWidth
    const h = el.offsetHeight
    const m = 8
    const left = Math.min(Math.max(m, a.left + a.width / 2 - w / 2), Math.max(m, window.innerWidth - w - m))
    const above = a.top - h - m
    const top = above >= m ? above : Math.min(a.bottom + m, Math.max(m, window.innerHeight - h - m))
    setPosition({ left: Math.round(left), top: Math.round(top) })
  }, [active, present])

  if (!present) return null
  return createPortal(
    <div
      ref={ref}
      role="tooltip"
      style={{ left: position.left, top: position.top }}
      className={cn(
        'kryo-global-tooltip kryo-radius fixed z-[900] max-w-64 border border-border bg-popover px-2 py-1 text-[10px] leading-snug text-foreground shadow-xl [overflow-wrap:anywhere]',
        leaving && 'kryo-tooltip-out',
      )}
    >
      {present.text}
    </div>,
    document.body,
  )
}
