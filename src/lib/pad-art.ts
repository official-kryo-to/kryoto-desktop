/**
 * The controllers, drawn as pixel art.
 *
 * Each pad is a handful of shapes (rounded rectangles, circles, capsules) on
 * a 112 x 64 grid, rasterized here into pixels and shaded the pixel-art way:
 * a one-pixel outline, light from the top left, dithered shade along the
 * bottom and right rims, and a one-pixel drop shadow under every key (gone
 * when it is pressed in). Each family has its own colours: a carbon Xbox pad, a
 * white and black DualSense with its light bars, a black Switch Pro, a grey
 * universal pad. Every control is its own layer with its own id, so the
 * screen can light up what is pressed, move a stick with the real stick, and
 * point at the control a binding is about. The home button wears the K// mark.
 *
 * Pure (no React, no DOM, no path aliases), so `scripts/pad-art.mjs` can also
 * write the drawings out as plain .svg files for kryoto-padmap's README.
 */

export type PadFamily = 'xbox' | 'playstation' | 'nintendo' | 'generic'
export const PAD_FAMILIES: PadFamily[] = ['xbox', 'playstation', 'nintendo', 'generic']

/** SDL's names for the controls (kryoto-padmap's `Control`). */
export type PadControl =
  | 'a' | 'b' | 'x' | 'y'
  | 'leftshoulder' | 'rightshoulder' | 'lefttrigger' | 'righttrigger'
  | 'back' | 'start' | 'guide' | 'leftstick' | 'rightstick'
  | 'dpup' | 'dpdown' | 'dpleft' | 'dpright'
  | 'misc1' | 'touchpad' | 'paddle1' | 'paddle2' | 'paddle3' | 'paddle4'

/** What a pixel is: the colour roles a palette fills in. */
export type Tone =
  | 'outline' | 'body' | 'bodyHi' | 'bodyLo' | 'well'
  | 'plate' | 'plateHi' | 'plateLo'
  | 'btn' | 'btnHi' | 'btnLo' | 'glyph' | 'shadow' | 'light'
  | 'green' | 'red' | 'blue' | 'yellow' | 'pink' | 'teal'

export type ArtLayer = {
  /** The control this layer is, or null for the body and decoration. */
  control: PadControl | null
  /** `M x y h w v 1 h -w z` runs, one path per tone. */
  paths: { tone: Tone; d: string }[]
  /** Where the control sits (pixel centre), for pointing at it. */
  at: { x: number; y: number }
  /** Its pixels' bounds, inclusive: where an arrow can point at it from. */
  box: { x0: number; y0: number; x1: number; y1: number }
}

export type PadArt = { family: PadFamily; width: number; height: number; layers: ArtLayer[] }

export const ART_W = 112
export const ART_H = 64

// ---- shapes ---------------------------------------------------------------

type Shape = (x: number, y: number) => boolean
// A little under r squared: a pixel circle without a one-pixel nub at each
// of its four extremes.
const circle = (cx: number, cy: number, r: number): Shape => (x, y) => (x - cx) ** 2 + (y - cy) ** 2 <= r * r - r * 0.3
const rrect = (x0: number, y0: number, x1: number, y1: number, r: number): Shape => (x, y) => {
  if (x < x0 || x > x1 || y < y0 || y > y1) return false
  const dx = Math.max(x0 + r - x, 0, x - (x1 - r))
  const dy = Math.max(y0 + r - y, 0, y - (y1 - r))
  return dx * dx + dy * dy <= r * r
}
const capsule = (ax: number, ay: number, bx: number, by: number, r: number): Shape => (x, y) => {
  const vx = bx - ax, vy = by - ay
  const t = Math.max(0, Math.min(1, ((x - ax) * vx + (y - ay) * vy) / (vx * vx + vy * vy)))
  return (x - ax - t * vx) ** 2 + (y - ay - t * vy) ** 2 <= r * r
}
const union = (...s: Shape[]): Shape => (x, y) => s.some((f) => f(x, y))
const minus = (a: Shape, b: Shape): Shape => (x, y) => a(x, y) && !b(x, y)
const both = (a: Shape, b: Shape): Shape => (x, y) => a(x, y) && b(x, y)

