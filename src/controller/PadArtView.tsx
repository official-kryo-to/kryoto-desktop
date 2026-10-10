import { useMemo } from 'react'
import { FACE, glyphBits, padArt, paletteOf, PALETTE, type PadControl, type PadFamily, type Tone } from '@/lib/pad-art'
import type { PadAxesEvent } from '@/lib/pad'
import { cn } from '@/lib/utils'

/**
 * A controller, drawn (src/lib/pad-art.ts), alive: a held key lights up and
 * sinks a pixel into the shell (its shadow gone), the sticks move with the
 * real ones, the triggers light as they are pulled, and `focus` rings the
 * control a binding is about. With `callouts`, each control's name sits
 * beside the pad with a line to it, so it is plain which button is which and
 * what it does. Each control can be pointed at and clicked.
 */

/** A held control's tones: a lit key. */
const PRESSED: Partial<Record<Tone, string>> = {
  outline: '#07080b',
  btn: '#e8ebf0',
  btnHi: '#ffffff',
  btnLo: '#b9bec8',
  glyph: '#14161a',
}

export type Callout = {
  control: PadControl
  label: string
  /** Doing another button's job: drawn in the accent colour. */
  changed?: boolean
}

/** Room beside the pad for callouts, in art pixels. */
const SIDE = 46
const GAP = 6.4
const D_PAD: PadControl[] = ['dpup', 'dpdown', 'dpleft', 'dpright']

export function PadArtView({
  family,
  pressed,
  axes,
  focus,
  onPick,
  onHover,
  names,
  callouts,
  className,
}: {
  family: PadFamily
  pressed?: ReadonlySet<PadControl>
  axes?: PadAxesEvent | null
  focus?: PadControl | null
  onPick?: (c: PadControl) => void
  onHover?: (c: PadControl | null) => void
  /** Tooltips: each control's name on this pad. */
  names?: (c: PadControl) => string
  callouts?: Callout[]
  className?: string
}) {
  const art = useMemo(() => padArt(family), [family])
  const colours = useMemo(() => paletteOf(family), [family])
  const fill = (tone: Tone) => `var(--pad-${tone}, ${colours[tone]})`
  const trigger = (c: PadControl) => (c === 'lefttrigger' ? axes?.lt ?? 0 : c === 'righttrigger' ? axes?.rt ?? 0 : 0)
  const shift = (c: PadControl): [number, number] => {
    if (!axes) return [0, 0]
    const [x, y] = c === 'leftstick' ? [axes.lx, axes.ly] : c === 'rightstick' ? [axes.rx, axes.ry] : [0, 0]
    // Two art pixels of travel, y up on the pad and down on screen.
    return [Math.round(x * 2), Math.round(-y * 2)]
  }

  const placed = useMemo(() => (callouts?.length ? place(callouts, art.layers, art.width, art.height) : []), [callouts, art])
  const side = placed.length ? SIDE : 2

  return (
    <svg
      viewBox={`${-side} -2 ${art.width + side * 2} ${art.height + 4}`}
      shapeRendering="crispEdges"
      role="img"
      aria-label={`${family} controller`}
      className={cn('block h-auto w-full select-none', className)}
      onMouseLeave={() => onHover?.(null)}
    >
      {art.layers.map((layer, i) => {
        const c = layer.control
        const held = !!c && (pressed?.has(c) || trigger(c) > 0.35)
        const [dx, dy] = c ? shift(c) : [0, 0]
        const ring = !!c && (focus === c || (focus === 'dpup' && D_PAD.includes(c)))
        // Pressed in: down a pixel, onto where its shadow was.
        const y = dy + (held ? 1 : 0)
        return (
          <g
            key={i}
            transform={dx || y ? `translate(${dx} ${y})` : undefined}
            onClick={c && onPick ? () => onPick(c) : undefined}
            onMouseEnter={c ? () => onHover?.(c) : undefined}
            className={cn(c && onPick && 'cursor-pointer')}
          >
            {c && names ? <title>{names(c)}</title> : null}
            {layer.paths.map((p) =>
              held && p.tone === 'shadow' ? null : (
                <path
                  key={p.tone}
                  d={p.d}
                  fill={
                    ring && p.tone === 'outline'
                      ? 'var(--pad-focus, var(--primary))'
                      : held && PRESSED[p.tone]
                        ? PRESSED[p.tone]
                        : fill(p.tone)
                  }
                />
              ),
            )}
          </g>
        )
      })}
      {placed.map((p) => {
        const on = focus === p.control || (D_PAD.includes(p.control) && focus !== null && focus !== undefined && D_PAD.includes(focus))
        const lit = on || (pressed ? (D_PAD.includes(p.control) ? D_PAD.some((d) => pressed.has(d)) : pressed.has(p.control)) : false)
        const colour = lit ? 'var(--primary)' : p.changed ? 'var(--warning)' : 'var(--muted-foreground)'
        return (
          <g key={p.control} onMouseEnter={() => onHover?.(p.control)} className="cursor-default">
            <polyline
              points={`${p.lx},${p.y} ${p.ex},${p.y} ${p.ax},${p.ay}`}
              fill="none"
              stroke={colour}
              strokeWidth={lit ? 0.5 : 0.3}
              // Faint until it is the one being looked at: the pad stays readable.
              strokeOpacity={lit ? 1 : 0.35}
              shapeRendering="geometricPrecision"
            />
            <rect x={p.ax - 0.75} y={p.ay - 0.75} width={1.5} height={1.5} fill={colour} fillOpacity={lit ? 1 : 0.6} />
            <text
              x={p.left ? p.lx - 1 : p.lx + 1}
              y={p.y}
              textAnchor={p.left ? 'end' : 'start'}
              dominantBaseline="central"
              fontSize={3.4}
              fontWeight={lit || p.changed ? 700 : 400}
              fill={lit ? 'var(--foreground)' : p.changed ? 'var(--warning)' : 'var(--muted-foreground)'}
              style={{ fontFamily: 'var(--font-mono, monospace)' }}
            >
              {p.label}
            </text>
          </g>
        )
      })}
    </svg>
  )
}

