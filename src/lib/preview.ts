/**
 * Browser preview: the native side, played by an in-memory stand-in.
 *
 * Only used when the UI runs in a plain browser (`pnpm dev`) - the desktop
 * app never loads any of this. It exists so every screen of the client can
 * be opened, clicked and checked without building Rust: a small library with
 * Captain Hardcore's real launch entries, a download that moves, a game that
 * runs for a few seconds and adds to its playtime.
 */

type Handler = (p: unknown) => void
const listeners = new Map<string, Set<Handler>>()

export const previewBus = {
  on(event: string, fn: Handler) {
    if (!listeners.has(event)) listeners.set(event, new Set())
    listeners.get(event)!.add(fn)
    return () => listeners.get(event)?.delete(fn)
  },
  emit(event: string, payload: unknown) {
    listeners.get(event)?.forEach((fn) => fn(payload))
  },
}

const now = () => Math.floor(Date.now() / 1000)
const steam = (appid: number, file: string) => `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/${file}`

function game(appid: number, title: string, extra: Record<string, unknown> = {}) {
  return {
    id: title.toLowerCase().replace(/[^a-z0-9]+/g, '-'),
    title,
    slug: title.toLowerCase().replace(/[^a-z0-9]+/g, '-'),
    cover: steam(appid, 'library_600x900.jpg'),
    hero: steam(appid, 'library_hero.jpg'),
    installDir: `C:\\Users\\you\\Kryoto Games\\${title}`,
    executable: `${title}.exe`,
    defaultArgs: '',
    entries: [],
    source: 'Steam + gbe_fork',
    preferredEntry: null,
    launchOptions: '',
    compatTool: null,
    applyOverrides: true,
    playtimeSeconds: 0,
    lastPlayed: null,
    addedAt: now() - 86400 * 20,
    version: 'b1',
    short: null,
    developer: null,
    nsfw: false,
    ...extra,
  }
}

const games = [
  game(1190600, 'Captain Hardcore', {
    executable: 'Captain Hardcore.exe',
    entries: [
      { executable: 'Captain Hardcore.exe', arguments: '', workingdir: '', description: '', oslist: 'windows', type: 'vr' },
      {
        executable: 'Captain Hardcore.exe',
        arguments: '-nohmd',
        workingdir: '',
        description: 'Captain Hardcore Desktop Mode',
        oslist: 'windows',
        type: 'default',
      },
    ],
    source: 'Steam (DRM-free)',
    playtimeSeconds: 5400,
    lastPlayed: now() - 86400 * 4,
    short: 'A VR action game with a desktop mode behind -nohmd.',
    developer: 'AntiZero Games',
    nsfw: true,
    version: 'b20488383',
  }),
  game(367520, 'Hollow Knight', { playtimeSeconds: 162000, lastPlayed: now() - 3600 * 3, source: 'Steam (DRM-free)' }),
  game(504230, 'Celeste', { playtimeSeconds: 30000, lastPlayed: now() - 86400 * 9 }),
  game(1145360, 'Hades', { playtimeSeconds: 72000, lastPlayed: now() - 86400 * 30, source: 'Steam + Kryoto Online' }),
  game(413150, 'Stardew Valley', { playtimeSeconds: 0 }),
  game(105600, 'Terraria', {
    playtimeSeconds: 900,
    lastPlayed: now() - 86400 * 60,
    source: 'Steam + online-fix',
  }),
] as Record<string, unknown>[]

const running = new Set<string>()
const settings: Record<string, unknown> = {
  libraryDir: 'C:\\Users\\you\\Kryoto Games',
  deleteArchives: true,
  startPage: 'library',
  defaultCompatTool: null,
  minimizeOnPlay: false,
  notifyDownloads: true,
  palette: 'monochrome',
  radius: 'pill',
  font: 'teletext',
  showAdult: false,
  followAccount: true,
  libraryFolders: ['D:\\Games\\Kryoto'],
  sendReports: true,
  closeToTray: true,
  startWithSystem: false,
  pressEffect: true,
  sharePlaytime: true,
  connections: 16,
  speedLimitMb: 0,
  catalogEndpoint: '',
  linuxMangohud: false,
  linuxGamemode: false,
  linuxFsr: false,
}