// ---- glyphs (3x5 letters, 5x5 symbols) ------------------------------------

const GLYPHS: Record<string, string[]> = {
  A: ['.#.', '#.#', '###', '#.#', '#.#'],
  B: ['##.', '#.#', '##.', '#.#', '##.'],
  X: ['#.#', '#.#', '.#.', '#.#', '#.#'],
  Y: ['#.#', '#.#', '.#.', '.#.', '.#.'],
  S: ['.##', '#..', '.#.', '..#', '##.'],
  E: ['###', '#..', '##.', '#..', '###'],
  W: ['#...#', '#...#', '#.#.#', '#.#.#', '.#.#.'],
  N: ['#...#', '##..#', '#.#.#', '#..##', '#...#'],
  K: ['#.#', '#.#', '##.', '#.#', '#.#'],
  '/': ['..#', '..#', '.#.', '#..', '#..'],
  L: ['#..', '#..', '#..', '#..', '###'],
  R: ['##.', '#.#', '##.', '#.#', '#.#'],
  T: ['###', '.#.', '.#.', '.#.', '.#.'],
  Z: ['###', '..#', '.#.', '#..', '###'],
  '1': ['.#.', '##.', '.#.', '.#.', '###'],
  '2': ['.#.', '#.#', '..#', '.#.', '###'],
  '+': ['...', '.#.', '###', '.#.', '...'],
  '-': ['...', '...', '###', '...', '...'],
  cross: ['#...#', '.#.#.', '..#..', '.#.#.', '#...#'],
  circle: ['.###.', '#...#', '#...#', '#...#', '.###.'],
  square: ['#####', '#...#', '#...#', '#...#', '#####'],
  triangle: ['..#..', '.#.#.', '.#.#.', '#...#', '#####'],
  // A stick cap's dish.
  dish: ['.###.', '#...#', '#...#', '#...#', '.###.'],
}

export function glyphBits(text: string): string[] {
  const whole = GLYPHS[text]
  if (whole) return whole
  const rows = ['', '', '', '', '']
  ;[...text].forEach((ch, i) => {
    const g = GLYPHS[ch] ?? ['...', '...', '###', '...', '...']
    for (let r = 0; r < 5; r++) rows[r] += (i ? '.' : '') + (g[r] ?? '...')
  })
  return rows
}

// ---- parts ----------------------------------------------------------------

type Part = {
  control?: PadControl
  shape: Shape
  /**
   * body: the shell; plate: a second shell colour on top of it (the
   * DualSense's black middle); well: a stick's hollow; btn: anything you
   * press; light: a light bar.
   */
  kind: 'body' | 'plate' | 'well' | 'btn' | 'light'
  /** Base, lit and shaded tones, instead of the kind's. */
  tones?: [Tone, Tone, Tone]
  glyph?: string
  glyphTone?: Tone
  /** No outline (a stick's hollow is already dark). */
  bare?: boolean
}

/** `decor`: drawn over the body, under the controls. */
type Spec = { body: Shape; decor?: (body: Shape) => Part[]; parts: Part[] }

/** The K// home button, the same on every pad. */
const home = (cx: number, cy: number): Part => ({
  control: 'guide', kind: 'btn', shape: rrect(cx - 7.5, cy - 4.5, cx + 7.5, cy + 4.5, 4.5), glyph: 'K//',
})

/** A small key (View, Menu, Create, Share): big enough to show its face. */
const small = (control: PadControl, cx: number, cy: number, w = 5, h = 4, glyph?: string): Part => ({
  control, kind: 'btn', shape: rrect(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2, 1), glyph,
})

/** A stick: its hollow, then the cap (the control). */
const stick = (control: PadControl, cx: number, cy: number, well: number, cap: number): Part[] => [
  { kind: 'well', shape: circle(cx, cy, well), bare: true },
  { control, kind: 'btn', shape: circle(cx, cy, cap), glyph: 'dish', glyphTone: 'btnLo' },
]

/** A joined d-pad's arms are flat: the cross around them carries the shading. */
const FLAT: [Tone, Tone, Tone] = ['btn', 'btn', 'btn']

/**
 * A d-pad. Joined (Xbox, Nintendo, universal): one cross with an outline,
 * each arm's inside its own layer. Split (`gap`, PlayStation): four arrow
 * keys, each pointed at the middle.
 */
