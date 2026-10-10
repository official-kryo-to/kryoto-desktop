import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Bell, ChevronDown, Expand, Megaphone, Shrink } from 'lucide-react'
import { artSrc } from '@/lib/art'
import { Matrix, MenuButton, MenuList, type MenuEntry } from '@/ui'
import { KryoMark } from '@/ui/ascii/KryoMark'
import { DevEndpointNotice } from '@/ui/DevEndpointNotice'
import { KryosCoin, compactKryos } from '@/ui/KryosCoin'
import { closeWindow, isTauri, minimizeWindow, toggleFullscreen, toggleMaximize, useWindowState } from '@/lib/window'
import { openInbox } from '@/lib/popup'
import { cn } from '@/lib/utils'
import { isWindowsHost } from '@/lib/library'
import type { Account } from '@/hooks/useAccount'
import type { Inbox, News } from '@/hooks/useInbox'

/**
 * The window's top strip - Forge's title bar, with Steam's jobs in it.
 *
 * Left: the mark and the menus (Kryoto, View, Games, Help). Right: what is new
 * on kryo.to, the notification bell, the account, full screen, and the window
 * buttons. Everything that is not a control drags the window, and a
 * double-click on it maximizes. Menus open in the menu view, over the Store,
 * without hiding it.
 */
export function TitleBar({
  offline = false,
  account,
  inbox,
  news,
  menus,
  accountMenu,
  onNews,
  onOpenNotification,
  onMarkRead,
  onAllNotifications,
  onKryos,
}: {
  /** No connection: a quiet pill says so, and what needs kryo.to waits. */
  offline?: boolean
  account: Account
  inbox: Inbox
  news: News | null
  menus: { label: string; items: MenuEntry[] }[]
  accountMenu: MenuEntry[]
  onNews: () => void
  onOpenNotification: (url: string | null) => void
  onMarkRead: () => void
  onAllNotifications: () => void
  /** The coin beside the account: opens the Hatchery (18+) in the Store. */
  onKryos: () => void
}) {
  const { fullscreen } = useWindowState()

  const initial = (account.displayName || account.username || '?').slice(0, 1).toUpperCase()

  return (
    <header className="relative z-50 flex h-9 shrink-0 select-none items-stretch border-b border-border bg-background">
      <div data-maximize className="drag flex items-center pl-3.5 pr-2">
        <KryoMark className="pointer-events-none h-3.5" />
      </div>
      <nav aria-label="Menus" className="no-drag flex items-center">
        {menus.map((m) => (
          <MenuButton
            key={m.label}
            native={`title:${m.label}`}
            trigger={m.label}
            items={m.items}
            className="kryo-pill h-7 px-2.5 text-[11px] uppercase tracking-wider text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground aria-expanded:bg-secondary aria-expanded:text-foreground"
          />
        ))}
      </nav>
      <div data-maximize className="drag flex grow items-center justify-center gap-2">
        {offline ? (
          <span
            title="No connection. The library, your games and settings work as usual; the Store and downloads wait for it."
            className="no-drag kryo-pill pointer-events-auto flex items-center gap-1.5 border border-border px-2.5 py-0.5 text-[10px] uppercase tracking-wider text-muted-foreground"
          >
            <Matrix state="connect" className="size-3" />
            Offline
          </span>
        ) : null}
        <DevEndpointNotice className="no-drag" />
      </div>

      <div className="no-drag flex items-center gap-1.5 pr-2">
        {news?.version ? (
          <button
            type="button"
            onClick={onNews}
            title={`What's new on kryo.to - ${news.version}`}
            className="kryo-pill flex h-7 items-center gap-1.5 px-2.5 text-[10px] uppercase tracking-wider text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            <Megaphone className="size-3.5" />
            <span>{news.version}</span>
          </button>
        ) : null}

        {account.guest ? null : (
          <MenuButton
            native="bell"
            label={`Notifications${inbox.unreadCount ? `, ${inbox.unreadCount} unread` : ''}`}
            align="right"
            nativeOpen={(anchor) =>
              void openInbox('bell', anchor, inbox, { open: onOpenNotification, markRead: onMarkRead, all: onAllNotifications })
            }
            className="kryo-pill relative grid size-7 place-items-center text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground aria-expanded:bg-secondary"
            trigger={
              <>
                <Bell className="size-3.5" />
                {inbox.unreadCount ? (
                  <span className="kryo-pill absolute -right-0.5 -top-0.5 grid h-3.5 min-w-3.5 place-items-center bg-primary px-1 text-[8px] font-bold text-primary-foreground">
                    {inbox.unreadCount > 9 ? '9+' : inbox.unreadCount}
                  </span>
                ) : null}
              </>
            }
            panel={(close) => (
              <div className="w-80">
                <MenuList
                  onDone={close}
                  items={[
                    { heading: 'Notifications' },
                    ...inbox.notifications.map((n) => ({ label: n.title, onSelect: () => onOpenNotification(n.url) })),
                    { separator: true },
                    { label: 'See all', onSelect: onAllNotifications },
                  ]}
                />
              </div>
            )}
          />
        )}

        {/* Your Kryos, as on kryo.to: the coin and the amount, and a way into
            the Hatchery, which opens behind its own 18+ check. Nothing else. */}
        {!account.guest && typeof account.kryos === 'number' ? (
          <button
            type="button"
            onClick={onKryos}
            title={`${account.kryos.toLocaleString('en-GB')} Kryos · the Hatchery, 18+`}
            aria-label={`You have ${account.kryos.toLocaleString('en-GB')} Kryos. Open the Hatchery, 18+`}
            className="kryo-pill flex h-7 items-center gap-1.5 border border-border px-2.5 text-[11px] tabular-nums text-foreground transition-colors hover:border-foreground"
          >
            <KryosCoin size={14} className="shrink-0" />
            {compactKryos(account.kryos)}
          </button>
        ) : null}

        <MenuButton
          native="account"
          label="Account"
          align="right"
          items={accountMenu}
          className="kryo-pill flex h-7 items-center gap-2 border border-border pl-0.5 pr-2.5 text-[11px] text-foreground transition-colors hover:border-foreground aria-expanded:border-foreground"
          trigger={
            <>
              {account.avatarUrl ? (
                <img src={artSrc(account.avatarUrl) ?? undefined} alt="" className="kryo-pill size-6 object-cover" />
              ) : (
                <span className="kryo-pill grid size-6 place-items-center bg-secondary text-[10px] font-bold">{initial}</span>
              )}
              <span className="max-w-40 truncate">{account.displayName || account.username}</span>
              <ChevronDown className="size-3 text-muted-foreground" />
            </>
          }
        />

        {isTauri() ? (
          <button
            type="button"
            aria-label={fullscreen ? 'Leave full screen' : 'Full screen'}
            title={fullscreen ? 'Leave full screen (F11)' : 'Full screen (F11)'}
            onClick={() => void toggleFullscreen()}
            className="kryo-pill grid size-7 place-items-center text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            {fullscreen ? <Shrink className="size-3.5" /> : <Expand className="size-3.5" />}
          </button>
        ) : null}
      </div>

      {isTauri() ? <WindowControls /> : null}
    </header>
  )
}