const GB = 1_073_741_824
const stored = (id: string, bytes: number) => {
  const g = games.find((x) => x.id === id) ?? {}
  return { id, title: String(g.title ?? id), folder: `C:\\Users\\you\\Kryoto Games\\${String(g.title ?? id)}`, bytes, lastPlayed: g.lastPlayed ?? null, cover: g.cover ?? null, nsfw: !!g.nsfw }
}

const LOG = [
  '2026-09-25 14:03:07Z INFO  app: Kryoto Desktop 0.2.2 started on windows',
  '2026-09-25 14:03:09Z INFO  storage: added library folder D:\\Games\\Kryoto',
  '2026-09-25 14:05:41Z ERROR download: dl-1: The connection kept dropping (timed out). Resume to try again.',
  '2026-09-25 14:07:12Z INFO  library: uninstalling Terraria from C:\\Users\\you\\Kryoto Games\\Terraria',
]

const dl = (over: Record<string, unknown>) => ({
  id: 'dl-1',
  slug: 'hades-ii',
  url: 'https://dl.kryo.to/d/preview',
  fileName: 'Hades II - Kryoto.7z',
  archivePath: '',
  total: 11_200_000_000,
  received: 3_900_000_000,
  speed: 38_000_000,
  extracted: 0,
  extractTotal: null,
  status: 'downloading',
  error: null,
  installDir: null,
  gameId: null,
  addedAt: now() - 600,
  finishedAt: null,
  meta: {
    title: 'Hades II',
    cover: steam(1145350, 'library_600x900.jpg'),
    hero: steam(1145350, 'library_hero.jpg'),
    executable: 'Hades2.exe',
    entries: [],
    source: 'Steam + gbe_fork',
    version: 'b1',
    sizeBytes: 11_200_000_000,
  },
  ...over,
})

const downloadList: Record<string, unknown>[] = [
  dl({}),
  dl({
    id: 'dl-0',
    slug: 'celeste',
    status: 'installed',
    received: 1_200_000_000,
    total: 1_200_000_000,
    speed: 0,
    finishedAt: now() - 86400,
    gameId: 'celeste',
    meta: {
      ...dl({}).meta,
      title: 'Celeste',
      cover: steam(504230, 'library_600x900.jpg'),
      hero: steam(504230, 'library_hero.jpg'),
    },
  }),
]

// Initialized only by the browser-preview backend, never by native/web chat.
let simulation: ReturnType<typeof setInterval> | undefined
export function stopPreview() {
  clearInterval(simulation)
  simulation = undefined
}
export function startPreview() {
  if (simulation !== undefined) return
  simulation = setInterval(() => {
  const d = downloadList[0]
  if (!d || d.status !== 'downloading') return
  d.received = Math.min(d.total as number, (d.received as number) + (d.speed as number) / 2)
  d.speed = 30_000_000 + Math.round(Math.random() * 16_000_000)
  if (d.received === d.total) d.status = 'extracting'
  previewBus.emit('downloads', downloadList.map((x) => ({ ...x })))
}, 500)
}
if (import.meta.hot) import.meta.hot.dispose(stopPreview)

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T

let previewChat: { state: string } = { state: 'off' }