const dpad = (cx: number, cy: number, arm: number, w: number, gap: number): Part[] => {
  const h = w / 2
  if (gap) {
    // One arrow pointing up at the middle, turned for the other three.
    const inner = gap + 0.5
    const arrow = (turn: (x: number, y: number) => [number, number]): Shape => (x, y) => {
      const [u, v] = turn(x - cx, y - cy) // u across, -v out from the middle
      const d = -v
      if (d < inner || d > arm || Math.abs(u) > h) return false
      // Square at the outer end, narrowing to a point toward the middle.
      return Math.abs(u) <= (d - inner) * 1.25 + 0.6
    }
    return [
      { control: 'dpup', kind: 'btn', shape: arrow((x, y) => [x, y]) },
      { control: 'dpdown', kind: 'btn', shape: arrow((x, y) => [x, -y]) },
      { control: 'dpleft', kind: 'btn', shape: arrow((x, y) => [y, x]) },
      { control: 'dpright', kind: 'btn', shape: arrow((x, y) => [y, -x]) },
    ]
  }
  const i = h - 1 // an arm's inside, one pixel in from the outline
  return [
    { kind: 'btn', shape: union(rrect(cx - h, cy - arm, cx + h, cy + arm, 1), rrect(cx - arm, cy - h, cx + arm, cy + h, 1)) },
    { control: 'dpup', kind: 'btn', bare: true, tones: FLAT, shape: rrect(cx - i, cy - arm + 1, cx + i, cy - 1, 0) },
    { control: 'dpdown', kind: 'btn', bare: true, tones: FLAT, shape: rrect(cx - i, cy + 1, cx + i, cy + arm - 1, 0) },
    { control: 'dpleft', kind: 'btn', bare: true, tones: FLAT, shape: rrect(cx - arm + 1, cy - i, cx - 1, cy + i, 0) },
    { control: 'dpright', kind: 'btn', bare: true, tones: FLAT, shape: rrect(cx + 1, cy - i, cx + arm - 1, cy + i, 0) },
  ]
}

type Face = 'a' | 'b' | 'x' | 'y'

/** What each family prints on its face buttons, and in which colour. */
export const FACE: Record<PadFamily, Record<Face, [glyph: string, tone: Tone]>> = {
  xbox: { a: ['A', 'green'], b: ['B', 'red'], x: ['X', 'blue'], y: ['Y', 'yellow'] },
  playstation: { a: ['cross', 'blue'], b: ['circle', 'red'], x: ['square', 'pink'], y: ['triangle', 'teal'] },
  nintendo: { a: ['B', 'glyph'], b: ['A', 'glyph'], x: ['Y', 'glyph'], y: ['X', 'glyph'] },
  generic: { a: ['S', 'glyph'], b: ['E', 'glyph'], x: ['W', 'glyph'], y: ['N', 'glyph'] },
}

/** Four face buttons around (cx, cy). */
const face = (
  cx: number, cy: number, off: number, r: number,
  glyphs: Record<Face, [string, Tone]>,
): Part[] =>
  ([
    ['y', cx, cy - off],
    ['a', cx, cy + off],
    ['x', cx - off, cy],
    ['b', cx + off, cy],
  ] as const).map(([c, x, y]) => ({ control: c, kind: 'btn', shape: circle(x, y, r), glyph: glyphs[c][0], glyphTone: glyphs[c][1] }))

const W = ART_W - 1 // mirror axis: x -> W - x
const mirror = (x: number) => W - x

/**
 * Shoulders, drawn before the body so it covers their lower half: triggers
 * as tabs peeking out behind, bumpers as bars along the top edge. `lb` and
 * `lt` are the left one's x span; the right one mirrors it.
 */
const shoulders = (lb: [number, number], lt: [number, number], tones?: [Tone, Tone, Tone]): Part[] => [
  { control: 'lefttrigger', kind: 'btn', tones, shape: rrect(lt[0] + 1, 5.5, lt[1], 18, 4.5) },
  { control: 'righttrigger', kind: 'btn', tones, shape: rrect(mirror(lt[1]), 5.5, mirror(lt[0] + 1), 18, 4.5) },
  { control: 'leftshoulder', kind: 'btn', tones, shape: rrect(lb[0], 10, lb[1], 30, 10) },
  { control: 'rightshoulder', kind: 'btn', tones, shape: rrect(mirror(lb[1]), 10, mirror(lb[0]), 30, 10) },
]

