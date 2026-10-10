import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Button, Matrix, Modal } from '@/ui'
import { on } from '@/lib/bridge'
import { padApi, padLabel, type PadAxesEvent, type PadInfo, type PadRawEvent, type PadRawState } from '@/lib/pad'
import type { PadControl, PadFamily } from '@/lib/pad-art'
import { PadArtView, type Pointer } from '@/controller/PadArtView'
import { savePadConfig } from '@/hooks/usePads'
import { cn } from '@/lib/utils'

/**
 * Setting up a controller, one control at a time.
 *
 * Kryoto shows a control on the drawing (blinking, or a stick moving the way
 * to push it) and listens to the pad's raw buttons, axes and hats
 * (`pad_capture`). Whatever moves is that control: a button, a hat
 * direction, an axis or half of one. A trigger that rests at -1 is told from
 * a stick that rests at 0, and an input already given to another control is
 * refused. Between steps everything has to be let go, so one press is never
 * read twice. The result is an SDL mapping for this model (`pad_mapping_set`),
 * which Kryoto, SDL games and the virtual Xbox controller then use.
 */

type Kind = 'press' | 'trigger' | 'push-right' | 'push-down'
type Step = { key: string; control: PadControl; kind: Kind; optional?: boolean }

/** In the order a person holding a pad finds them. */
const STEPS: Step[] = [
  { key: 'a', control: 'a', kind: 'press' },
  { key: 'b', control: 'b', kind: 'press' },
  { key: 'x', control: 'x', kind: 'press' },
  { key: 'y', control: 'y', kind: 'press' },
  { key: 'dpup', control: 'dpup', kind: 'press' },
  { key: 'dpdown', control: 'dpdown', kind: 'press' },
  { key: 'dpleft', control: 'dpleft', kind: 'press' },
  { key: 'dpright', control: 'dpright', kind: 'press' },
  { key: 'leftshoulder', control: 'leftshoulder', kind: 'press' },
  { key: 'rightshoulder', control: 'rightshoulder', kind: 'press' },
  { key: 'lefttrigger', control: 'lefttrigger', kind: 'trigger' },
  { key: 'righttrigger', control: 'righttrigger', kind: 'trigger' },
  { key: 'leftx', control: 'leftstick', kind: 'push-right' },
  { key: 'lefty', control: 'leftstick', kind: 'push-down' },
  { key: 'leftstick', control: 'leftstick', kind: 'press' },
  { key: 'rightx', control: 'rightstick', kind: 'push-right' },
  { key: 'righty', control: 'rightstick', kind: 'push-down' },
  { key: 'rightstick', control: 'rightstick', kind: 'press' },
  { key: 'back', control: 'back', kind: 'press' },
  { key: 'start', control: 'start', kind: 'press' },
  { key: 'guide', control: 'guide', kind: 'press', optional: true },
]

const WHERE: Partial<Record<string, string>> = {
  a: 'The bottom face button',
  b: 'The right face button',
  x: 'The left face button',
  y: 'The top face button',
  back: 'The small button left of the middle',
  start: 'The small button right of the middle',
  guide: 'The logo button in the middle. Skip it if yours has none.',
  lefttrigger: 'All the way',
  righttrigger: 'All the way',
}

function prompt(step: Step, family: PadFamily): string {
  const label = padLabel(family, step.control)
  const the = (s: string) => `the ${s.charAt(0).toLowerCase()}${s.slice(1)}`
  switch (step.key) {
    case 'dpup':
      return 'Press up on the d-pad'
    case 'dpdown':
      return 'Press down on the d-pad'
    case 'dpleft':
      return 'Press left on the d-pad'
    case 'dpright':
      return 'Press right on the d-pad'
    case 'leftx':
      return 'Push the left stick right'
    case 'lefty':
      return 'Push the left stick down'
    case 'rightx':
      return 'Push the right stick right'
    case 'righty':
      return 'Push the right stick down'
    case 'leftstick':
      return 'Click the left stick in'
    case 'rightstick':
      return 'Click the right stick in'
  }
  if (step.kind === 'trigger') return `Pull ${the(label.long)}`
  if (['a', 'b', 'x', 'y', 'back', 'start', 'guide'].includes(step.key)) return `Press ${label.short}`
  return `Press ${the(label.long)}`
}

/** What a source claims: a whole axis, half of one, a button or a hat direction. */
function claims(src: string): string[] {
  const m = /^([+-]?)a(\d+)(~?)$/.exec(src)
  if (!m) return [src]
  return m[1] ? [`${m[1]}a${m[2]}`] : [`+a${m[2]}`, `-a${m[2]}`]
}
const clash = (a: string, b: string) => claims(a).some((c) => claims(b).includes(c))