// A small pretend chat for `pnpm dev`: messages live in memory, the other
// side "reads" and answers after a moment. Ids match lib/chat's conversation id.
type PreviewMsg = {
  msgId: string; conversationId: string; senderUser: string; outgoing: boolean; sentAt: number; receivedAt: number
  kind: 'text' | 'gif' | 'invite'; body: string; replyTo: string | null; editedAt: number | null; deleted: boolean
  status: string; reactions: { emoji: string; userIds: string[] }[]
}
const previewMsgs: PreviewMsg[] = []
const previewVerified = new Set<string>()
let previewGroups: { id: string; name: string; members: { userId: string; role: string }[]; myRole: string }[] = [
  { id: '900', name: 'Lethal crew', members: [{ userId: '1', role: 'owner' }, { userId: '2', role: 'member' }, { userId: '8', role: 'member' }], myRole: 'owner' },
]
let previewBackup = { exists: false, updatedAtMs: 0, keptHere: false }
let previewSettings = { readReceipts: true, typing: true, notificationContent: 'name', gifsAuto: true }
const convOf = (peer: string) => {
  if (peer.startsWith('g:')) return `6772703a${BigInt(peer.slice(2)).toString(16).padStart(16, '0')}`
  const [lo, hi] = BigInt('1') <= BigInt(peer) ? [1n, BigInt(peer)] : [BigInt(peer), 1n]
  return `646d3a${lo.toString(16).padStart(16, '0')}${hi.toString(16).padStart(16, '0')}`
}
const newId = () => Array.from({ length: 16 }, () => Math.floor(Math.random() * 256).toString(16).padStart(2, '0')).join('')
const update = (m: PreviewMsg) => previewBus.emit('chat-updated', { ...m, reactions: m.reactions.map((r) => ({ ...r })) })
function previewSeed() {
  if (previewMsgs.length) return
  const now = Date.now()
  previewMsgs.push(
    { msgId: newId(), conversationId: convOf('2'), senderUser: '2', outgoing: false, sentAt: now - 3_600_000, receivedAt: now - 3_600_000, kind: 'text', body: 'Lethal Company tonight?', replyTo: null, editedAt: null, deleted: false, status: 'received', reactions: [] },
    { msgId: newId(), conversationId: convOf('2'), senderUser: '1', outgoing: true, sentAt: now - 3_500_000, receivedAt: now - 3_500_000, kind: 'text', body: 'yes! after dinner', replyTo: null, editedAt: null, deleted: false, status: 'read', reactions: [{ emoji: '🔥', userIds: ['2'] }] },
    { msgId: newId(), conversationId: convOf('6'), senderUser: '6', outgoing: false, sentAt: now - 600_000, receivedAt: now - 600_000, kind: 'text', body: 'hey, saw your review of OFME, can I ask something?', replyTo: null, editedAt: null, deleted: false, status: 'received', reactions: [] },
  )
}