/** The shell: a wide top, two grips going down and out, a notch between. */
const shell = (top: [number, number, number, number, number], grip: [number, number, number, number, number], notch: [number, number, number]): Shape =>
  minus(
    union(
      rrect(top[0], top[1], mirror(top[0]), top[3], top[4]),
      capsule(grip[0], grip[1], grip[2], grip[3], grip[4]),
      capsule(mirror(grip[0]), grip[1], mirror(grip[2]), grip[3], grip[4]),
    ),
    circle(55.5, notch[1], notch[2]),
  )

const SPECS: Record<PadFamily, () => Spec> = {
  xbox: () => ({
    body: shell([18, 14, 0, 41, 12], [29, 30, 21, 48, 12.5], [0, 59, 13.5]),
    parts: [
      ...stick('leftstick', 31, 26, 8, 5.5),
      ...dpad(43, 37.5, 6.5, 5, 0),
      ...stick('rightstick', 67.5, 37.5, 7.5, 5.5),
      ...face(80, 27, 6.8, 4, FACE.xbox),
      home(55.5, 18.5),
      small('back', 47.5, 26.5),
      small('start', 63.5, 26.5),
      small('misc1', 55.5, 31.5, 4, 3),
    ],
  }),
  playstation: () => ({
    body: shell([16, 15, 0, 41, 11], [26, 30, 19, 48, 12.5], [0, 64, 12]),
    // The DualSense: white wings, a black middle, light bars beside the pad.
    decor: (body) => [
      { kind: 'plate', shape: both(body, rrect(37, 25, mirror(37), 56, 8)) },
      { kind: 'light', bare: true, shape: rrect(37, 15, 37.9, 26, 0.4) },
      { kind: 'light', bare: true, shape: rrect(mirror(37.9), 15, mirror(37), 26, 0.4) },
    ],
    parts: [
      { control: 'touchpad', kind: 'btn', tones: ['body', 'bodyHi', 'bodyLo'], shape: rrect(39.5, 13, mirror(39.5), 28, 3) },
      ...dpad(26.5, 29, 8, 5, 1),
      ...face(85, 29, 6.8, 4.1, FACE.playstation),
      ...stick('leftstick', 41, 34.5, 7, 5.2),
      ...stick('rightstick', 70, 34.5, 7, 5.2),
      small('back', 33.5, 20, 3, 5),
      small('start', mirror(33.5), 20, 3, 5),
      home(55.5, 40.5),
    ],
  }),
  nintendo: () => ({
    body: shell([18, 14, 0, 41, 13], [29, 30, 22, 47, 12.5], [0, 58, 13]),
    parts: [
      ...stick('leftstick', 31, 26, 8, 5.5),
      ...dpad(43, 37.5, 6.5, 5, 0),
      ...stick('rightstick', 67.5, 37.5, 7.5, 5.5),
      ...face(80, 27, 6.8, 4, FACE.nintendo),
      home(55.5, 18.5),
      small('back', 46, 26, 5, 5, '-'),
      small('start', 65, 26, 5, 5, '+'),
      small('misc1', 55.5, 28, 4, 4),
    ],
  }),
  generic: () => ({
    body: shell([16, 15, 0, 41, 13], [27, 30, 23, 47, 12], [0, 58, 12.5]),
    parts: [
      ...dpad(31, 28.5, 7, 5, 0),
      ...face(80, 28.5, 6.8, 4, FACE.generic),
      ...stick('leftstick', 44, 37.5, 7, 5),
      ...stick('rightstick', 67, 37.5, 7, 5),
      small('back', 49, 28),
      small('start', 62, 28),
      home(55.5, 19.5),
    ],
  }),
}

const SHOULDERS: Record<PadFamily, [[number, number], [number, number]]> = {
  xbox: [[18, 44], [23, 38]],
  playstation: [[16, 41], [21, 36]],
  nintendo: [[18, 44], [24, 38]],
  generic: [[17, 43], [22, 37]],
}

