import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Play, Square } from 'lucide-react'
import { Matrix } from '@/ui'
import { KryoMark } from '@/ui/ascii/KryoMark'
import { Art } from '@/library/Art'
import { PadBadge } from '@/controller/PadArtView'
import { usePadStore } from '@/hooks/usePads'
import { backOf, confirmOf, familyOf, padLabel } from '@/lib/pad'
import type { PadControl, PadFamily } from '@/lib/pad-art'
import type { LibraryGame } from '@/lib/library'
import { isTauri } from '@/lib/window'
import { cn } from '@/lib/utils'

/**
 * Big Picture: Kryoto for the couch, made for a controller.
 *
 * Full screen, the games big, every action a button: the d-pad or stick
 * moves (lib/pad-nav.ts, scoped to this as a dialog), A opens and plays,
 * B goes back (detail, then the grid, then out), the bumpers switch rows,
 * Home leaves. The buttons are named the way the pad in hand prints them, in
 * the bar along the bottom. Works with a mouse and keyboard too (Esc leaves).
 */

type Tab = 'library' | 'recent'

export function BigPicture({
  games,
  running,
  onPlay,
  onStop,
  onExit,
  playerName,
}: {
  games: LibraryGame[]
  running: Set<string>
  onPlay: (g: LibraryGame) => void
  onStop: (g: LibraryGame) => void
  onExit: () => void
  playerName: string
}) {
  const { pads, config } = usePadStore()
  const pad = pads[0] ?? null
  const family: PadFamily = familyOf(pad, pad ? config?.pads[pad.guid] : null)
  const [tab, setTab] = useState<Tab>('library')
  const [open, setOpen] = useState<LibraryGame | null>(null)
  const root = useRef<HTMLDivElement>(null)
  const clock = useClock()

  const recent = useMemo(() => games.filter((g) => g.lastPlayed).sort((a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0)), [games])
  const shown = useMemo(() => {
    if (tab === 'recent') return recent
    return games.slice().sort((a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' }))
  }, [tab, games, recent])
  const hero = recent[0] ?? shown[0] ?? null

  // Full screen while it is open, as it was before afterwards.
  useEffect(() => {
    if (!isTauri()) return
    const win = getCurrentWindow()
    let entered = false
    void win.isFullscreen().then((fs) => {
      if (fs) return
      entered = true
      void win.setFullscreen(true)
    })
    return () => {
      if (entered) void win.setFullscreen(false)
    }
  }, [])

  // The controller starts on something: the open game's Play, else the hero.
  useLayoutEffect(() => {
    const first = root.current?.querySelector<HTMLElement>('[data-bp-first]')
    first?.focus({ preventScroll: true })
    document.documentElement.classList.add('kryo-pad')
  }, [open, tab])

  // B (lib/pad-nav.ts asks first), Esc, the bumpers and Home.
  useEffect(() => {
    const back = (e: Event) => {
      e.preventDefault()
      if (open) setOpen(null)
      else onExit()
    }
    const page = (e: Event) => {
      e.preventDefault()
      setOpen(null)
      setTab((t) => (t === 'library' ? 'recent' : 'library'))
    }
    const guide = (e: Event) => {
      e.preventDefault()
      onExit()
    }
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      if (open) setOpen(null)
      else onExit()
    }
    window.addEventListener('kryo-pad-back', back)
    window.addEventListener('kryo-pad-tab', page)
    window.addEventListener('kryo-pad-guide', guide)
    window.addEventListener('keydown', key)
    return () => {
      window.removeEventListener('kryo-pad-back', back)
      window.removeEventListener('kryo-pad-tab', page)
      window.removeEventListener('kryo-pad-guide', guide)
      window.removeEventListener('keydown', key)
    }
  }, [open, onExit])

  const hint = (c: PadControl, what: string) => (
    <span className="flex items-center gap-2">
      <PadBadge family={family} control={c} label={padLabel(family, c).long} className="h-7 min-w-7 text-[11px]" />
      {what}
    </span>
  )

  return (
    <div
      ref={root}
      role="dialog"
      aria-modal="true"
      aria-label="Big Picture"
      className="kryo-bp fixed inset-0 z-[650] grid grid-rows-[auto_1fr_auto] overflow-hidden bg-background text-foreground"
    >
      {/* Top: the mark, the rows, the time. */}
      <header className="flex items-center gap-8 px-12 pb-4 pt-8">
        <KryoMark className="h-6" />
        <nav aria-label="Big Picture rows" className="flex gap-2">
          {(['library', 'recent'] as const).map((t) => (
            <button
              key={t}
              type="button"
              onClick={() => {
                setOpen(null)
                setTab(t)
              }}
              aria-current={tab === t ? 'page' : undefined}
              className={cn(
                'kryo-pill h-10 px-5 text-sm font-bold uppercase tracking-wider transition-colors',
                tab === t ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground',
              )}
            >
              {t === 'library' ? 'Library' : 'Recently played'}
            </button>
          ))}
        </nav>
        <div className="grow" />
        <span className="flex items-center gap-2 text-sm text-muted-foreground">
          {pad ? <Matrix state="online" className="size-3.5 text-success" /> : null}
          {pad ? pad.name : 'No controller'}
        </span>
        <span className="text-lg font-bold tabular-nums">{clock}</span>
        <span className="text-sm text-muted-foreground">{playerName}</span>
      </header>

      <main className="min-h-0 overflow-auto px-12 pb-8 pt-3">
        {open ? (
          <Detail game={open} running={running.has(open.id)} onPlay={() => onPlay(open)} onStop={() => onStop(open)} family={family} />
        ) : (
          <div className="grid gap-10">
            {hero && tab === 'library' ? (
              <button
                type="button"
                data-bp-first
                onClick={() => setOpen(hero)}
                // A wide tile grows less, so it stays inside the page.
                style={{ ['--bp-grow' as string]: 1.012 }}
                className="kryo-bp-tile kryo-radius relative h-[38vh] min-h-56 overflow-hidden border border-border text-left"
              >
                <Art adult={hero.nsfw} src={hero.hero} fallback={[hero.header, hero.cover]} title={hero.title} className="absolute inset-0 size-full object-cover" />
                <span className="hero-side absolute inset-0" />
                <span className="absolute bottom-8 left-10 grid gap-2">
                  <span className="text-xs uppercase tracking-[0.25em] text-muted-foreground">{hero.lastPlayed ? 'Continue playing' : 'In your library'}</span>
                  <span className="text-4xl font-bold">{hero.title}</span>
                </span>
              </button>
            ) : null}
            {shown.length ? (
              <section aria-label={tab === 'library' ? 'All games' : 'Recently played'} className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6">
                {shown.map((g, i) => (
                  <button
                    key={g.id}
                    type="button"
                    data-bp-first={!(hero && tab === 'library') && i === 0 ? true : undefined}
                    onClick={() => setOpen(g)}
                    className="kryo-bp-tile kryo-radius group relative aspect-[2/3] overflow-hidden border border-border bg-card text-left"
                  >
                    <Art adult={g.nsfw} src={g.cover} fallback={[g.header, g.hero]} title={g.title} className="absolute inset-0 size-full object-cover" />
                    <span className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/85 to-transparent px-3 pb-3 pt-10 text-sm font-bold text-white">
                      {g.title}
                    </span>
                    {running.has(g.id) ? (
                      <span className="kryo-pill absolute right-2 top-2 flex items-center gap-1.5 bg-success px-2.5 py-1 text-[10px] font-bold uppercase text-black">
                        <Matrix state="busy" className="size-2.5" />
                        Running
                      </span>
                    ) : null}
                  </button>
                ))}
              </section>
            ) : (
              <p className="py-20 text-center text-lg text-muted-foreground">
                {tab === 'recent' ? 'Nothing played yet.' : 'No games yet. Get some from the Store, then come back here.'}
              </p>
            )}
          </div>
        )}
      </main>

      {/* What each button does here, named the way this pad prints it. */}
      <footer className="flex items-center gap-8 border-t border-border bg-card/60 px-12 py-4 text-sm text-muted-foreground">
        {hint(confirmOf(family), open ? (running.has(open.id) ? 'Stop' : 'Play') : 'Open')}
        {hint(backOf(family), open ? 'Back' : 'Leave Big Picture')}
        {open ? null : (
          <span className="flex items-center gap-2">
            <PadBadge family={family} control="leftshoulder" label={padLabel(family, 'leftshoulder').long} className="h-7 min-w-7 text-[11px]" />
            <PadBadge family={family} control="rightshoulder" label={padLabel(family, 'rightshoulder').long} className="h-7 min-w-7 text-[11px]" />
            Library or Recently played
          </span>
        )}
        <span className="grow" />
        <button type="button" onClick={onExit} className="flex items-center gap-2 hover:text-foreground">
          <PadBadge family={family} control="guide" label={padLabel(family, 'guide').long} className="h-7 min-w-7 text-[11px]" />
          Leave
        </button>
      </footer>
    </div>
  )
}

function Detail({ game, running, onPlay, onStop, family }: { game: LibraryGame; running: boolean; onPlay: () => void; onStop: () => void; family: PadFamily }) {
  const hours = Math.floor(game.playtimeSeconds / 3600)
  const minutes = Math.floor((game.playtimeSeconds % 3600) / 60)
  return (
    <div className="kryo-in relative grid min-h-full content-end overflow-hidden">
      <Art adult={game.nsfw} src={game.hero} fallback={[game.header, game.cover]} title={game.title} className="kryo-radius absolute inset-0 size-full object-cover opacity-70" />
      <span className="hero-fade absolute inset-0" />
      <div className="relative grid gap-5 p-10">
        <h1 className="text-5xl font-bold">{game.title}</h1>
        <p className="text-sm text-muted-foreground">
          {game.playtimeSeconds ? `${hours ? `${hours} h ` : ''}${minutes} min played` : 'Not played yet'}
          {game.developer ? ` · ${game.developer}` : ''}
        </p>
        {game.short ? <p className="max-w-2xl text-sm leading-relaxed text-muted-foreground">{game.short}</p> : null}
        <div className="flex gap-3">
          <button
            type="button"
            data-bp-first
            onClick={running ? onStop : onPlay}
            className={cn(
              'kryo-bp-tile kryo-pill flex h-14 items-center gap-3 px-10 text-base font-bold uppercase tracking-wider',
              running ? 'bg-secondary text-foreground' : 'bg-primary text-primary-foreground',
            )}
          >
            <PadBadge family={family} control={confirmOf(family)} label={padLabel(family, confirmOf(family)).long} className="h-7 min-w-7" />
            {running ? (
              <>
                <Square className="size-5" /> Stop
              </>
            ) : (
              <>
                <Play className="size-5" /> Play
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  )
}

function useClock() {
  const now = () => new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  const [t, setT] = useState(now)
  useEffect(() => {
    const i = window.setInterval(() => setT(now()), 10_000)
    return () => window.clearInterval(i)
  }, [])
  return t
}
