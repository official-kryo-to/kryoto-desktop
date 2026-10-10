import { useEffect, useRef, useState } from 'react'
import { artSrc } from '@/lib/art'
import { Bell, Code2, Download, Gamepad2, HardDrive, Heart, LogOut, Palette, ScrollText, Shield, SlidersHorizontal, User, Wrench } from 'lucide-react'
import { AsciiBar, Button, Caption, Check, Section, Segmented, inputCls } from '@/ui'
import { errorText } from '@/lib/bridge'
import { displayApi, PLAYER_NAME_MAX, settingsApi, useSettings, type DisplayMode, type DisplayState, type Settings } from '@/lib/settings'
import { isWindowsHost } from '@/lib/library'
import { browserNavigate, isTauri, mountStore, placeMainStore, STORE_HOME } from '@/lib/window'
import type { Account } from '@/hooks/useAccount'
import type { BrowserPageState } from '@/hooks/useBrowserPage'
import { cn } from '@/lib/utils'
import { chime, setSoundsOn, soundsOn } from '@/lib/sound'
import { StoragePane } from '@/settings/StoragePane'
import { CompatPane } from '@/settings/CompatPane'
import { LogsPane } from '@/settings/LogsPane'
import { ControllerPane } from '@/settings/ControllerPane'

/**
 * Settings: one page for everything you can set.
 *
 * The top of the rail is your kryo.to account - the site's own settings
 * (profile, appearance, notifications, security, support), shown in the
 * right pane exactly as kryo.to has them, so there is one place to change
 * each thing and it is the same place as on the web. Under it, what only the
 * app has. Changes save as you make them.
 */

export type SettingsSection =
  | 'profile'
  | 'appearance'
  | 'notifications'
  | 'security'
  | 'support'
  | 'general'
  | 'storage'
  | 'downloads'
  | 'controller'
  | 'compat'
  | 'developer'
  | 'logs'

/** kryo.to's settings, by the fragment that opens each category there. */
const WEB: Partial<Record<SettingsSection, string>> = {
  profile: 'profile',
  appearance: 'appearance',
  notifications: 'notifications',
  security: 'password',
  support: 'donations',
}

export const isWebSection = (s: SettingsSection) => s in WEB

type RailItem = { id: SettingsSection; label: string; icon: React.ReactNode }

export function SettingsPage({
  section,
  onSection,
  account,
  page,
  onSignOut,
  onSignIn,
}: {
  section: SettingsSection
  onSection: (s: SettingsSection) => void
  account: Account
  page: BrowserPageState
  onSignOut: () => void
  onSignIn: () => void
}) {
  const guest = !!account.guest
  const kryo: RailItem[] = [
    { id: 'profile', label: 'Profile', icon: <User /> },
    { id: 'appearance', label: 'Appearance', icon: <Palette /> },
    { id: 'notifications', label: 'Notifications', icon: <Bell /> },
    { id: 'security', label: 'Security', icon: <Shield /> },
    { id: 'support', label: 'Support', icon: <Heart /> },
  ]
  const desktop: RailItem[] = [
    { id: 'general', label: 'General', icon: <SlidersHorizontal /> },
    { id: 'storage', label: 'Storage', icon: <HardDrive /> },
    { id: 'downloads', label: 'Downloads', icon: <Download /> },
    // Behind kryo.to's `controller` feature flag.
    ...(account.controller ? [{ id: 'controller' as const, label: 'Controller', icon: <Gamepad2 /> }] : []),
    ...(isWindowsHost() ? [] : [{ id: 'compat' as const, label: 'Compatibility', icon: <Wrench /> }]),
    { id: 'developer', label: 'Developer', icon: <Code2 /> },
    { id: 'logs', label: 'Logs', icon: <ScrollText /> },
  ]
  const name = account.displayName || account.username

  return (
    <div className="grid min-h-0 grow grid-cols-[232px_1fr]">
      <nav aria-label="Settings" className="flex min-h-0 flex-col gap-5 overflow-auto border-r border-border bg-card/40 p-3">
        <div className="flex items-center gap-2.5 px-2 pt-2">
          {account.avatarUrl ? (
            <img src={artSrc(account.avatarUrl) ?? undefined} alt="" className="kryo-pill size-9 object-cover" />
          ) : (
            <span className="kryo-pill grid size-9 place-items-center bg-secondary text-xs font-bold">{name.slice(0, 1).toUpperCase()}</span>
          )}
          <span className="grid min-w-0">
            <b className="truncate text-xs text-foreground">{name}</b>
            <span className="truncate text-[10px] text-muted-foreground">{guest ? 'No account' : `@${account.username}`}</span>
          </span>
        </div>
        {guest ? null : <Rail title="kryo.to account" items={kryo} current={section} onSection={onSection} />}
        <Rail title="Kryoto Desktop" items={desktop} current={section} onSection={onSection} />
        <div className="mt-auto px-1">
          {guest ? (
            <Button variant="primary" size="sm" className="w-full" onClick={onSignIn}>
              Sign in to kryo.to
            </Button>
          ) : (
            <Button variant="ghost" size="sm" className="w-full justify-start" onClick={onSignOut}>
              <LogOut className="size-3" />
              Sign out
            </Button>
          )}
        </div>
      </nav>
      {WEB[section] && !guest ? <WebPane fragment={WEB[section]!} page={page} /> : <DesktopPane section={section} />}
    </div>
  )
}