// ---- rasterizing ----------------------------------------------------------

type Grid = (Tone | null)[]

function rasterize(part: Part): { grid: Grid; at: { x: number; y: number }; box: ArtLayer['box'] } {
  const inside = new Uint8Array(ART_W * ART_H)
  let sx = 0, sy = 0, n = 0, minX = ART_W, maxX = 0, minY = ART_H, maxY = 0
  for (let y = 0; y < ART_H; y++)
    for (let x = 0; x < ART_W; x++)
      if (part.shape(x + 0.5, y + 0.5)) {
        inside[y * ART_W + x] = 1
        sx += x; sy += y; n++
        minX = Math.min(minX, x); maxX = Math.max(maxX, x); minY = Math.min(minY, y); maxY = Math.max(maxY, y)
      }
  const at = (x: number, y: number) => x >= 0 && y >= 0 && x < ART_W && y < ART_H && inside[y * ART_W + x] === 1
  const shell = part.kind === 'body' || part.kind === 'plate'
  const [base, hi, lo]: [Tone, Tone, Tone] =
    part.tones ??
    (part.kind === 'body'
      ? ['body', 'bodyHi', 'bodyLo']
      : part.kind === 'plate'
        ? ['plate', 'plateHi', 'plateLo']
        : part.kind === 'well'
          ? ['well', 'well', 'well']
          : part.kind === 'light'
            ? ['light', 'light', 'light']
            : ['btn', 'btnHi', 'btnLo'])
  const grid: Grid = new Array(ART_W * ART_H).fill(null)
  const checker = (x: number, y: number) => (x + y) % 2 === 0
  for (let y = 0; y < ART_H; y++)
    for (let x = 0; x < ART_W; x++) {
      if (!at(x, y)) continue
      const edge = !at(x - 1, y) || !at(x + 1, y) || !at(x, y - 1) || !at(x, y + 1)
      let t: Tone = base
      if (edge && !part.bare) t = 'outline'
      else if (shell) {
        // Light from the top left: a lit rim, then a dithered fade; the
        // bottom and right rims in shade, dithered too: it follows the shape.
        if (!at(x, y - 2) || !at(x - 2, y)) t = hi
        else if ((!at(x, y - 3) || !at(x - 3, y)) && checker(x, y)) t = hi
        else if (!at(x, y + 3) || !at(x + 2, y)) t = lo
        else if ((!at(x, y + 5) || !at(x + 3, y)) && checker(x, y)) t = lo
      } else if (!at(x, y - 2) || !at(x - 2, y - 1)) t = hi
      else if (!at(x, y + 2)) t = lo
      grid[y * ART_W + x] = t
    }
  // A key casts a one-pixel shadow below it (its layer moves with it, and the
  // shadow goes when it is pressed in).
  if (part.kind === 'btn' && !part.bare)
    for (let y = 1; y < ART_H; y++)
      for (let x = 0; x < ART_W; x++) if (at(x, y - 1) && !at(x, y)) grid[y * ART_W + x] = 'shadow'
  if (part.glyph) {
    const bits = glyphBits(part.glyph)
    const gw = bits[0]?.length ?? 0, gh = bits.length
    const ox = Math.round((minX + maxX + 1) / 2 - gw / 2)
    const oy = Math.round((minY + maxY + 1) / 2 - gh / 2)
    bits.forEach((row, r) =>
      [...row].forEach((c, col) => {
        if (c === '#' && at(ox + col, oy + r)) grid[(oy + r) * ART_W + ox + col] = part.glyphTone ?? 'glyph'
      }),
    )
  }
  return {
    grid,
    at: n ? { x: sx / n + 0.5, y: sy / n + 0.5 } : { x: 0, y: 0 },
    box: n ? { x0: minX, y0: minY, x1: maxX, y1: maxY } : { x0: 0, y0: 0, x1: 0, y1: 0 },
  }
}