type Phase = 'intro' | 'step' | 'done' | 'saving'

export function PadSetup({ pad, family, onClose }: { pad: PadInfo; family: PadFamily; onClose: (saved: boolean) => void }) {
  const [phase, setPhase] = useState<Phase>('intro')
  const [index, setIndex] = useState(0)
  const [bound, setBound] = useState<Record<string, string>>({})
  const [skipped, setSkipped] = useState<Set<string>>(new Set())
  const [note, setNote] = useState<string | null>(null)
  const [got, setGot] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [ready, setReady] = useState(false)
  const [releasing, setReleasing] = useState(false)
  const [blink, setBlink] = useState(false)
  const [beat, setBeat] = useState(0)
  const step = STEPS[index]!

  // What the pad is doing right now, and where it rests.
  const live = useRef({ buttons: new Set<number>(), axes: new Map<number, number>(), hats: new Map<number, number>() })
  const rest = useRef<{ axes: number[]; hats: number[] }>({ axes: [], hats: [] })
  // After each answer everything must be let go before the next is heard.
  const waitRelease = useRef(true)
  const advancing = useRef(false)
  const nextStepTimer = useRef<number | undefined>(undefined)
  useEffect(() => () => window.clearTimeout(nextStepTimer.current), [])
  const state = useRef({ phase, index, bound })
  state.current = { phase, index, bound }

  // The drawing breathes: the asked control blinks, a stick shows the way.
  useEffect(() => {
    const t = window.setInterval(() => {
      setBlink((b) => !b)
      setBeat((n) => n + 1)
    }, 420)
    return () => window.clearInterval(t)
  }, [])

  const neutral = () => {
    const l = live.current
    if (l.buttons.size) return false
    for (const [i, v] of l.axes) if (Math.abs(v - (rest.current.axes[i] ?? 0)) > 0.35) return false
    for (const [, v] of l.hats) if (v) return false
    return true
  }

  const accept = useCallback((src: string) => {
    if (advancing.current) return
    const { index: i, bound: b } = state.current
    const s = STEPS[i]!
    const other = Object.entries(b).find(([k, v]) => k !== s.key && clash(v, src))
    if (other) {
      const owner = STEPS.find((x) => x.key === other[0])
      setNote(`That one is already ${owner ? prompt(owner, family).replace(/^(Press|Pull|Push|Click) /, '') : other[0]}. Try another, or skip.`)
      return
    }
    setNote(null)
    setBound((cur) => ({ ...cur, [s.key]: src }))
    setGot(s.key)
    advancing.current = true
    waitRelease.current = true
    void padApi.haptic(pad.id, 'tick').catch(() => {})
    nextStepTimer.current = window.setTimeout(() => {
      advancing.current = false
      setGot(null)
      if (i + 1 < STEPS.length) setIndex(i + 1)
      else setPhase('done')
    }, 380)
  }, [family, pad.id])

  /** What a raw input means for the step being asked, if anything. */
  const read = useCallback((ev: PadRawEvent): string | null => {
    const s = STEPS[state.current.index]!
    const base = rest.current.axes[ev.index] ?? 0
    const moved = ev.kind === 'axis' && Math.abs(ev.value - base) > 0.6
    if (s.kind === 'push-right' || s.kind === 'push-down') {
      // A stick rests in the middle; something resting at an end is a trigger.
      if (!moved || Math.abs(base) > 0.5) return null
      return ev.value > base ? `a${ev.index}` : `a${ev.index}~`
    }
    if (ev.kind === 'button') return ev.value > 0 ? `b${ev.index}` : null
    if (ev.kind === 'hat') return [1, 2, 4, 8].includes(ev.value) ? `h${ev.index}.${ev.value}` : null
    if (!moved) return null
    if (s.kind === 'trigger') {
      if (base < -0.5) return ev.value > base ? `a${ev.index}` : null
      if (base > 0.5) return ev.value < base ? `a${ev.index}~` : null
    }
    return ev.value > base ? `+a${ev.index}` : `-a${ev.index}`
  }, [])

  // Listen to the pad's raw inputs while this is open.
  useEffect(() => {
    let cancelled = false
    const stops: (() => void)[] = []
    const keep = (fn: () => void) => (cancelled ? fn() : stops.push(fn))
    const subscriptions = [on<PadRawState>('pad-raw-state', (s) => {
      if (s.id !== pad.id) return
      rest.current = { axes: s.axes, hats: s.hats }
      live.current.axes = new Map(s.axes.map((v, i) => [i, v]))
      live.current.buttons = new Set((s.buttons ?? []).flatMap((held, i) => held ? [i] : []))
      live.current.hats = new Map(s.hats.map((v, i) => [i, v]))
      setReady(true)
    }).then(keep), on<PadRawEvent>('pad-raw', (ev) => {
      if (ev.id !== pad.id) return
      const l = live.current
      if (ev.kind === 'button') {
        if (ev.value > 0) l.buttons.add(ev.index)
        else l.buttons.delete(ev.index)
      } else if (ev.kind === 'axis') l.axes.set(ev.index, ev.value)
      else l.hats.set(ev.index, ev.value)
      if (state.current.phase !== 'step') return
      if (advancing.current) return
      if (waitRelease.current) {
        if (neutral()) { waitRelease.current = false; setReleasing(false) }
        return
      }
      const src = read(ev)
      if (src) accept(src)
    }).then(keep)]
    // The resting-state reply must not beat the event subscriptions.
    void Promise.all(subscriptions).then(() => {
      if (!cancelled) return padApi.capture(pad.id)
    }).catch(e => setError(String(e)))
    return () => {
      cancelled = true
      stops.forEach((f) => f())
      void padApi.capture(null).catch(() => {})
    }
  }, [pad.id, read, accept])

  // A new step listens once everything is let go.
  useEffect(() => {
    waitRelease.current = !neutral()
    setReleasing(waitRelease.current)
    setNote(null)
  }, [index, phase])

  const go = (to: number) => {
    window.clearTimeout(nextStepTimer.current)
    advancing.current = false
    setNote(null)
    setGot(null)
    if (to < 0) return setPhase('intro')
    if (to >= STEPS.length) return setPhase('done')
    setIndex(to)
  }
  const begin = () => {
    setBound({})
    setSkipped(new Set())
    setError(null)
    setIndex(0)
    setPhase('step')
  }
  const back = () => {
    const prev = index - 1
    if (prev >= 0) {
      const key = STEPS[prev]!.key
      setBound((cur) => {
        const next = { ...cur }
        delete next[key]
        return next
      })
      setSkipped((s) => {
        const n = new Set(s)
        n.delete(key)
        return n
      })
    }
    go(prev)
  }
  const skip = () => {
    setSkipped((s) => new Set(s).add(step.key))
    go(index + 1)
  }

  const save = async () => {
    setPhase('saving')
    setError(null)
    try {
      await savePadConfig(await padApi.saveSetup(pad.guid, pad.name, bound))
      void padApi.haptic(pad.id, 'select').catch(() => {})
      onClose(true)
    } catch (e) {
      setError(typeof e === 'string' ? e : e instanceof Error ? e.message : 'The setup could not be saved.')
      setPhase('done')
    }
  }

  // What the drawing shows.
  const art = useMemo(() => {
    // The intro lights each control in turn, the way the setup will go.
    if (phase === 'intro') {
      const c = STEPS[beat % STEPS.length]!.control
      return { pressed: new Set<PadControl>([c]), axes: null, focus: c, pointer: null }
    }
    if (phase !== 'step') return { pressed: new Set<PadControl>(), axes: null, focus: null, pointer: null }
    const held = got === step.key
    const lit = held || blink
    const axes: PadAxesEvent = { id: pad.id, lx: 0, ly: 0, rx: 0, ry: 0, lt: 0, rt: 0 }
    const pressed = new Set<PadControl>()
    if (step.kind === 'trigger') axes[step.key === 'lefttrigger' ? 'lt' : 'rt'] = lit ? 1 : 0
    else if (step.kind === 'push-right') axes[step.control === 'leftstick' ? 'lx' : 'rx'] = lit ? 1 : 0
    else if (step.kind === 'push-down') axes[step.control === 'leftstick' ? 'ly' : 'ry'] = lit ? -1 : 0
    else if (lit) pressed.add(step.control)
    const pointer: Pointer | null =
      held ? null : { control: step.control, way: step.kind === 'push-right' ? 'right' : step.kind === 'push-down' ? 'down' : 'press' }
    return { pressed, axes, focus: step.control, pointer }
  }, [phase, step, blink, beat, got, pad.id])

  const count = Object.keys(bound).length
  const title = `Set up ${pad.name}`

  return (
    <Modal
      title={title}
      onClose={() => onClose(false)}
      wide
      footer={
        phase === 'intro' ? (
          <div className="flex w-full items-center justify-between gap-3">
            <span className="text-[11px] text-muted-foreground">About a minute. Nothing changes until you save.</span>
            <Button variant="primary" onClick={begin} disabled={!ready}>
              {ready ? 'Start setup' : 'Connecting…'}
            </Button>
          </div>
        ) : phase === 'step' ? (
          <div className="flex w-full items-center justify-between gap-3">
            <Button variant="ghost" size="sm" onClick={back}>
              Back
            </Button>
            <Button size="sm" onClick={skip} disabled={step.key === 'a'} title={step.key === 'a' ? 'Every controller needs this one.' : undefined}>
              {step.optional ? 'My controller has none' : 'Skip'}
            </Button>
          </div>
        ) : (
          <div className="flex w-full items-center justify-between gap-3">
            <Button variant="ghost" size="sm" onClick={begin} disabled={phase === 'saving'}>
              Start over
            </Button>
            <Button variant="primary" onClick={() => void save()} disabled={phase === 'saving' || !bound.a}>
              {phase === 'saving' ? 'Saving' : 'Save and test'}
            </Button>
          </div>
        )
      }
    >
      <div className="grid h-full content-center justify-items-center gap-5 px-6 py-4">
        <Progress total={STEPS.length} index={phase === 'step' ? index : phase === 'intro' ? -1 : STEPS.length} bound={bound} skipped={skipped} />

        <div className="relative w-full max-w-[460px]">
          {phase === 'done' || phase === 'saving' ? (
            <div className="grid place-items-center py-8">
              <Matrix
                state={phase === 'saving' ? 'busy' : error ? 'error' : 'success'}
                className={cn('size-20', error ? 'text-destructive' : phase === 'saving' ? 'text-muted-foreground' : 'text-success')}
                announce
              />
            </div>
          ) : (
            <PadArtView family={family} pressed={art.pressed} axes={art.axes} focus={art.focus} pointer={art.pointer} />
          )}
        </div>

        <div className="grid min-h-20 justify-items-center gap-1.5 text-center" aria-live="polite">
          {phase === 'intro' ? (
            <>
              <p className="text-lg font-bold text-foreground">Follow the highlighted button</p>
              <p className="max-w-md text-xs leading-relaxed text-muted-foreground">
                Put the controller down so the sticks and triggers rest. Press Start setup, then press each highlighted button once and let go. Skip controls your controller does not have. Save and test when finished.
              </p>
            </>
          ) : phase === 'step' ? (
            <>
              <p className={cn('flex items-center gap-2 text-lg font-bold', got === step.key ? 'text-success' : 'text-foreground')}>
                {got === step.key ? <Matrix key={step.key} state="success" className="size-4" /> : null}
                {got === step.key ? 'Got it' : prompt(step, family)}
              </p>
              <p className={cn('text-xs', note ? 'text-warning' : 'text-muted-foreground')}>{note ?? (releasing ? 'Let go of the buttons and centre both sticks to continue.' : WHERE[step.key] ?? 'Press once, then let go.')}</p>
              <p className="mt-1 flex items-center gap-2 text-[10px] uppercase tracking-wider text-muted-foreground">
                <Matrix state="scan" className="size-3 text-primary" />
                Listening to your controller
              </p>
            </>
          ) : (
            <>
              <p className="text-lg font-bold text-foreground">{error ? 'Not saved' : 'All set'}</p>
              <p className="max-w-md text-xs text-muted-foreground">
                {error ??
                  `${count} controls ready${skipped.size ? `, ${skipped.size} skipped` : ''}. Press Save and test, then try the buttons on the controller drawing.`}
              </p>
            </>
          )}
        </div>
      </div>
    </Modal>
  )
}

