import { useEffect, useState } from 'react'
import { Button, Caption, Matrix } from '@/ui'
import { PadArtView, PadBadge } from '@/controller/PadArtView'
import { usePadStore } from '@/hooks/usePads'
import { backOf, confirmOf, familyOf, padLabel } from '@/lib/pad'
import type { PadControl } from '@/lib/pad-art'

/**
 * The first time controller support is on for an account: a card at the top
 * of the Library saying so, with the pad drawn and its buttons lighting in
 * turn, how to move around, and the way into Big Picture. It was only in
 * Settings before, where nobody looks for it. Once dismissed, it stays gone.
 */
const SEEN = 'kryoto.pad-welcome'

/** The buttons the drawing lights, one after another. */
const TOUR: PadControl[] = ['dpup', 'dpright', 'dpdown', 'dpleft', 'a', 'b', 'leftshoulder', 'rightshoulder', 'leftstick', 'rightstick']

export function ControllerWelcome({ onBigPicture, onSetup }: { onBigPicture: () => void; onSetup: () => void }) {
  const { on, pads, config } = usePadStore()
  const [seen, setSeen] = useState(() => {
    try {
      return localStorage.getItem(SEEN) === '1'
    } catch {
      return false
    }
  })
  const [beat, setBeat] = useState(0)
  useEffect(() => {
    const t = window.setInterval(() => setBeat((n) => n + 1), 520)
    return () => window.clearInterval(t)
  }, [])
  if (!on || seen) return null

  const pad = pads[0] ?? null
  const family = familyOf(pad, pad ? config?.pads[pad.guid] : null)
  const lit = TOUR[beat % TOUR.length]!
  const dismiss = () => {
    try {
      localStorage.setItem(SEEN, '1')
    } catch {
      /* shown again next time, nothing worse */
    }
    setSeen(true)
  }
  const key = (c: PadControl) => <PadBadge family={family} control={c} label={padLabel(family, c).long} />

  return (
    <section
      aria-label="Controller support"
      className="kryo-radius kryo-in grid items-center gap-6 border border-border bg-card/60 p-5 md:grid-cols-[minmax(0,320px)_1fr]"
      style={{ backgroundImage: 'radial-gradient(closest-side at 22% 50%, color-mix(in oklab, var(--foreground) 8%, transparent), transparent)' }}
    >
      <PadArtView family={family} pressed={new Set([lit])} focus={lit} className="mx-auto max-w-[320px]" />
      <div className="grid gap-3">
        <Caption className="flex items-center gap-2">
          <Matrix state={pad ? 'online' : 'wait'} className="size-3 text-primary" />
          {pad ? `${pad.name} is connected` : 'Early beta'}
        </Caption>
        <h2 className="text-xl font-bold text-foreground">Controller support is on</h2>
        <p className="max-w-xl text-xs leading-relaxed text-muted-foreground">
          Play and move around Kryoto with a controller. Big Picture is Kryoto for the couch: your games big and a press away.
        </p>
        <ul className="flex flex-wrap gap-x-5 gap-y-2 text-[11px] text-muted-foreground">
          <li className="flex items-center gap-1.5">{key('dpup')}Move</li>
          <li className="flex items-center gap-1.5">{key(confirmOf(family))}Select</li>
          <li className="flex items-center gap-1.5">{key(backOf(family))}Back</li>
          <li className="flex items-center gap-1.5">
            {key('leftshoulder')}
            {key('rightshoulder')}
            Pages
          </li>
          <li className="flex items-center gap-1.5">{key('guide')}Big Picture</li>
        </ul>
        <div className="flex flex-wrap gap-2 pt-1">
          <Button variant="primary" onClick={onBigPicture}>
            Open Big Picture
          </Button>
          <Button onClick={onSetup}>{pad && !pad.mapped ? 'Set up my controller' : 'Controller settings'}</Button>
          <Button variant="ghost" onClick={dismiss}>
            Got it
          </Button>
        </div>
      </div>
    </section>
  )
}