/** Horizontal runs of each tone, as one path per tone. */
function toPaths(grid: Grid): { tone: Tone; d: string }[] {
  const byTone = new Map<Tone, string[]>()
  for (let y = 0; y < ART_H; y++) {
    let x = 0
    while (x < ART_W) {
      const t = grid[y * ART_W + x]
      if (!t) { x++; continue }
      let e = x + 1
      while (e < ART_W && grid[y * ART_W + e] === t) e++
      const list = byTone.get(t) ?? []
      list.push(`M${x} ${y}h${e - x}v1h-${e - x}z`)
      byTone.set(t, list)
      x = e
    }
  }
  return [...byTone].map(([tone, d]) => ({ tone, d: d.join('') }))
}

const cache = new Map<PadFamily, PadArt>()

export function padArt(family: PadFamily): PadArt {
  const hit = cache.get(family)
  if (hit) return hit
  const spec = SPECS[family]()
  const parts: Part[] = [
    ...shoulders(...SHOULDERS[family], family === 'playstation' ? ['body', 'bodyHi', 'bodyLo'] : undefined),
    { kind: 'body', shape: spec.body },
    ...(spec.decor?.(spec.body) ?? []),
    ...spec.parts,
  ]
  const layers = parts.map((p) => {
    const { grid, at, box } = rasterize(p)
    return { control: p.control ?? null, paths: toPaths(grid), at, box }
  })
  const art = { family, width: ART_W, height: ART_H, layers }
  cache.set(family, art)
  return art
}

// ---- palette and standalone SVG -------------------------------------------

/** The shared colours: a dark pad, as in the product photos. */
export const PALETTE: Record<Tone, string> = {
  outline: '#07080b',
  body: '#1d2027',
  bodyHi: '#2e333c',
  bodyLo: '#121418',
  well: '#0b0c0f',
  plate: '#16181d',
  plateHi: '#23262d',
  plateLo: '#0d0e11',
  btn: '#2a2e36',
  btnHi: '#3e434d',
  btnLo: '#1a1d22',
  glyph: '#c9cdd5',
  // Translucent, so it reads on a white shell and a black one alike.
  shadow: 'rgba(0, 0, 0, 0.42)',
  light: '#4ba4ff',
  green: '#5fd16e',
  red: '#f0545e',
  blue: '#4ba4ff',
  yellow: '#f4d03f',
  pink: '#ee8ccf',
  teal: '#3ccfbe',
}

/** Each family's own colours, over the shared ones. */
const FAMILY_PALETTE: Record<PadFamily, Partial<Record<Tone, string>>> = {
  // Carbon black, with a little blue in it.
  xbox: { body: '#1c1f26', bodyHi: '#2d323c', bodyLo: '#111318' },
  // The DualSense: white wings, black middle, blue light bars.
  playstation: {
    body: '#e4e6eb',
    bodyHi: '#fbfcfd',
    bodyLo: '#b8bcc6',
    outline: '#1a1c21',
    btn: '#2a2d35',
    btnHi: '#40444e',
    btnLo: '#1a1c21',
    plate: '#101114',
    plateHi: '#1c1e23',
    plateLo: '#0a0b0d',
    well: '#050607',
    light: '#5fb4ff',
  },
  // A Switch Pro Controller: black, a touch warmer.
  nintendo: { body: '#1b1b1f', bodyHi: '#2c2c32', bodyLo: '#111114', btn: '#2b2b31', btnHi: '#3f3f47', btnLo: '#1a1a1e' },
  // A plain grey pad.
  generic: { body: '#4a4f59', bodyHi: '#646a76', bodyLo: '#343841', btn: '#2c3038', btnHi: '#424752', btnLo: '#1d2025' },
}

export function paletteOf(family: PadFamily): Record<Tone, string> {
  return { ...PALETTE, ...FAMILY_PALETTE[family] }
}

/** A drawing as a standalone .svg file, `scale` screen pixels per art pixel. */
export function padSvg(family: PadFamily, scale = 8): string {
  const art = padArt(family)
  const colours = paletteOf(family)
  const body = art.layers
    .map((l) => {
      const id = l.control ? ` id="${l.control}"` : ''
      return `<g${id}>${l.paths.map((p) => `<path fill="${colours[p.tone]}" d="${p.d}"/>`).join('')}</g>`
    })
    .join('')
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${art.width} ${art.height}" width="${art.width * scale}" height="${art.height * scale}" shape-rendering="crispEdges">${body}</svg>\n`
}
