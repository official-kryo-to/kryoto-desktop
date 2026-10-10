import { useEffect, useState } from 'react'
import { PadBadge } from '@/controller/PadArtView'
import { backOf, confirmOf, padLabel } from '@/lib/pad'
import type { PadControl, PadFamily } from '@/lib/pad-art'

/**
 * While a controller is the thing in use (lib/pad-nav.ts marks the page
 * `kryo-pad`, the mouse clears it): a strip above the bottom bar naming what
 * each button does here, the way this pad prints them. In the Store too,
 * where the page moves by itself but the buttons are the same.
 */
export function PadHints({ family, store }: { family: PadFamily; store: boolean }) {
  const [active, setActive] = useState(() => document.documentElement.classList.contains('kryo-pad'))
  useEffect(() => {
    const html = document.documentElement
    const o = new MutationObserver(() => setActive(html.classList.contains('kryo-pad')))
    o.observe(html, { attributes: true, attributeFilter: ['class'] })
    return () => o.disconnect()
  }, [])
  if (!active) return null
  const item = (controls: PadControl[], what: string) => (
    <span className="flex items-center gap-1.5">
      {controls.map((c) => (
        <PadBadge key={c} family={family} control={c} label={padLabel(family, c).long} />
      ))}
      {what}
    </span>
  )
  return (
    <div aria-hidden className="kryo-in flex h-9 shrink-0 items-center justify-center gap-6 border-t border-border bg-card/60 text-[11px] text-muted-foreground">
      {item(['dpup'], 'Move')}
      {item([confirmOf(family)], 'Select')}
      {item([backOf(family)], 'Back')}
      {store ? item(['y'], 'Search') : null}
      {item(['leftshoulder', 'rightshoulder'], 'Pages')}
      {item(['guide'], 'Big Picture')}
    </div>
  )
}
