const selector = 'button:not(:disabled), a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])'
const stack: HTMLElement[] = []
const previousInert = new Map<HTMLElement, boolean>()
let previousOverflow = ''

function isolate() {
  const top = stack.at(-1)
  for (const child of document.body.children) {
    if (!(child instanceof HTMLElement)) continue
    if (!previousInert.has(child)) previousInert.set(child, child.inert)
    child.inert = !!top && child !== top
  }
}

export function containDialog(overlay: HTMLElement, dialog: HTMLElement, close: () => void) {
  const trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null
  if (!stack.length) previousOverflow = document.body.style.overflow
  stack.push(overlay)
  document.body.style.overflow = 'hidden'
  isolate()
  const observer = new MutationObserver(isolate)
  observer.observe(document.body, { childList: true })
  const focusable = () => [...dialog.querySelectorAll<HTMLElement>(selector)].filter(el => el.tabIndex >= 0 && el.getClientRects().length && !el.closest('[inert]'))
  const initial = () => (focusable()[0] ?? dialog).focus()
  if (!dialog.contains(document.activeElement)) initial()
  const key = (e: KeyboardEvent) => {
    if (stack.at(-1) !== overlay || e.defaultPrevented) return
    if (e.key === 'Escape') {
      if (dialog.querySelector('[aria-expanded="true"]')) return
      e.preventDefault()
      e.stopPropagation()
      close()
    }
    if (e.key === 'Tab') {
      const items = focusable()
      const index = items.indexOf(document.activeElement as HTMLElement)
      if (!items.length || index < 0 || (!e.shiftKey && index === items.length - 1) || (e.shiftKey && index === 0)) {
        e.preventDefault()
        ;(e.shiftKey ? items.at(-1) ?? dialog : items[0] ?? dialog).focus()
      }
    }
  }
  const focus = (e: FocusEvent) => {
    if (stack.at(-1) === overlay && !dialog.contains(e.target as Node)) initial()
  }
  document.addEventListener('keydown', key)
  document.addEventListener('focusin', focus)
  return () => {
    observer.disconnect()
    document.removeEventListener('keydown', key)
    document.removeEventListener('focusin', focus)
    stack.splice(stack.indexOf(overlay), 1)
    if (stack.length) isolate()
    else {
      for (const [el, inert] of previousInert) el.inert = inert
      previousInert.clear()
      document.body.style.overflow = previousOverflow
    }
    if (trigger?.isConnected && !trigger.closest('[inert]')) trigger.focus()
  }
}