const HANDLERS: Record<string, (a: Record<string, unknown>) => unknown> = {
  chat_status: () => previewChat,
  chat_set_context: () => null,
  chat_conversations: () => {
    previewSeed()
    const peers = [...new Set(previewMsgs.map((m) => m.conversationId))]
    return peers.map((c) => {
      const list = previewMsgs.filter((m) => m.conversationId === c)
      const last = list[list.length - 1] as PreviewMsg
      return { id: c, peerUserId: last.outgoing ? '2' : last.senderUser, lastAt: last.sentAt, unread: list.filter((m) => !m.outgoing && m.status === 'received').length, last }
    })
  },
  chat_messages: (a) => {
    previewSeed()
    return previewMsgs.filter((m) => m.conversationId === convOf(String(a.peer)))
  },
  chat_mark_read: (a) => {
    for (const m of previewMsgs) if (m.conversationId === convOf(String(a.peer)) && !m.outgoing) m.status = 'read'
    return null
  },
  chat_typing: () => null,
  chat_settings_get: () => previewSettings,
  chat_settings_set: (a) => {
    previewSettings = a.settings as typeof previewSettings
    return null
  },
  chat_gif_search: () => ({ gifs: [], attribution: 'Powered by GIPHY' }),
  chat_message_request: (a) => {
    const username = String(a?.username ?? '').replace(/^@/, '')
    if (username === 'nobody') throw new Error(`Nobody is called ${username}.`)
    return { status: 'requested', userId: '77', username, name: username }
  },
  chat_message_request_respond: () => null,
  chat_verify_info: (a) =>
    String(a.peer) === '3'
      ? { safetyNumber: [], verified: false, pendingChange: true }
      : { safetyNumber: ['05213', '88410', '29371', '64102', '77215', '30981', '45120', '99832', '11046', '58723', '20419', '67310'], verified: previewVerified.has(String(a.peer)), pendingChange: false },
  chat_verify_mark: (a) => {
    if (a.verified) previewVerified.add(String(a.peer))
    else previewVerified.delete(String(a.peer))
    return null
  },
  chat_identity_ack: () => null,
  chat_backup_status: () => previewBackup,
  chat_backup_create: () => {
    previewBackup = { exists: true, updatedAtMs: Date.now(), keptHere: true }
    return '7K2M-9QXR-4HWD-P8TC-3NVB-6YJF-1GSE-5ZAQ-0RKD-8MXT-2HPW-4CNV-B000'
  },
  chat_backup_delete: () => {
    previewBackup = { exists: false, updatedAtMs: 0, keptHere: false }
    return null
  },
  chat_devices: () => [
    { deviceId: '2', kind: 'desktop', name: 'GAMING-PC', certified: true, createdAtMs: Date.now() - 40 * 86_400_000, lastSeenDayMs: Date.now(), current: true },
    { deviceId: '9', kind: 'desktop', name: 'LAPTOP', certified: true, createdAtMs: Date.now() - 9 * 86_400_000, lastSeenDayMs: Date.now() - 3 * 86_400_000, current: false },
  ],
  chat_device_revoke: () => null,
  chat_unlock: () => previewChat,
  chat_restore: () => null,
  chat_reset_identity: () => null,
  chat_send_invite: (a) => {
    const inv = a.invite as Record<string, unknown>
    const m: PreviewMsg = { msgId: newId(), conversationId: convOf(String(a.peer)), senderUser: '1', outgoing: true, sentAt: Date.now(), receivedAt: Date.now(), kind: 'invite', body: JSON.stringify({ ...inv, expiresAt: Date.now() + 900_000 }), replyTo: null, editedAt: null, deleted: false, status: 'sent', reactions: [] }
    previewMsgs.push(m)
    previewBus.emit('chat-message', m)
    return m
  },
  online_lobby: () => ({ lobby: '109775241075364289', hostSteamId: '76561198000000000' }),
  steam_join_lobby: () => null,
  chat_report: () => null,
  chat_send_file: () => null,
  room_list: () => ({
    canModerate: false,
    messages: [{ id: '1', body: 'anyone up for Lethal Company tonight?', createdAt: new Date().toISOString(), user: { id: '2', username: 'bo', displayName: 'Bo', avatarUrl: null, supporter: true } }],
  }),
  room_post: () => ({}),
  room_delete: () => null,
  chat_call_signal: () => null,
  chat_ice_servers: () => ({ iceServers: [{ urls: 'stun:stun.cloudflare.com:3478' }] }),
  chat_file_save: () => null,
  chat_file_preview: () => {
    throw new Error('No preview in the browser preview.')
  },
  chat_groups: () => previewGroups,
  chat_group_create: (a) => {
    const g = { id: String(1000 + previewGroups.length), name: String(a.name), members: [{ userId: '1', role: 'owner' }, ...(a.members as string[]).map((userId) => ({ userId, role: 'member' }))], myRole: 'owner' }
    previewGroups = [g, ...previewGroups]
    return g
  },
  chat_group_rename: () => null,
  chat_group_add: (a) => previewGroups.find((g) => g.id === a.group),
  chat_group_remove: (a) => {
    previewGroups = previewGroups.filter((g) => g.id !== a.group)
    return null
  },
  chat_people: () => ({ people: [{ id: '8', username: 'rook', displayName: 'Rook', avatarUrl: null, supporter: false }] }),
  chat_export_history: () => 'C:\Users\you\Documents\kryoto-chat-history.json',
  chat_send: (a) => {
    const peer = String(a.peer)
    const m: PreviewMsg = { msgId: newId(), conversationId: convOf(peer), senderUser: '1', outgoing: true, sentAt: Date.now(), receivedAt: Date.now(), kind: 'text', body: String(a.text), replyTo: (a.replyTo as string) ?? null, editedAt: null, deleted: false, status: 'sent', reactions: [] }
    previewMsgs.push(m)
    setTimeout(() => { m.status = 'delivered'; update(m) }, 600)
    setTimeout(() => previewBus.emit('chat-typing', { conversationId: m.conversationId, userId: peer, active: true }), 900)
    setTimeout(() => {
      m.status = 'read'
      update(m)
      const reply: PreviewMsg = { ...m, msgId: newId(), senderUser: peer, outgoing: false, sentAt: Date.now(), receivedAt: Date.now(), body: 'sounds good', replyTo: null, status: 'received', reactions: [] }
      previewMsgs.push(reply)
      previewBus.emit('chat-message', reply)
    }, 2200)
    return m
  },
  chat_edit: (a) => {
    const m = previewMsgs.find((x) => x.msgId === a.msg)
    if (!m) throw new Error('No such message.')
    m.body = String(a.text)
    m.editedAt = Date.now()
    return m
  },
  chat_delete: (a) => {
    const m = previewMsgs.find((x) => x.msgId === a.msg)
    if (m) { m.deleted = true; m.body = ''; m.reactions = []; update(m) }
    return null
  },
  chat_react: (a) => {
    const m = previewMsgs.find((x) => x.msgId === a.msg)
    if (!m) return null
    const emoji = String(a.emoji)
    let r = m.reactions.find((x) => x.emoji === emoji)
    if (a.remove) { if (r) r.userIds = r.userIds.filter((u) => u !== '1'); m.reactions = m.reactions.filter((x) => x.userIds.length) }
    else { if (!r) { r = { emoji, userIds: [] }; m.reactions.push(r) } if (!r.userIds.includes('1')) r.userIds.push('1') }
    update(m)
    return null
  },
  chat_enable: () => {
    previewChat = { state: 'connecting' }
    setTimeout(() => {
      previewChat = { state: 'online', userId: '1', deviceId: '2' } as { state: string }
      previewBus.emit('chat-status', previewChat)
    }, 800)
    return previewChat
  },
  chat_remove_device: () => {
    previewChat = { state: 'off' }
    previewBus.emit('chat-status', previewChat)
  },
  compat_status: () => ({
    tools: [
      { name: 'GE-Proton10-17', path: '/home/you/.local/share/to.kryo.desktop/compat/GE-Proton10-17/proton', kind: 'proton', managed: true },
      { name: 'Proton 9.0 (Beta)', path: '/home/you/.steam/steam/steamapps/common/Proton 9.0 (Beta)/proton', kind: 'proton', managed: false },
      { name: 'umu-run', path: '/home/you/.local/share/to.kryo.desktop/compat/umu/umu-run', kind: 'umu', managed: true },
      { name: 'wine', path: '/usr/bin/wine', kind: 'wine', managed: false },
    ],
    umu: '/home/you/.local/share/to.kryo.desktop/compat/umu/umu-run',
    mangohud: true,
    gamemode: false,
  }),
  library_list: () => clone(games),
  game_running: () => [...running],
  library_save: (a) => {
    const next = a.game as Record<string, unknown>
    const i = games.findIndex((g) => g.id === next.id)
    const old = games[i]
    if (!old) return clone(next)
    games[i] = { ...next, playtimeSeconds: old.playtimeSeconds, lastPlayed: old.lastPlayed }
    return clone(games[i])
  },
  library_add: (a) => {
    const g = { ...(a.game as Record<string, unknown>) }
    const path = String(a.exePath)
    g.id = String(g.slug || g.title || 'game').toLowerCase().replace(/[^a-z0-9]+/g, '-')
    g.installDir = path.replace(/[\\/][^\\/]+$/, '')
    g.executable = path.split(/[\\/]/).pop()
    g.addedAt = now()
    games.push(g)
    return clone(g)
  },
  library_remove: (a) => {
    const i = games.findIndex((g) => g.id === a.id)
    if (i >= 0) games.splice(i, 1)
    return false
  },
  game_launch: (a) => {
    const id = String(a.id)
    running.add(id)
    const g = games.find((x) => x.id === id)
    if (g) g.lastPlayed = now()
    previewBus.emit('game-state', { id, running: true, seconds: null, code: null })
    setTimeout(() => {
      running.delete(id)
      if (g) g.playtimeSeconds = (g.playtimeSeconds as number) + 600
      previewBus.emit('game-state', { id, running: false, seconds: 600, code: 0 })
    }, 6000)
    return null
  },
  game_stop: (a) => {
    running.delete(String(a.id))
    previewBus.emit('game-state', { id: a.id, running: false, seconds: 60, code: 1 })
    return null
  },
  game_launch_preview: (a) => {
    const g = a.game as { installDir: string; executable: string; entries: { executable: string; arguments: string }[]; launchOptions: string; defaultArgs: string }
    const e = a.entry != null ? g.entries[a.entry as number] : null
    const exe = e ? e.executable : g.executable
    const args = [e ? e.arguments : g.defaultArgs, g.launchOptions.replace(/^.*%command%/, '').trim()].filter(Boolean).join(' ')
    return `"${g.installDir}\\${exe}"${args ? ` ${args}` : ''}`
  },
  game_disk_size: () => 14_300_000_000,
  open_folder: () => null,
  settings_get: () => ({ ...settings }),
  // Every sample game can use Kryoto Online, so the add-ons card shows it.
  online_check: () => null,
  settings_save: (a) => Object.assign(settings, a.settings),
  downloads_list: () => clone(downloadList),
  storage_overview: () => ({
    folders: [
      {
        path: String(settings.libraryDir),
        drive: 'C:',
        isDefault: true,
        exists: true,
        total: 953 * GB,
        free: 212 * GB,
        gamesBytes: 41.3 * GB,
        games: [stored('captain-hardcore', 22.1 * GB), stored('stardew-valley', 0.6 * GB), stored('celeste', 1.2 * GB)],
      },
      { path: 'D:\\Games\\Kryoto', drive: 'D:', isDefault: false, exists: true, total: 1863 * GB, free: 1204 * GB, gamesBytes: 0, games: [] },
    ],
    elsewhere: [stored('terraria', 0.4 * GB)],
  }),
  storage_add_folder: (a) => {
    ;(settings.libraryFolders as string[]).push(String(a.path))
    return null
  },
  storage_remove_folder: () => null,
  storage_set_default: () => null,
  storage_move: () => null,
  logs_tail: () => LOG.join('\n'),
  logs_folder: () => 'C:\\Users\\you\\AppData\\Roaming\\to.kryo.desktop\\logs',
  logs_send: () => 1,
  log_write: () => null,
  shell_ready: () => null,
  download_pause: (a) => {
    const d = downloadList.find((x) => x.id === a.id)
    if (d) Object.assign(d, { status: 'paused', speed: 0 })
    previewBus.emit('downloads', clone(downloadList))
    return null
  },
  download_resume: (a) => {
    const d = downloadList.find((x) => x.id === a.id)
    if (d) d.status = 'downloading'
    previewBus.emit('downloads', clone(downloadList))
    return null
  },
  download_cancel: (a) => {
    const d = downloadList.find((x) => x.id === a.id)
    if (d) Object.assign(d, { status: 'canceled', speed: 0, received: 0 })
    previewBus.emit('downloads', clone(downloadList))
    return null
  },
  download_remove: (a) => {
    const i = downloadList.findIndex((x) => x.id === a.id)
    if (i >= 0) downloadList.splice(i, 1)
    previewBus.emit('downloads', clone(downloadList))
    return null
  },
}