type Placed = Callout & { left: boolean; y: number; lx: number; ex: number; ax: number; ay: number }

/**
 * Callouts down each side of the pad, in the order their controls sit, at
 * least `GAP` apart, each with a line out to its control: across, then
 * straight in.
 */
function place(callouts: Callout[], layers: { control: PadControl | null; at: { x: number; y: number } }[], w: number, h: number): Placed[] {
  const anchor = (c: PadControl) => {
    // The d-pad is one callout, pointing at its middle.
    const group = D_PAD.includes(c) ? D_PAD : [c]
    const hits = layers.filter((l) => l.control && group.includes(l.control))
    if (!hits.length) return null
    return { x: hits.reduce((s, l) => s + l.at.x, 0) / hits.length, y: hits.reduce((s, l) => s + l.at.y, 0) / hits.length }
  }
  const all = callouts.flatMap((c) => {
    const a = anchor(c.control)
    return a ? [{ ...c, ax: a.x, ay: a.y, left: a.x < w / 2 - 0.6 }] : []
  })
  const out: Placed[] = []
  for (const left of [true, false]) {
    const list = all.filter((c) => c.left === left).sort((a, b) => a.ay - b.ay)
    const ys = list.map((c) => c.ay)
    // Push down to keep the gap, then pull back up if the last fell off.
    for (let i = 1; i < ys.length; i++) ys[i] = Math.max(ys[i]!, ys[i - 1]! + GAP)
    const over = (ys.at(-1) ?? 0) - (h - 1)
    if (over > 0) for (let i = ys.length - 1; i >= 0; i--) ys[i] = Math.min(ys[i]! - over, i < ys.length - 1 ? ys[i + 1]! - GAP : Infinity)
    list.forEach((c, i) => {
      const lx = left ? -2 : w + 2
      out.push({ ...c, y: ys[i]!, lx, ex: left ? 4 + (i % 2) * 2 : w - 4 - (i % 2) * 2 })
    })
  }
  return out
}

/**
 * A control's name as a key cap, the way the pad prints it: coloured face
 * letters on an Xbox pad, the four shapes on a PlayStation one.
 */
export function PadBadge({ family, control, label, className }: { family: PadFamily; control: PadControl; label: string; className?: string }) {
  const face = control === 'a' || control === 'b' || control === 'x' || control === 'y' ? FACE[family][control] : null
  const symbol = face && GLYPHS_5.has(face[0]) ? face[0] : null
  const color = face && face[1] !== 'glyph' ? PALETTE[face[1]] : undefined
  return (
    <span
      className={cn(
        'inline-flex h-6 min-w-6 shrink-0 items-center justify-center rounded-md border border-black/60 bg-[#2a2e36] px-1.5 font-mono text-[10px] font-bold text-[#c9cdd5] shadow-[inset_0_1px_0_#3c414b,inset_0_-1px_0_#1a1d22]',
        face && 'rounded-full px-0',
        className,
      )}
      style={color ? { color } : undefined}
      title={label}
    >
      {symbol ? (
        <svg viewBox="0 0 5 5" className="size-2.5" shapeRendering="crispEdges" aria-label={label}>
          {glyphBits(symbol).flatMap((row, y) =>
            [...row].map((ch, x) => (ch === '#' ? <rect key={`${x}-${y}`} x={x} y={y} width={1} height={1} fill="currentColor" /> : null)),
          )}
        </svg>
      ) : (
        <span aria-hidden>{face ? face[0] : label}</span>
      )}
    </span>
  )
}

const GLYPHS_5 = new Set(['cross', 'circle', 'square', 'triangle'])
