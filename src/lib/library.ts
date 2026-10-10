import { call, errorText, on } from '@/lib/bridge'
import { logWarn } from '@/lib/log'
import { catalogApiUrl } from '@/lib/endpoint'

/**
 * The Library: games on this PC and how each one starts.
 *
 * Mirrors `src-tauri/src/library.rs`. What a Play press actually runs is
 * decided in Rust (`launch.rs`); this side picks, shows and saves.
 */

export type LaunchEntry = {
  executable: string
  arguments: string
  workingdir: string
  description: string
  oslist: string
  /** Steam's kind: "default", "vr", "config"... */
  type?: string
}

export type LibraryGame = {
  id: string
  title: string
  slug: string | null
  cover: string | null
  hero: string | null
  /** The transparent title logo, when Steam has one. */
  logo?: string | null
  /** The wide store header, for rows and cards. */
  header?: string | null
  installDir: string
  executable: string
  defaultArgs: string
  entries: LaunchEntry[]
  source: string | null
  preferredEntry: number | null
  launchOptions: string
  compatTool: string | null
  applyOverrides: boolean
  playtimeSeconds: number
  lastPlayed: number | null
  addedAt: number
  version: string | null
  /** A build picked under Versions: while it is installed, no update is offered. */
  pinnedVersion?: string | null
  short: string | null
  developer: string | null
  /** kryo.to marks it an adult game; its art is blurred unless Settings says otherwise. */
  nsfw: boolean
  /** Add-ons put into it, with the files each wrote (for Undo). */
  addons?: InstalledAddon[]
  /** Kryoto Online set up on this PC. */
  online?: { version: string; added: string[]; saved: [string, string][]; appliedAt: number } | null
  /** This game's in-game name, over Settings'; null follows Settings. */
  playerNameMode?: import('@/lib/settings').PlayerNameMode | null
  playerName?: string
}

export type InstalledAddon = { label: string; file: string; files: string[]; installedAt: number }

/** One of a game's add-ons on kryo.to (`/api/games/<slug>/downloads`). */
export type KryoAddon = {
  id: number | string
  label: string | null
  note: string | null
  source: string | null
  version: string | null
  download_size: string | null
  links: { name: string | null }[]
  /**
   * kryo.to's word on whether to offer it for this game: `false` for the
   * Online add-on on a game Steam lists no online play for, or whose release
   * already brings its own. Absent from older answers.
   */
  offered?: boolean
}

export async function fetchAddons(slug: string): Promise<KryoAddon[]> {
  const res = await fetch(await catalogApiUrl(`/api/games/${encodeURIComponent(slug)}/downloads`))
  if (!res.ok) return []
  const json = (await res.json()) as { addons?: KryoAddon[] }
  return json.addons ?? []
}

/** Where a release can be downloaded from, as Kryoto Desktop sees it. */
export type ReleaseSource =
  /** Our own copy: through the Store's download sheet (it passes kryo.to's check). */
  | { kind: 'ours' }
  /** A mirror Kryoto downloads itself. `page` hosts open their page and may ask for a check. */
  | { kind: 'mirror'; host: string; url: string; page: boolean }

/** One full release of a game (`releases` in `/api/games/<slug>/downloads`). */
export type KryoRelease = {
  id: string
  version: string
  label: string | null
  source: string | null
  downloadSize: string | null
  createdAt: string | null
  /** The current release: what Install and updates get. */
  primary: boolean
  /** We no longer keep our own copy: mirrors only. */
  archived: boolean
  sources: ReleaseSource[]
  /** Mirrors Kryoto can't fetch itself (MEGA, torrents): open in the browser. */
  elsewhere: { host: string; url: string }[]
}

type RawLink = { host?: string; url?: string }
type RawRelease = {
  id: string | number
  version?: string | null
  label?: string | null
  source?: string | null
  download_size?: string | null
  created_at?: string | null
  primary?: boolean
  archived?: boolean
  links?: RawLink[]
}

/**
 * A game's releases, newest first, each with the ways Kryoto can get it. Our
 * own copy leads (fastest, and checked against its hash); mirrors the app
 * resolves by itself follow, API hosts before page hosts.
 */