function Rail({ title, items, current, onSection }: { title: string; items: RailItem[]; current: SettingsSection; onSection: (s: SettingsSection) => void }) {
  return (
    <div className="grid gap-1">
      <Caption className="px-2 pb-1">{title}</Caption>
      {items.map((it) => (
        <button
          key={it.id}
          type="button"
          aria-current={current === it.id ? 'page' : undefined}
          onClick={() => onSection(it.id)}
          className={cn(
            'kryo-pill flex h-9 items-center gap-2.5 px-3 text-left text-[11px] uppercase tracking-wider [&>svg]:size-3.5',
            current === it.id ? 'bg-primary font-bold text-primary-foreground' : 'text-muted-foreground hover:bg-secondary hover:text-foreground',
          )}
        >
          {it.icon}
          {it.label}
        </button>
      ))}
    </div>
  )
}

/**
 * kryo.to's settings in the pane: the Store's web view, moved here and sent
 * to the category. It goes back to its own place when you leave; the Store
 * returns to its page when it is next shown (Shell follows its history).
 */
function WebPane({ fragment, page }: { fragment: string; page: BrowserPageState }) {
  const slot = useRef<HTMLDivElement | null>(null)
  const [ready, setReady] = useState(false)

  useEffect(() => {
    const el = slot.current
    if (!el || !isTauri()) return
    const place = () => {
      const r = el.getBoundingClientRect()
      if (r.width > 2 && r.height > 2) void mountStore(STORE_HOME, { x: r.left, y: r.top, width: r.width, height: r.height })
    }
    place()
    const ro = new ResizeObserver(place)
    ro.observe(el)
    return () => {
      ro.disconnect()
      placeMainStore()
    }
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    setReady(false)
    void browserNavigate(`${STORE_HOME}settings#${fragment}`)
      .then(() => setReady(true))
      .catch(() => setReady(true))
  }, [fragment])

  return (
    <div ref={slot} className="relative grid min-h-0 place-content-center justify-items-center gap-3 bg-background">
      {!isTauri() ? (
        <p className="max-w-sm text-center text-xs text-muted-foreground">kryo.to&apos;s settings open here in the app.</p>
      ) : page.error ? (
        <p className="max-w-sm text-center text-xs text-destructive">{page.error.message}</p>
      ) : !ready || page.loading ? (
        <AsciiBar fraction={null} cells={16} showPct={false} className="text-muted-foreground" />
      ) : null}
    </div>
  )
}