/**
 * Minimize, maximize or restore, and close, as Windows 11 draws them: its own
 * glyphs (Segoe Fluent Icons, or MDL2 Assets on Windows 10), 46 pixels wide
 * and the bar's full height, the red close, and dimmed while the window is
 * not the one in front. Elsewhere the same buttons drawn as one-pixel lines.
 * Maximize follows the window (a drag to the top of the screen, a double-click
 * on the bar, Win+Up), not just this button, and hides in full screen.
 */
function WindowControls() {
  const { maximized, fullscreen } = useWindowState()
  const active = useWindowFocus()
  const glyphs = isWindowsHost()
  return (
    <div className={cn('no-drag flex items-stretch', !active && 'opacity-60')}>
      <WindowButton label="Minimize" onClick={() => void minimizeWindow()} glyph={glyphs ? '' : undefined}>
        <path d="M0 5.5h10" />
      </WindowButton>
      {fullscreen ? null : (
        <WindowButton
          label={maximized ? 'Restore down' : 'Maximize'}
          onClick={() => void toggleMaximize()}
          glyph={glyphs ? (maximized ? '' : '') : undefined}
        >
          {maximized ? (
            <>
              <path d="M2.5 2.5V0.5h7v7h-2" />
              <rect x="0.5" y="2.5" width="7" height="7" />
            </>
          ) : (
            <rect x="0.5" y="0.5" width="9" height="9" />
          )}
        </WindowButton>
      )}
      <WindowButton label="Close" onClick={() => void closeWindow()} danger glyph={glyphs ? '' : undefined}>
        <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
      </WindowButton>
    </div>
  )
}

/** Whether this window is the one in front, as Windows dims its buttons when not. */
function useWindowFocus() {
  const [active, setActive] = useState(() => (typeof document === 'undefined' ? true : document.hasFocus()))
  useEffect(() => {
    if (!isTauri()) return
    let stop: (() => void) | undefined
    let cancelled = false
    void getCurrentWindow()
      .onFocusChanged(({ payload }) => setActive(payload))
      .then((fn) => (cancelled ? fn() : (stop = fn)))
    return () => {
      cancelled = true
      stop?.()
    }
  }, [])
  return active
}

function WindowButton({
  label,
  onClick,
  danger = false,
  glyph,
  children,
}: {
  label: string
  onClick: () => void
  danger?: boolean
  /** A Segoe glyph (Windows), else `children` as one-pixel lines. */
  glyph?: string
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      // A press that turns into a drag would otherwise start moving the
      // window from its button.
      onMouseDown={(e) => e.stopPropagation()}
      onClick={onClick}
      className={cn(
        'kryo-square grid w-[46px] place-items-center text-foreground/90 transition-colors duration-75',
        danger
          ? 'hover:bg-[#c42b1c] hover:text-white active:bg-[#c42b1c]/90 active:text-white/80'
          : 'hover:bg-foreground/[0.06] active:bg-foreground/[0.04] active:text-foreground/60',
      )}
    >
      {glyph ? (
        <span className="kryo-caption" aria-hidden>
          {glyph}
        </span>
      ) : (
        // One device pixel wide at any display scale.
        <svg
          viewBox="0 0 10 10"
          className="size-2.5 overflow-visible [&_*]:[vector-effect:non-scaling-stroke]"
          fill="none"
          stroke="currentColor"
          strokeWidth="1"
          aria-hidden
        >
          {children}
        </svg>
      )}
    </button>
  )
}