export async function fetchReleases(slug: string): Promise<KryoRelease[]> {
  const res = await fetch(await catalogApiUrl(`/api/games/${encodeURIComponent(slug)}/downloads`))
  if (!res.ok) throw new Error(res.status === 404 ? `kryo.to has no game at /game/${slug}.` : `kryo.to answered ${res.status}.`)
  const json = (await res.json()) as {
    releases?: RawRelease[]
    version?: string
    download_size?: string | null
    links?: RawLink[]
  }
  // A kryo.to from before `releases`: the current release is all it names.
  const raw: RawRelease[] = json.releases ?? [
    { id: 'current', version: json.version ?? '', download_size: json.download_size ?? null, primary: true, links: json.links ?? [] },
  ]
  const external = raw.flatMap((r) => (r.links ?? []).map((l) => l.url ?? '')).filter((u) => /^https?:\/\//.test(u))
  const known = new Map<string, { host: string; kind: string } | null>()
  if (external.length) {
    const kinds = await call<({ host: string; kind: string } | null)[]>('mirror_hosts', { urls: external }).catch(() => [])
    external.forEach((u, i) => known.set(u, kinds[i] ?? null))
  }
  return raw.map((r) => {
    const links = r.links ?? []
    const ours = links.some((l) => l.url?.startsWith('/api/download/'))
    const mirrors: ReleaseSource[] = []
    const elsewhere: { host: string; url: string }[] = []
    for (const l of links) {
      if (!l.url || l.url.startsWith('/')) continue
      const k = known.get(l.url)
      if (k) mirrors.push({ kind: 'mirror', host: k.host, url: l.url, page: k.kind === 'page' })
      else elsewhere.push({ host: l.host ?? 'Mirror', url: l.url })
    }
    mirrors.sort((a, b) => Number(a.kind === 'mirror' && a.page) - Number(b.kind === 'mirror' && b.page))
    return {
      id: String(r.id),
      version: r.version ?? '',
      label: r.label ?? null,
      source: r.source ?? null,
      downloadSize: r.download_size ?? null,
      createdAt: r.created_at ?? null,
      primary: !!r.primary,
      archived: !!r.archived,
      sources: [...(ours && !r.archived ? [{ kind: 'ours' } as const] : []), ...mirrors],
      elsewhere,
    }
  })
}

export type GameStateEvent = { id: string; running: boolean; seconds: number | null; code: number | null }

export const BLANK_GAME: LibraryGame = {
  id: '',
  title: '',
  slug: null,
  cover: null,
  hero: null,
  installDir: '',
  executable: '',
  defaultArgs: '',
  entries: [],
  source: null,
  preferredEntry: null,
  launchOptions: '',
  compatTool: null,
  applyOverrides: true,
  playtimeSeconds: 0,
  lastPlayed: null,
  addedAt: 0,
  version: null,
  short: null,
  developer: null,
  nsfw: false,
}

/** One launch's log (src-tauri/src/game_logs.rs). */
export type GameLogInfo = { name: string; size: number; modified: number }

export const gameLogs = {
  list: (id: string) => call<GameLogInfo[]>('game_logs', { id }),
  read: (id: string, name: string) => call<string>('game_log_read', { id, name }),
  folder: (id: string) => call<string>('game_logs_folder', { id }),
  /** The game's Wine prefix on Linux; null on Windows or before its first start. */
  prefix: (id: string) => call<string | null>('library_prefix_folder', { id }),
}

export const library = {
  list: () => call<LibraryGame[]>('library_list'),
  add: (exePath: string, game: Partial<LibraryGame>) =>
    call<LibraryGame>('library_add', { exePath, game: { ...BLANK_GAME, ...game } }),
  save: (game: LibraryGame) => call<LibraryGame>('library_save', { game }),
  remove: (id: string, deleteFiles: boolean) => call<boolean>('library_remove', { id, deleteFiles }),
  /** `joinLobby`: a Steam lobby id from a game invite (Kryoto Online games). */
  launch: (id: string, entry: number | null, joinLobby?: string) => call<void>('game_launch', { id, entry, joinLobby: joinLobby ?? null }),
  preview: (game: LibraryGame, entry: number | null) => call<string>('game_launch_preview', { game, entry }),
  running: () => call<string[]>('game_running'),
  stop: (id: string) => call<void>('game_stop', { id }),
  diskSize: (installDir: string) => call<number>('game_disk_size', { installDir }),
  /**
   * Show a folder. `create` for a library folder that may not exist yet; a
   * missing game folder shows the nearest one above it. Never rejects: a
   * folder that cannot be shown is logged, not thrown at the window.
   */
  openFolder: (path: string, create = false) =>
    call<void>('open_folder', { path, create }).catch((e) => logWarn('library', `open folder: ${errorText(e)}`)),
  onState: (fn: (e: GameStateEvent) => void) => on<GameStateEvent>('game-state', fn),
  onChanged: (fn: () => void) => on<unknown>('library-changed', fn),
  addonUndo: (gameId: string, file: string) => call<LibraryGame>('addon_undo', { gameId, file }),
  /** `null` when the game can use Kryoto Online, else why it cannot. */
  onlineCheck: (gameId: string) => call<string | null>('online_check', { gameId }),
  onlineApply: (gameId: string) => call<LibraryGame>('online_apply', { gameId }),
  onlineUndo: (gameId: string) => call<LibraryGame>('online_undo', { gameId }),
}

/* ── kryo.to ─────────────────────────────────────────────── */

export type CatalogGame = {
  slug: string
  title: string
  cover: string | null
  hero: string | null
  logo: string | null
  header: string | null
  screenshots: string[]
  executable: string
  defaultArgs: string
  entries: LaunchEntry[]
  source: string | null
  version: string | null
  short: string | null
  developer: string | null
  nsfw: boolean
}

/** A kryo.to game page link or a bare slug, as a slug. */
export function slugFrom(input: string): string | null {
  const text = input.trim()
  if (!text) return null
  const fromUrl = text.match(/kryo\.to\/game\/([a-z0-9-]+)/i)
  if (fromUrl?.[1]) return fromUrl[1].toLowerCase()
  return /^[a-z0-9-]+$/i.test(text) ? text.toLowerCase() : null
}

/**
 * A release's facts from kryo.to's public game API: Steam's launch entries,
 * the exe and arguments staff picked, art, version, and the source label
 * (which says whether Wine needs DLL overrides).
 */
export async function fetchCatalogGame(slug: string): Promise<CatalogGame> {
  const res = await fetch(await catalogApiUrl(`/api/games/${encodeURIComponent(slug)}`))
  if (res.status === 404) throw new Error(`kryo.to has no game at /game/${slug}.`)
  if (!res.ok) throw new Error(`kryo.to answered ${res.status}.`)
  const { game, art } = (await res.json()) as { game: Record<string, unknown>; art?: Record<string, unknown> }
  const artUrl = (k: string) => (typeof art?.[k] === 'string' && (art[k] as string).trim() ? (art[k] as string).trim() : null)
  const str = (v: unknown) => (typeof v === 'string' && v.trim() ? v.trim() : null)
  const available = (game.game_launch_options as { available?: LaunchEntry[] } | null)?.available
  // A Steam branch listed as its own game carries a suffix (GoreBox's
  // "2027330x"); Steam's art is under the number alone.
  const appid = str(game.steam_appid)?.match(/^\d+/)?.[0] ?? null
  return {
    slug,
    title: str(game.title) ?? slug,
    // `art` is what kryo.to resolved from Steam (real, hashed URLs); the
    // legacy appid paths are only a last guess, and they 404 for new games.
    cover: artUrl('capsule') ?? str(game.cover_vertical) ?? str(game.cover),
    hero:
      artUrl('hero') ??
      str(game.hero_image_override) ??
      (appid ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/library_hero.jpg` : str(game.cover_horizontal)),
    logo: artUrl('logo'),
    header: artUrl('header') ?? str(game.cover_horizontal) ?? str(game.cover),
    screenshots: Array.isArray(art?.screenshots) ? (art.screenshots as unknown[]).filter((u): u is string => typeof u === 'string') : [],
    executable: str(game.game_executable_path) ?? '',
    defaultArgs: str(game.game_executable_args) ?? '',
    entries: Array.isArray(available) ? available.filter(isWindowsEntry) : [],
    source: str(game.source),
    version: str(game.version),
    short: str(game.short),
    developer: str(game.developer),
    nsfw: game.nsfw === true,
  }
}

/** Steam's logo art for the game page, from the hero URL's app id. */
export function logoFor(game: LibraryGame): string | null {
  const m = (game.hero ?? game.cover ?? '').match(/\/apps\/(\d+)\//)
  return m ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${m[1]}/logo.png` : null
}

/** Steam's landscape capsule, for the recent-games shelf. */
/**
 * The wide picture for a row or card, best first: the store header kryo.to
 * resolved, then the legacy Steam path (a guess that 404s for newer games),
 * then the cover.
 */
export function capsulesFor(game: { hero?: string | null; cover: string | null; header?: string | null }): (string | null)[] {
  const m = (game.hero ?? game.cover ?? '').match(/\/apps\/(\d+)\//)
  return [game.header ?? null, m ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${m[1]}/header.jpg` : null, game.cover]
}

/* ── Launch entries ──────────────────────────────────────── */

const TYPE_LABELS: Record<string, string> = {
  vr: 'Play in VR',
  safemode: 'Safe mode',
  config: 'Configure',
  editor: 'Editor',
  server: 'Dedicated server',
  manual: 'Manual',
  option1: 'Alternative launch 1',
  option2: 'Alternative launch 2',
  option3: 'Alternative launch 3',
}

export function isWindowsEntry(e: LaunchEntry): boolean {
  const os = (e.oslist ?? '').toLowerCase()
  return !!e.executable && (!os || os.includes('windows'))
}

export function entryLabel(e: LaunchEntry): string {
  return e.description?.trim() || TYPE_LABELS[(e.type ?? '').toLowerCase()] || 'Play'
}

export function entryIsVr(e: LaunchEntry): boolean {
  return (e.type ?? '').toLowerCase() === 'vr' || /\bvr\b/i.test(entryLabel(e))
}

/** Whether Play has a question to ask: two ways in, or one needing a flag. */
export function hasChoice(game: LibraryGame): boolean {
  return game.entries.length > 1 || game.entries.some((e) => e.arguments.trim())
}

/** The entry the release is set up with on kryo.to, else Steam's first. */
export function releaseDefaultEntry(game: LibraryGame): number | null {
  if (game.entries.length === 0) return null
  const exe = game.executable.replace(/\\/g, '/').toLowerCase()
  const i = game.entries.findIndex(
    (e) =>
      e.executable.replace(/\\/g, '/').toLowerCase() === exe &&
      e.arguments.trim() === game.defaultArgs.trim(),
  )
  return i >= 0 ? i : 0
}

/** What Play starts without asking, or `'ask'`. */
export function playTarget(game: LibraryGame): number | null | 'ask' {
  if (game.preferredEntry != null && game.entries[game.preferredEntry]) return game.preferredEntry
  if (hasChoice(game)) return 'ask'
  return game.entries.length === 1 ? 0 : null
}

/* ── Launch option presets (the Steam non-Steam-game guide) ── */

export const PRESETS = [
  {
    id: 'kryoto-online',
    label: 'Kryoto Online',
    line: 'WINEDLLOVERRIDES="steam_api64=n,b;kryotoO=n,b;photon_universal=n,b" %command%',
  },
  {
    id: 'online-fix',
    label: 'Online-Fix',
    line: 'WINEDLLOVERRIDES="OnlineFix64=n;SteamOverlay64=n;winmm=n,b;dnet=n;steam_api64=n" %command%',
  },
] as const

/** Which preset a release's source label calls for, if any. */
export function presetFor(source: string | null): (typeof PRESETS)[number] | null {
  const s = (source ?? '').toLowerCase()
  if (s.includes('kryoto online')) return PRESETS[0]
  if (/online-?fix|\bofme\b/.test(s)) return PRESETS[1]
  return null
}

/** The line for Steam's LAUNCH OPTIONS box, when played through Steam. */
export function steamLine(game: LibraryGame, entry: number | null): string {
  const args = entry != null ? (game.entries[entry]?.arguments ?? '') : game.defaultArgs
  const preset = presetFor(game.source)
  return [preset ? preset.line : '%command%', args.trim()].filter(Boolean).join(' ')
}

export const isWindowsHost = () => /Windows/i.test(navigator.userAgent)