/** The app's own settings. Each change is saved straight away. */
function DesktopPane({ section }: { section: SettingsSection }) {
  const stored = useSettings()
  const [s, setS] = useState<Settings | null>(stored)
  const [state, setState] = useState<'idle' | 'saving' | 'saved' | string>('idle')
  const [endpointDraft, setEndpointDraft] = useState(stored?.catalogEndpoint ?? '')
  const timer = useRef<number | undefined>(undefined)
  useEffect(() => {
    if (stored && (!s || (s.recoveryError && !stored.recoveryError))) setS(stored)
  }, [stored, s])
  useEffect(() => {
    if (stored) setEndpointDraft(stored.catalogEndpoint)
  }, [stored?.catalogEndpoint])

  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => patch({ [k]: v } as Partial<Settings>)
  const patch = (p: Partial<Settings>) => {
    if (!s) return
    const next = { ...s, ...p }
    setS(next)
    setState('saving')
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => {
      // Storage changes its folders straight to disk; keep those.
      void settingsApi
        .get()
        .then((latest) => settingsApi.save({ ...next, libraryDir: latest.libraryDir, libraryFolders: latest.libraryFolders }))
        .then(() => setState('saved'))
        .catch((e: unknown) => setState(errorText(e)))
    }, 250)
  }

  if (!s) return <div className="grid place-items-center"><AsciiBar fraction={null} cells={16} showPct={false} /></div>
  const title: Record<string, string> = {
    general: 'General',
    storage: 'Storage',
    downloads: 'Downloads',
    controller: 'Controller',
    compat: 'Compatibility',
    logs: 'Logs',
    developer: 'Developer',
  }

  const applyEndpoint = () => {
    const endpoint = endpointDraft.trim()
    setState('saving')
    void settingsApi
      .get()
      .then((latest) => settingsApi.save({ ...latest, catalogEndpoint: endpoint }))
      .then((saved) => {
        setS(saved)
        setEndpointDraft(saved.catalogEndpoint)
        setState('saved')
      })
      .catch((e: unknown) => setState(errorText(e)))
  }

  return (
    <div className="min-h-0 overflow-auto">
      {/* The controller page is wider: the drawing and its labels need the room. */}
      <div className={cn('mx-auto grid gap-7 px-8 py-8', section === 'controller' ? 'max-w-5xl' : 'max-w-3xl')}>
        <header className="flex items-baseline justify-between gap-4">
          <h1 className="text-xl font-bold text-foreground">{title[section]}</h1>
          <span className={cn('text-[10px] uppercase tracking-wider', state === 'saved' || state === 'saving' || state === 'idle' ? 'text-muted-foreground' : 'text-destructive')}>
            {state === 'saving' ? 'Saving' : state === 'saved' ? 'Saved' : state === 'idle' ? '' : state}
          </span>
        </header>

        {section === 'storage' ? <StoragePane onChanged={() => void settingsApi.get().then((v) => setS((cur) => (cur ? { ...cur, libraryDir: v.libraryDir, libraryFolders: v.libraryFolders } : v)))} /> : null}
        {section === 'downloads' ? (
          <>
            <Section title="Connections" hint="Connections per download. More may improve speed if the filehost allows it.">
              <Segmented
                label="Download connections" value={String(s.connections)}
                options={['1', '4', '8', '16', '32'].map((v) => ({ value: v, label: v }))}
                onChange={(v) => set('connections', Number(v))}
              />
            </Section>
            <Section title="Speed limit">
              <Segmented
                label="Download speed limit" value={String(s.speedLimitMb)}
                options={[
                  { value: '0', label: 'None' },
                  { value: '5', label: '5 MB/s' },
                  { value: '10', label: '10 MB/s' },
                  { value: '25', label: '25 MB/s' },
                  { value: '50', label: '50 MB/s' },
                ]}
                onChange={(v) => set('speedLimitMb', Number(v))}
              />
            </Section>
            <Check checked={s.deleteArchives} onChange={(v) => set('deleteArchives', v)} label="Delete the archive once a game is installed" />
            <Check checked={s.notifyDownloads} onChange={(v) => set('notifyDownloads', v)} label="Tell me when a game is ready to play" />
            <SoundCheck />
            <Section title="New games install to" hint="Change it, or add folders on other drives, in Storage.">
              <p className="kryo-ascii-art select-text text-[11px] text-foreground">{s.libraryDir}</p>
            </Section>
          </>
        ) : null}
        {section === 'general' ? (
          <>
            <Section title="Open on">
              <Segmented label="Open on" value={s.startPage} options={[{ value: 'library', label: 'Library' }, { value: 'store', label: 'Store' }]} onChange={(v) => set('startPage', v)} />
            </Section>
            <Check checked={s.closeToTray} onChange={(v) => set('closeToTray', v)} label="Closing the window keeps Kryoto running in the tray" />
            <Check checked={s.startWithSystem} onChange={(v) => set('startWithSystem', v)} label={`Start Kryoto when I sign in to ${isWindowsHost() ? 'Windows' : 'my computer'}`} />
            <Check checked={s.minimizeOnPlay} onChange={(v) => set('minimizeOnPlay', v)} label="Minimize Kryoto while a game runs" />
            <PlayerNameSection s={s} patch={patch} />
            <Section title="Play time" hint="Counts toward the community statistics, and toward what people are playing right now (a number per game, never who). The play-time board only shows public profiles.">
              <Check checked={s.sharePlaytime} onChange={(v) => set('sharePlaytime', v)} label="Share my play time and what I am playing with kryo.to" />
            </Section>
            <GraphicsSection />
          </>
        ) : null}
        {section === 'controller' ? <ControllerPane /> : null}
        {section === 'compat' ? <CompatPane s={s} set={set} /> : null}
        {section === 'developer' ? (
          <Section title="Kryo.to endpoint" hint="Blank uses production. For local testing, use http://localhost:3000.">
            <div className="grid gap-3">
              <label className="grid gap-1.5 text-[10px] uppercase tracking-wider text-muted-foreground">
                Site origin
                <input
                  className={inputCls}
                  type="url"
                  value={endpointDraft}
                  onChange={(e) => setEndpointDraft(e.target.value)}
                  placeholder="https://kryo.to"
                  spellCheck={false}
                />
              </label>
              <p className="text-[10px] leading-relaxed text-muted-foreground">
                This changes the Store, catalog requests and download metadata. Sign in separately on the selected site.
                HTTP is allowed on localhost only.
              </p>
              <Button
                variant="primary"
                size="sm"
                onClick={applyEndpoint}
                disabled={state === 'saving' || endpointDraft.trim() === s.catalogEndpoint}
              >
                Apply endpoint
              </Button>
            </div>
          </Section>
        ) : null}
        {section === 'logs' ? <LogsPane sendReports={s.sendReports} onSendReports={(v) => set('sendReports', v)} /> : null}
      </div>
    </div>
  )
}

