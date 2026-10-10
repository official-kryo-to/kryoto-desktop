import { call } from '@/lib/bridge'
import type { PadControl, PadFamily } from '@/lib/pad-art'

/**
 * Controllers, from the native side (src-tauri/src/pad.rs), behind kryo.to's
 * `controller` feature flag. Which pad it is, what its buttons are called and
 * how remaps work come from kryoto-padmap; the names below mirror its
 * `Layout` (layout.rs) so the browser preview can draw them too.
 */

export type PadModel = {
  family: PadFamily
  /** "DualSense", "Xbox Series X|S Controller", or what the pad calls itself. */
  name: string
  features: { touchpad: boolean; gyro: boolean; paddles: number; misc: boolean; analogTriggers: boolean }
  /** How it was told: by its USB ids, by its name, by its maker, or not at all. */
  matched: 'ids' | 'name' | 'maker' | 'fallback'
}

export type PadInfo = {
  /** SDL's instance id: this connection's. */
  id: number
  /** SDL's GUID: one per model; settings are kept under it. */
  guid: string
  /** The name to show (the maker's when the system gave a placeholder). */
  name: string
  /** What the system calls it ("HID-compliant game controller"). */
  systemName: string
  vendor: number | null
  product: number | null
  model: PadModel
  /** Its buttons are known (SDL's database, or a setup saved here). */
  mapped: boolean
  /** Its buttons come from a setup saved here. */
  custom: boolean
  /** An Xbox pad: Windows games read it as it is. */
  xinput: boolean
  rumble: boolean
  battery: number | null
  charging: boolean
  /** What the setup can listen to. */
  buttons: number
  axes: number
  hats: number
}

/** physical control -> the control it acts as; missing ones act as themselves. */
export type PadRemap = Partial<Record<PadControl, PadControl>>

export type PadPrefs = {
  name: string
  /** Drawn and named as this family instead of the detected one. */
  family: PadFamily | null
  remap: PadRemap
  /** The setup saved here (an SDL mapping), when there is one. */
  mapping: string | null
  /** SDL's own mapping for it, as last seen. */
  base: string | null
  /** The SDL mapping games get, written by the native side. */
  sdl: string | null
}

export type PadConfig = {
  enabled: boolean
  navigate: boolean
  notify: boolean
  games: boolean
  /** Feel the selection move (light rumble). */
  haptics: boolean
  /** Windows: play as a virtual Xbox controller while a game runs. */
  virtualPad: boolean
  pads: Record<string, PadPrefs>
}

/** tick: the selection moved; select: pressed, or the page changed; edge: nothing further that way. */
export type PadHaptic = 'tick' | 'select' | 'edge'

export type PadButtonEvent = { id: number; control: PadControl; pressed: boolean }
/** A raw input during the setup: a button (1 down, 0 up), an axis (-1..1) or a hat (direction bits). */
export type PadRawEvent = { id: number; kind: 'button' | 'axis' | 'hat'; index: number; value: number }
/** Where every raw axis and hat rests when the setup starts listening. */
export type PadRawState = { id: number; axes: number[]; hats: number[] }

/** The free driver Windows needs for the virtual Xbox controller. */
export const VIRTUAL_DRIVER_URL = 'https://github.com/nefarius/ViGEmBus/releases/latest'

export type PadAxesEvent = { id: number; lx: number; ly: number; rx: number; ry: number; lt: number; rt: number }

export const padApi = {
  start: () => call<PadInfo[]>('pad_start'),
  stop: () => call<void>('pad_stop'),
  list: () => call<PadInfo[]>('pad_list'),
  config: () => call<PadConfig>('pad_config_get'),
  save: (config: PadConfig) => call<PadConfig>('pad_config_set', { config }),
  rumble: (id: number) => call<void>('pad_rumble', { id }),
  haptic: (id: number, kind: PadHaptic) => call<void>('pad_haptic', { id, kind }),
  /** Hand a press or the sticks to the Store's page (kryo.to moves around by itself). */
  forward: (detail: Record<string, unknown>) => call<void>('pad_forward', { detail }),
  /** Listen to one pad's raw inputs (`pad-raw`), for the setup; `null` stops. */
  capture: (id: number | null) => call<void>('pad_capture', { id }),
  /** Save a setup (SDL field -> source), or forget it with `null`. */
  saveSetup: (guid: string, name: string, bindings: Record<string, string> | null) =>
    call<PadConfig>('pad_mapping_set', { guid, name, bindings }),
  /** Windows: is the virtual-controller driver installed? */
  virtualDriver: () => call<boolean>('pad_virtual_driver'),
}

/* ── Names ─────────────────────────────────────────────── */

export type PadLabel = { short: string; long: string }

const SHARED: Partial<Record<PadControl, PadLabel>> = {
  leftshoulder: { short: 'LB', long: 'Left bumper' },
  rightshoulder: { short: 'RB', long: 'Right bumper' },
  lefttrigger: { short: 'LT', long: 'Left trigger' },
  righttrigger: { short: 'RT', long: 'Right trigger' },
  leftstick: { short: 'LS', long: 'Left stick press' },
  rightstick: { short: 'RS', long: 'Right stick press' },
  dpup: { short: 'Up', long: 'D-pad up' },
  dpdown: { short: 'Down', long: 'D-pad down' },
  dpleft: { short: 'Left', long: 'D-pad left' },
  dpright: { short: 'Right', long: 'D-pad right' },
  touchpad: { short: 'Touchpad', long: 'Touchpad click' },
  paddle1: { short: 'P1', long: 'Upper right paddle' },
  paddle2: { short: 'P2', long: 'Upper left paddle' },
  paddle3: { short: 'P3', long: 'Lower right paddle' },
  paddle4: { short: 'P4', long: 'Lower left paddle' },
}

