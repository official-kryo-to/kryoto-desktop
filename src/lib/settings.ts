import { useEffect, useState } from 'react'
import { call } from '@/lib/bridge'
import { clearCatalogEndpointCache } from '@/lib/endpoint'

export type Palette = 'monochrome' | 'oled' | 'amber' | 'emerald' | 'nord' | 'sepia' | 'blossom'
export type Radius = 'sharp' | 'soft' | 'rounded' | 'round' | 'pill'

/** Mirrors `src-tauri/src/settings.rs`. */
export type Settings = {
  recoveryError?: string | null
  libraryDir: string
  deleteArchives: boolean
  startPage: 'store' | 'library'
  defaultCompatTool: string | null
  minimizeOnPlay: boolean
  notifyDownloads: boolean
  palette: Palette
  radius: Radius
  font: 'teletext' | 'mono'
  showAdult: boolean
  /** Take palette, corners, typeface and the adult blur from the kryo.to account. */
  followAccount: boolean
  /** Library folders besides `libraryDir` (where new games go). */
  libraryFolders: string[]
  sendReports: boolean
  closeToTray: boolean
  startWithSystem: boolean
  /** Buttons press in under the pointer. */
  pressEffect: boolean
  /** Add play time to the kryo.to account. */
  sharePlaytime: boolean
  /** Parallel connections per download. */
  connections: number
  /** MB/s cap; 0 is none. */
  speedLimitMb: number
  /** Blank uses production; set a local site origin while developing. */
  catalogEndpoint: string
  /** Linux: MangoHud's overlay on Windows games. */
  linuxMangohud: boolean
  /** Linux: run Windows games under GameMode. */
  linuxGamemode: boolean
  /** Linux: Proton-GE's FSR upscaling at lower fullscreen resolutions. */
  linuxFsr: boolean
  /** The name in games: your K// username, `playerName`, or each build's own. */
  playerNameMode: PlayerNameMode
  playerName: string
}

/** How a game's in-game name is picked (src-tauri player_name.rs). */
export type PlayerNameMode = 'account' | 'custom' | 'build'

/** Steam's limit for a name, which the emulators share. */
export const PLAYER_NAME_MAX = 32

/** kryo.to's palettes, named as the site names them. */
export const PALETTES: { id: Palette; label: string }[] = [
  { id: 'monochrome', label: 'Monochrome' },
  { id: 'oled', label: 'Black' },
  { id: 'amber', label: 'Amber' },
  { id: 'emerald', label: 'Green' },
  { id: 'nord', label: 'Slate' },
  { id: 'sepia', label: 'Sepia' },
  { id: 'blossom', label: 'Pink' },
]

export const RADII: { value: Radius; label: string }[] = [
  { value: 'sharp', label: 'Sharp' },
  { value: 'soft', label: 'Soft' },
  { value: 'rounded', label: 'Rounded' },
  { value: 'round', label: 'Round' },
  { value: 'pill', label: 'Pill' },
]

export type Look = Pick<Settings, 'palette' | 'radius' | 'font' | 'showAdult'> & {
  reducedMotion?: boolean
  /** kryo.to's "Hide" for adult games: left out of every list the client draws from kryo.to. */
  hideAdult?: boolean
}

/** What a kryo.to account says about its look (`/api/auth/me`). */
export type AccountAppearance = {
  palette: string | null
  radius: string | null
  typeface: string | null
  nsfwBlur: boolean
  /** kryo.to's "Hide adult games" (Settings > Appearance on the site). */
  nsfwHide?: boolean
  /** kryo.to's "reduce motion": no press, no animations. */
  motion?: string | null
}

const PALETTE_IDS = new Set(PALETTES.map((p) => p.id as string))
const RADIUS_IDS = new Set(RADII.map((r) => r.value as string))

/**
 * The look to draw in: the account's, once kryo.to has said, else the defaults. The site's light theme is not carried
 * over - the client is dark by design - but its palette, corners, typeface
 * and adult blur are.
 */
export function effectiveLook(s: Settings, account: { appearance?: AccountAppearance | null } | null | undefined): Look {
  const a = account?.appearance
  if (!a) return { palette: s.palette, radius: s.radius, font: s.font, showAdult: s.showAdult }
  return {
    palette: (a.palette && PALETTE_IDS.has(a.palette) ? a.palette : 'monochrome') as Palette,
    radius: (a.radius && RADIUS_IDS.has(a.radius) ? a.radius : 'pill') as Radius,
    font: a.typeface === 'mono' ? 'mono' : 'teletext',
    showAdult: !a.nsfwBlur && !a.nsfwHide,
    hideAdult: a.nsfwHide === true,
    reducedMotion: a.motion === 'reduced',
  }
}

/** Stamp the look on <html>, the way kryo.to does before first paint. */
export function applyLook(s: Pick<Settings, 'palette' | 'radius' | 'font'> & { reducedMotion?: boolean }) {
  const html = document.documentElement
  if (s.reducedMotion) html.dataset.motion = 'reduced'
  else delete html.dataset.motion
  html.dataset.palette = s.palette || 'monochrome'
  html.dataset.radius = s.radius || 'pill'
  if (s.font === 'mono') html.dataset.font = 'mono'
  else delete html.dataset.font
}

/* One copy of the settings for the whole window, so a save in Settings
   reaches everything that reads them (the look, the Store's start page). */
let cached: Settings | null = null
const subscribers = new Set<(s: Settings) => void>()
function publish(s: Settings) {
  cached = s
  subscribers.forEach((fn) => fn(s))
}

/** How the Linux build draws its window (src-tauri/src/display_env.rs). */
export type DisplayMode = 'auto' | 'compatible' | 'full'
export type DisplayState = { mode: DisplayMode; reason: string; healed: boolean }

export const displayApi = {
  /** Null outside the Linux build. */
  state: () => call<DisplayState | null>('display_state'),
  /** Takes effect on the next start. */
  setMode: (mode: DisplayMode) => call<void>('display_set_mode', { mode }),
}

export const settingsApi = {
  recover: async () => {
    const settings = await call<Settings>('settings_recover')
    publish(settings)
    return settings
  },
  /** The K// username games get, remembered from the last sign-in. Null for a guest. */
  playerAccountName: () => call<string | null>('player_account_name'),
  get: async () => {
    const s = await call<Settings>('settings_get')
    clearCatalogEndpointCache()
    publish(s)
    return s
  },
  save: async (settings: Settings) => {
    const s = await call<Settings>('settings_save', { settings })
    clearCatalogEndpointCache()
    publish(s)
    return s
  },
  /** Re-read after the native side changed them (Storage writes folders). */
  reload: () => settingsApi.get(),
}

export function useSettings(): Settings | null {
  const [s, setS] = useState<Settings | null>(cached)
  useEffect(() => {
    subscribers.add(setS)
    if (!cached) void settingsApi.get().catch(() => {})
    else setS(cached)
    return () => {
      subscribers.delete(setS)
    }
  }, [])
  return s
}