/**
 * The name you have in games. Ticked: your K// username. Unticked: the name
 * typed here, or - left empty - whatever each build came with. Written into
 * the game's emulator files as it starts (src-tauri player_name.rs).
 */
function PlayerNameSection({ s, patch }: { s: Settings; patch: (p: Partial<Settings>) => void }) {
  const [account, setAccount] = useState<string | null>(null)
  useEffect(() => {
    if (isTauri()) void settingsApi.playerAccountName().then(setAccount).catch(() => {})
  }, [])
  const useAccount = s.playerNameMode === 'account'
  return (
    <Section
      title="In-game name"
      hint="What games and other players call you, in games built with gbe_fork or RUNE (most of the catalog). Set it per game under Properties. Kryoto Online games use your Steam name."
    >
      <div className="grid gap-3">
        <Check
          checked={useAccount}
          onChange={(on) => patch({ playerNameMode: on ? 'account' : s.playerName.trim() ? 'custom' : 'build' })}
          label={account ? `Use my K// username (${account})` : 'Use my K// username (sign in to kryo.to to set it)'}
        />
        {!useAccount ? (
          <label className="grid gap-1.5 text-[10px] uppercase tracking-wider text-muted-foreground">
            Custom name
            <input
              className={inputCls}
              value={s.playerName}
              maxLength={PLAYER_NAME_MAX}
              placeholder="Empty keeps each game's own name"
              spellCheck={false}
              onChange={(e) => patch({ playerName: e.target.value, playerNameMode: e.target.value.trim() ? 'custom' : 'build' })}
            />
          </label>
        ) : null}
      </div>
    </Section>
  )
}

/**
 * The one sound (lib/sound.ts), off unless asked for. Turning it on plays it
 * once, so the choice is made knowing what it sounds like.
 */
function SoundCheck() {
  const [on, setOn] = useState(soundsOn)
  return (
    <Check
      checked={on}
      onChange={(v) => {
        setOn(v)
        setSoundsOn(v)
        if (v) chime({ force: true })
      }}
      label="Play a short sound when a game is ready, while Kryoto Desktop is in front"
    />
  )
}

const DISPLAY_HINT: Record<DisplayMode, string> = {
  auto: 'Hardware rendering, except where it is known to break: NVIDIA drivers and machines without a GPU render device.',
  compatible: 'Draws on anything, a little slower. Use it when pages are blank, flicker or are drawn in the wrong place.',
  full: 'Hardware rendering everywhere. Only if Automatic picked Compatible and your machine handles the fast path.',
}

/**
 * How the Linux build draws its window (src-tauri/src/display_env.rs). Shown
 * only there; takes effect on the next start.
 */
function GraphicsSection() {
  const [state, setState] = useState<DisplayState | null>(null)
  const [picked, setPicked] = useState<DisplayMode | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    void displayApi
      .state()
      .then(setState)
      .catch(() => setState(null))
  }, [])
  if (!state) return null
  const mode = picked ?? state.mode
  const choose = (m: DisplayMode) => {
    setError(null)
    void displayApi
      .setMode(m)
      .then(() => setPicked(m))
      .catch((e: unknown) => setError(errorText(e)))
  }
  return (
    <Section title="Graphics" hint={DISPLAY_HINT[mode]}>
      <Segmented
        value={mode}
        options={[
          { value: 'auto', label: 'Automatic' },
          { value: 'compatible', label: 'Compatible' },
          { value: 'full', label: 'Full' },
        ]}
        onChange={(v) => choose(v as DisplayMode)}
      />
      <p className="text-[11px] text-muted-foreground">
        {state.healed ? 'The last start did not draw its window, so Kryoto switched to Compatible. ' : ''}
        Now: {state.reason}.{picked && picked !== state.mode ? ' Restart Kryoto to use the new setting.' : ''}
      </p>
      {error ? <p className="text-[11px] text-destructive">{error}</p> : null}
    </Section>
  )
}