const OWN: Record<PadFamily, Partial<Record<PadControl, PadLabel>>> = {
  xbox: {
    a: { short: 'A', long: 'A button' },
    b: { short: 'B', long: 'B button' },
    x: { short: 'X', long: 'X button' },
    y: { short: 'Y', long: 'Y button' },
    back: { short: 'View', long: 'View button' },
    start: { short: 'Menu', long: 'Menu button' },
    guide: { short: 'Home', long: 'Home button' },
    misc1: { short: 'Share', long: 'Share button' },
  },
  playstation: {
    a: { short: 'Cross', long: 'Cross button' },
    b: { short: 'Circle', long: 'Circle button' },
    x: { short: 'Square', long: 'Square button' },
    y: { short: 'Triangle', long: 'Triangle button' },
    leftshoulder: { short: 'L1', long: 'L1 button' },
    rightshoulder: { short: 'R1', long: 'R1 button' },
    lefttrigger: { short: 'L2', long: 'L2 trigger' },
    righttrigger: { short: 'R2', long: 'R2 trigger' },
    leftstick: { short: 'L3', long: 'Left stick press' },
    rightstick: { short: 'R3', long: 'Right stick press' },
    back: { short: 'Create', long: 'Create / Share button' },
    start: { short: 'Options', long: 'Options button' },
    guide: { short: 'PS', long: 'PS button' },
    misc1: { short: 'Mic', long: 'Mute button' },
  },
  nintendo: {
    a: { short: 'B', long: 'B button' },
    b: { short: 'A', long: 'A button' },
    x: { short: 'Y', long: 'Y button' },
    y: { short: 'X', long: 'X button' },
    leftshoulder: { short: 'L', long: 'L button' },
    rightshoulder: { short: 'R', long: 'R button' },
    lefttrigger: { short: 'ZL', long: 'ZL button' },
    righttrigger: { short: 'ZR', long: 'ZR button' },
    back: { short: '-', long: 'Minus button' },
    start: { short: '+', long: 'Plus button' },
    guide: { short: 'Home', long: 'Home button' },
    misc1: { short: 'Capture', long: 'Capture button' },
  },
  generic: {
    a: { short: 'S', long: 'Bottom face button' },
    b: { short: 'E', long: 'Right face button' },
    x: { short: 'W', long: 'Left face button' },
    y: { short: 'N', long: 'Top face button' },
    leftshoulder: { short: 'L1', long: 'Left bumper' },
    rightshoulder: { short: 'R1', long: 'Right bumper' },
    lefttrigger: { short: 'L2', long: 'Left trigger' },
    righttrigger: { short: 'R2', long: 'Right trigger' },
    back: { short: 'Select', long: 'Select button' },
    start: { short: 'Start', long: 'Start button' },
    guide: { short: 'Home', long: 'Home button' },
    misc1: { short: 'Misc', long: 'Extra button' },
  },
}

export function padLabel(family: PadFamily, control: PadControl): PadLabel {
  return OWN[family][control] ?? SHARED[control] ?? { short: control, long: control }
}

export const FAMILY_NAMES: Record<PadFamily, string> = {
  xbox: 'Xbox',
  playstation: 'PlayStation',
  nintendo: 'Nintendo',
  generic: 'Universal',
}

/** Confirm and back in menus: Nintendo pads say yes with A, on the right. */
export const confirmOf = (f: PadFamily): PadControl => (f === 'nintendo' ? 'b' : 'a')
export const backOf = (f: PadFamily): PadControl => (f === 'nintendo' ? 'a' : 'b')

/* ── Remapping (padmap's Remap: always a swap) ─────────── */

/**
 * The controls a remap can move, in the order Settings lists them. The
 * d-pad and Home stay put (Home belongs to the system; the d-pad is a hat on
 * most pads, which a mapping cannot split), and paddles, touchpad and the
 * extra middle button are not read on every system.
 */
export const REMAPPABLE: PadControl[] = [
  'a', 'b', 'x', 'y',
  'leftshoulder', 'rightshoulder', 'lefttrigger', 'righttrigger',
  'leftstick', 'rightstick', 'back', 'start',
]

export const actsAs = (r: PadRemap, physical: PadControl): PadControl => r[physical] ?? physical
export const doneBy = (r: PadRemap, job: PadControl): PadControl =>
  REMAPPABLE.find((p) => actsAs(r, p) === job) ?? job

/** `physical` now does `job`; whatever did `job` takes over its old one. */
export function assign(r: PadRemap, physical: PadControl, job: PadControl): PadRemap {
  const old = actsAs(r, physical)
  if (old === job) return r
  const other = doneBy(r, job)
  const next = { ...r, [physical]: job, [other]: old }
  for (const k of Object.keys(next) as PadControl[]) if (next[k] === k) delete next[k]
  return next
}

export const isIdentity = (r: PadRemap) => Object.keys(r).length === 0

/** The family a pad is drawn as: the override, else what it was told to be. */
export const familyOf = (pad: { model: PadModel } | null, prefs?: PadPrefs | null): PadFamily =>
  prefs?.family ?? pad?.model.family ?? 'generic'
