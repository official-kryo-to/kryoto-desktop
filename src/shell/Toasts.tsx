import { useCallback, useEffect, useState } from 'react'
import { Play } from 'lucide-react'
import { Matrix } from '@/ui'
import { on } from '@/lib/bridge'
import type { Notice } from '@/lib/downloads'
import { notify } from '@/lib/notify'
import { chime } from '@/lib/sound'

export type Toast = Notice & { key: number; controllerGuid?: string }

/** Notices from Rust (`notify`) and from the shell, each for seven seconds. */
export function useToasts() {
  const [toasts, setToasts] = useState<Toast[]>([])
  const push = useCallback((n: Notice & { controllerGuid?: string }) => {
    const key = Date.now() + Math.random()
    setToasts((t) => [...t.slice(-2), { ...n, key }])
    setTimeout(() => setToasts((t) => t.filter((x) => x.key !== key)), n.controllerGuid ? 15000 : 7000)
  }, [])
  const dismiss = useCallback((key: number) => setToasts((t) => t.filter((x) => x.key !== key)), [])
  useEffect(() => {
    let stop: (() => void) | undefined
    let cancelled = false
    // The app's news (a game ready to play, an add-on applied): in the app,
    // and from the system too when the window is not in front.
    void on<Notice>('notify', (n) => {
      push(n)
      if (!document.hasFocus()) void notify(n.title, n.body)
      // In front, a game becoming ready gets the optional sound instead.
      else if (n.gameId) chime()
    }).then((fn) => (cancelled ? fn() : (stop = fn)))
    return () => {
      cancelled = true
      stop?.()
    }
  }, [push])
  return { toasts, push, dismiss }
}

/**
 * Corner pop-ups, the site's toast: a card with a white badge. Only drawn
 * where nothing native covers the corner; over the Store the footer carries
 * the latest one instead.
 */
export function Toasts({ toasts, onOpen, onDismiss }: { toasts: Toast[]; onOpen: (t: Toast) => void; onDismiss: (key: number) => void }) {
  return (
    <div aria-live="polite" className="fixed bottom-14 right-4 z-[600] grid gap-2">
      {toasts.map((t) => (
        <button
          key={t.key}
          type="button"
          onClick={() => {
            onDismiss(t.key)
            onOpen(t)
          }}
          className="kryo-toast kryo-radius grid w-80 grid-cols-[36px_1fr] items-center gap-3 border border-border bg-card p-3 text-left shadow-2xl shadow-black/60"
        >
          <span className="kryo-pill grid size-9 place-items-center bg-primary text-primary-foreground">
            {t.gameId ? <Play className="size-4" /> : <Matrix state="info" className="size-3.5" />}
          </span>
          <span className="min-w-0">
            <span className="block truncate text-xs font-bold text-foreground">{t.title}</span>
            <span className="block text-[11px] leading-snug text-muted-foreground">{t.body}</span>
          </span>
        </button>
      ))}
    </div>
  )
}