/** Controllers: a DualSense plugged in, an Xbox pad seen before. */
const previewPads = [
  {
    id: 0,
    guid: '030000004c050000e60c000011810000',
    name: 'Sony Interactive Entertainment DualSense Wireless Controller',
    vendor: 0x054c,
    product: 0x0ce6,
    model: { family: 'playstation', name: 'DualSense', features: { touchpad: true, gyro: true, paddles: 0, misc: true, analogTriggers: true }, matched: 'ids' },
    rumble: true,
    battery: 80,
    charging: false,
  },
]
let previewPadConfig = {
  enabled: true,
  navigate: true,
  notify: true,
  games: true,
  haptics: true,
  pads: {
    '030000004c050000e60c000011810000': { name: 'DualSense', family: null, remap: {}, sdl: null },
    '030000005e040000120b00000b050000': { name: 'Xbox Series X|S Controller', family: null, remap: { a: 'b', b: 'a' }, sdl: null },
  } as Record<string, unknown>,
}
Object.assign(HANDLERS, {
  pad_start: () => previewPads,
  pad_stop: () => null,
  pad_list: () => previewPads,
  pad_config_get: () => previewPadConfig,
  pad_config_set: (a: Record<string, unknown>) => (previewPadConfig = a.config as typeof previewPadConfig),
  pad_rumble: () => null,
  pad_haptic: () => null,
  // Repair (src-tauri/src/repair.rs): the shared source list, nothing applied yet.
  repair_sources: () => [
    { id: 'gbe_fork', label: 'gbe_fork', online: false },
    { id: 'online', label: 'Kryoto Online', online: true },
    { id: 'rune', label: 'RUNE', online: false },
    { id: 'rune_steak', label: 'RUNE (Steakclient)', online: false },
    { id: 'rune_steamclient', label: 'RUNE (Steamclient)', online: false },
  ],
  repair_state: () => ({ undo_available: false, interrupted: false, report: null }),
})

export function previewCall<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const handler = HANDLERS[command]
  if (!handler) return Promise.reject(new Error(`${command} needs the desktop app.`))
  return new Promise((resolve, reject) =>
    setTimeout(() => {
      try {
        resolve(handler(args) as T)
      } catch (e) {
        reject(e)
      }
    }, 60),
  )
}
