import type { MouseEvent, KeyboardEvent } from 'react'

/** Right click and the keyboard menu key share the same menu and focus target. */
export function gameContext(open?: (x: number, y: number) => void) {
  const show = (e: MouseEvent<HTMLElement> | KeyboardEvent<HTMLElement>) => {
    if (!open || (e.target as Element).closest('input, textarea, [role="menu"]')) return
    e.preventDefault()
    e.stopPropagation()
    const target = (e.target as Element).closest<HTMLElement>('button, a[href]')
    const trigger = target && e.currentTarget.contains(target) ? target : e.currentTarget
    trigger.focus()
    const box = trigger.getBoundingClientRect()
    open('clientX' in e && e.clientX ? e.clientX : box.left + 12, 'clientY' in e && e.clientY ? e.clientY : box.bottom)
  }
  return {
    onContextMenu: show,
    onKeyDown: (e: KeyboardEvent<HTMLElement>) => {
      if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) show(e)
    },
  }
}