/**
 * One 3x3 matrix per control, Kryoto's own status marks: a tick resolves for
 * each one set, a slash for one skipped, the one being asked scans, the rest
 * wait empty. Beside it, the whole setup as a filling matrix.
 */
function Progress({ total, index, bound, skipped }: { total: number; index: number; bound: Record<string, string>; skipped: Set<string> }) {
  const done = Math.max(0, Math.min(index, total))
  return (
    <div className="grid justify-items-center gap-2.5">
      <div className="flex flex-wrap justify-center gap-1.5" aria-hidden>
        {STEPS.map((s, i) => (
          <Matrix
            key={s.key}
            state={bound[s.key] ? 'success' : skipped.has(s.key) ? 'unavailable' : i === index ? 'scan' : 'empty'}
            className={cn('size-3', bound[s.key] ? 'text-success' : i === index ? 'text-primary' : 'text-muted-foreground')}
          />
        ))}
      </div>
      <span className="flex items-center gap-2 text-[10px] uppercase tracking-wider text-muted-foreground">
        <Matrix progress={done / total} className="size-3 text-foreground" label={`${done} of ${total}`} />
        {index < 0 ? `${total} steps` : index >= total ? 'Done' : `Step ${index + 1} of ${total}`}
      </span>
    </div>
  )
}
