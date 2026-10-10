import { useMemo } from 'react'
import { ChevronDown, Play } from 'lucide-react'
import { Button, Label, MenuButton } from '@/ui'
import { capsulesFor, type LibraryGame } from '@/lib/library'
import { formatLastPlayed, formatPlaytime } from '@/lib/format'
import { usePersisted } from '@/hooks/usePersisted'
import { Art } from '@/library/Art'

export { Art }

type Sort = 'alpha' | 'recent' | 'playtime' | 'added'

const SORTS: Record<Sort, string> = {
  alpha: 'A to Z',
  recent: 'Last played',
  playtime: 'Hours played',
  added: 'Recently added',
}
const isSort = (v: string): v is Sort => v in SORTS

/**
 * Library home. The game you played last, big, one press from Play - then
 * Steam's Recent shelf and every game as a poster grid with Play on hover.
 */
export function LibraryHome({
  games,
  running,
  onOpen,
  onPlay,
  onContext,
}: {
  games: LibraryGame[]
  running: Set<string>
  onOpen: (id: string) => void
  onPlay: (game: LibraryGame) => void
  onContext: (game: LibraryGame, x: number, y: number) => void
}) {
  const [sort, setSort] = usePersisted<Sort>('kryoto.library.sort', 'alpha', isSort)
  const recent = useMemo(
    () => games.filter((g) => g.lastPlayed).sort((a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0)),
    [games],
  )
  const sorted = useMemo(() => {
    const list = games.slice()
    if (sort === 'alpha') list.sort((a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' }))
    if (sort === 'recent') list.sort((a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0))
    if (sort === 'playtime') list.sort((a, b) => b.playtimeSeconds - a.playtimeSeconds)
    if (sort === 'added') list.sort((a, b) => b.addedAt - a.addedAt)
    return list
  }, [games, sort])
  const last = recent[0]
  const ctx = (g: LibraryGame) => (e: React.MouseEvent) => {
    e.preventDefault()
    onContext(g, e.clientX, e.clientY)
  }

  return (
    <div className="grid min-h-0 grow content-start gap-8 overflow-auto p-6">
      {last ? (
        <section
          onContextMenu={ctx(last)}
          className="kryo-radius kryo-in relative h-64 overflow-hidden border border-border bg-card"
        >
          <Art adult={last.nsfw} src={last.hero} fallback={[last.header, last.cover]} title="" className="absolute inset-0 size-full object-cover" />
          <div className="hero-side absolute inset-0" />
          <div className="relative flex h-full flex-col justify-end gap-3 p-7">
            <span className="text-[10px] uppercase tracking-[0.25em] text-muted-foreground">
              {running.has(last.id) ? 'Playing now' : `Continue · ${formatLastPlayed(last.lastPlayed)}`}
            </span>
            <button type="button" onClick={() => onOpen(last.id)} className="kryo-square w-fit text-left">
              <h1 className="max-w-xl text-3xl font-bold leading-tight text-foreground">{last.title}</h1>
            </button>
            <div className="flex items-center gap-3">
              <Button variant="primary" size="lg" onClick={() => onPlay(last)} disabled={running.has(last.id)}>
                <Play className="size-4 fill-current" />
                {running.has(last.id) ? 'Running' : 'Play'}
              </Button>
              <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
                {formatPlaytime(last.playtimeSeconds)} played
              </span>
            </div>
          </div>
        </section>
      ) : null}

      {recent.length > 1 ? (
        <section className="grid min-w-0 gap-3">
          <Label>Recent</Label>
          <div className="grid auto-cols-[minmax(260px,1fr)] grid-flow-col gap-3 overflow-x-auto pb-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
            {recent.slice(1, 7).map((g) => (
              <button
                key={g.id}
                type="button"
                onClick={() => onOpen(g.id)}
                onDoubleClick={() => onPlay(g)}
                onContextMenu={ctx(g)}
                className="kryo-radius group grid overflow-hidden border border-border bg-card text-left transition-[transform,border-color] duration-200 hover:-translate-y-0.5 hover:border-foreground/50"
              >
                <Art adult={g.nsfw} src={capsulesFor(g)[0]} fallback={capsulesFor(g).slice(1)} title={g.title} className="aspect-[460/215] w-full object-cover" />
                <span className="grid gap-0.5 px-3 py-2.5">
                  <span className="truncate text-xs font-bold text-foreground">{g.title}</span>
                  <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
                    {running.has(g.id) ? 'Running now' : `${formatLastPlayed(g.lastPlayed)} · ${formatPlaytime(g.playtimeSeconds)}`}
                  </span>
                </span>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      <section className="grid min-w-0 gap-3">
        <div className="flex items-center gap-3">
          <Label>All games · {games.length}</Label>
          <span className="h-px grow bg-border" />
          <MenuButton
            align="right"
            className="kryo-pill flex h-7 items-center gap-1.5 border border-border px-3 text-[10px] uppercase tracking-wider text-muted-foreground hover:text-foreground"
            trigger={
              <>
                {SORTS[sort]}
                <ChevronDown className="size-3" />
              </>
            }
            items={(Object.keys(SORTS) as Sort[]).map((s) => ({ label: SORTS[s], onSelect: () => setSort(s) }))}
          />
        </div>
        <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-4">
          {sorted.map((g) => (
            <div
              key={g.id}
              onContextMenu={ctx(g)}
              className="kryo-radius group relative aspect-[2/3] overflow-hidden border border-border bg-card transition-[transform,border-color] duration-200 hover:-translate-y-1 hover:border-foreground/60"
            >
              <button type="button" onClick={() => onOpen(g.id)} className="kryo-square absolute inset-0" aria-label={`Open ${g.title}`}>
                <Art adult={g.nsfw} src={g.cover} fallback={[g.header]} title={g.title} className="size-full object-cover" />
              </button>
              {running.has(g.id) ? (
                <span className="kryo-pill absolute left-2 top-2 flex items-center gap-1 bg-background/80 px-2 py-0.5 text-[9px] font-bold uppercase tracking-wider text-success backdrop-blur">
                  <span className="kryo-blink size-1.5 rounded-full bg-success" />
                  Running
                </span>
              ) : null}
              <button
                type="button"
                aria-label={`Play ${g.title}`}
                onClick={() => onPlay(g)}
                className="kryo-pill absolute bottom-3 right-3 grid size-11 min-h-[44px] min-w-[44px] translate-y-2 place-items-center bg-primary text-primary-foreground opacity-0 shadow-lg shadow-black/50 transition-all duration-200 group-hover:translate-y-0 group-hover:opacity-100 group-focus-within:translate-y-0 group-focus-within:opacity-100 focus-visible:translate-y-0 focus-visible:opacity-100 [@media(pointer:coarse)]:translate-y-0 [@media(pointer:coarse)]:opacity-100"
              >
                <Play className="size-4 fill-current" />
              </button>
            </div>
          ))}
        </div>
      </section>
    </div>
  )
}

