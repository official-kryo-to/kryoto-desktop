import { useMemo, useState } from 'react'
import { artSrc, reportArt } from '@/lib/art'
import { usePersisted } from '@/hooks/usePersisted'
import { Download as DownloadIcon, Home, Search } from 'lucide-react'
import { asciiTrack, Dropdown } from '@/ui'
import { isActive, progressOf, type Download } from '@/lib/downloads'
import type { LibraryGame } from '@/lib/library'
import { cn } from '@/lib/utils'
import { adultBlur, useHideAdult, useShowAdult } from '@/lib/adult'
import { STATUSES, STATUS_LABEL, useAdultFlags, type SavedEntry, type SavedStatus } from '@/hooks/useSaved'

type Shelf = 'installed' | 'saved' | SavedStatus

const isShelf = (v: string): v is Shelf => v === 'installed' || v === 'saved' || (STATUSES as string[]).includes(v)

/** The search, kept while the client is open (the Library remounts on every visit). */
let lastQuery = ''

/**
 * The Library's left rail: Home, search, every game - and under them the ones
 * still downloading, with their ASCII progress, the way Steam lists a game
 * you own but have not finished installing.
 *
 * "Show" switches the list to one of the account's kryo.to shelves (Playing,
 * Plan to Play, Favorite...), Steam's collections: games on this PC open as
 * usual, the rest are dimmed and open a page of their own in the Library.
 * The shelf picked is remembered, and the search while the client is open.
 */
export function Sidebar({
  games,
  downloads,
  running,
  selectedId,
  homeActive,
  onHome,
  onSelect,
  onPlay,
  onContext,
  onDownloads,
  saved,
  selectedSlug,
  onCatalogGame,
}: {
  games: LibraryGame[]
  downloads: Download[]
  running: Set<string>
  selectedId: string | null
  homeActive: boolean
  onHome: () => void
  onSelect: (id: string) => void
  onPlay: (game: LibraryGame) => void
  onContext: (game: LibraryGame, x: number, y: number) => void
  onDownloads: () => void
  saved: SavedEntry[]
  /** The kryo.to game (not on this PC) whose page is open. */
  selectedSlug: string | null
  onCatalogGame: (slug: string) => void
}) {
  const [query, setQueryState] = useState(lastQuery)
  const setQuery = (q: string) => {
    lastQuery = q
    setQueryState(q)
  }
  const [picked, setShelf] = usePersisted<Shelf>('kryoto.library.shelf', 'installed', isShelf)
  const showAdult = useShowAdult()
  const q = query.trim().toLowerCase()
  const list = useMemo(
    () =>
      games
        .filter((g) => g.title.toLowerCase().includes(q))
        .sort((a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' })),
    [games, q],
  )
  const bySlug = useMemo(() => new Map(games.filter((g) => g.slug).map((g) => [g.slug!, g])), [games])
  // A remembered shelf that is empty now (or not reported yet) shows this PC's games.
  const shelf: Shelf =
    picked === 'installed' || (picked === 'saved' ? saved.length > 0 : saved.some((e) => e.status === picked)) ? picked : 'installed'
  const shelfEntries = useMemo(
    () =>
      shelf === 'installed'
        ? []
        : saved
            .filter((e) => (shelf === 'saved' || e.status === shelf) && e.title.toLowerCase().includes(q))
            .sort((a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' })),
    [saved, shelf, q],
  )
  const missingSlugs = useMemo(() => shelfEntries.filter((e) => !bySlug.has(e.slug)).map((e) => e.slug), [shelfEntries, bySlug])
  const isAdult = useAdultFlags(missingSlugs)
  // Hiding adult games on kryo.to leaves the ones saved there off this list
  // too; one installed here stays, blurred, under Installed.
  const hideAdult = useHideAdult()
  const listed = useMemo(
    () => (hideAdult ? shelfEntries.filter((e) => bySlug.has(e.slug) || !isAdult(e.slug)) : shelfEntries),
    [hideAdult, shelfEntries, bySlug, isAdult],
  )
  const count = (st: SavedStatus) => saved.filter((e) => e.status === st).length
  const shelfOptions = [
    { value: 'installed' as Shelf, label: 'On this PC', hint: String(games.length) },
    ...(saved.length ? [{ value: 'saved' as Shelf, label: 'Everything on kryo.to', hint: String(saved.length) }] : []),
    ...STATUSES.filter((st) => count(st) > 0).map((st) => ({ value: st as Shelf, label: STATUS_LABEL[st], hint: String(count(st)) })),
  ]
  const pending = downloads.filter((d) => (isActive(d) || d.status === 'paused') && d.meta.title.toLowerCase().includes(q))

  return (
    <aside aria-label="Your games" className="flex min-h-0 w-72 shrink-0 flex-col border-r border-border bg-card/40">
      <div className="grid gap-2 p-3">
        <button
          type="button"
          aria-current={homeActive ? 'page' : undefined}
          onClick={onHome}
          className={cn(
            'kryo-pill flex h-9 items-center gap-2 px-4 text-[11px] font-bold uppercase tracking-[0.2em] transition-colors',
            homeActive ? 'bg-primary text-primary-foreground' : 'border border-border text-muted-foreground hover:text-foreground',
          )}
        >
          <Home className="size-3.5" />
          Home
        </button>
        <label className="kryo-pill flex h-9 items-center gap-2 border border-border bg-background px-3 text-muted-foreground focus-within:border-foreground">
          <Search className="size-3.5 shrink-0" />
          <input
            id="library-search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            // Escape clears the search first, then leaves the field.
            onKeyDown={(e) => {
              if (e.key !== 'Escape') return
              if (query) setQuery('')
              else e.currentTarget.blur()
            }}
            placeholder="Search your games (Ctrl+F)"
            aria-label="Search your games"
            className="kryo-square min-w-0 grow bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground"
          />
        </label>
      </div>

      {shelfOptions.length > 1 ? (
        <div className="px-3 pb-1">
          <Dropdown label="Show" value={shelf} options={shelfOptions} onChange={setShelf} />
        </div>
      ) : null}

      <div className="min-h-0 grow overflow-auto px-2 pb-3">
        {shelf !== 'installed' ? (
          <>
            <Group label={shelf === 'saved' ? 'On kryo.to' : STATUS_LABEL[shelf]} count={listed.length} />
            {listed.map((e) => {
              const game = bySlug.get(e.slug)
              const adult = game ? game.nsfw : isAdult(e.slug)
              const cover = game?.cover ?? (e.cover || null)
              return (
                <button
                  key={e.slug}
                  type="button"
                  aria-current={(game ? game.id === selectedId : e.slug === selectedSlug) ? 'page' : undefined}
                  title={game ? undefined : 'Not on this PC'}
                  onClick={() => (game ? onSelect(game.id) : onCatalogGame(e.slug))}
                  onDoubleClick={() => game && onPlay(game)}
                  className={cn(
                    'kryo-pill flex h-9 w-full items-center gap-2.5 px-2 text-left text-xs transition-colors',
                    (game ? game.id === selectedId : e.slug === selectedSlug)
                      ? 'bg-secondary text-foreground'
                      : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground',
                    !game && e.slug !== selectedSlug && 'opacity-55 hover:opacity-100',
                  )}
                >
                  {cover ? (
                    <img src={artSrc(cover) ?? undefined} onError={() => reportArt(cover, 'sidebar')} alt="" className={cn('size-6 shrink-0 object-cover', adultBlur(adult, showAdult) && 'blur-[3px]')} style={{ borderRadius: 'min(var(--kryo-radius), 6px)' }} />
                  ) : (
                    <span className="size-6 shrink-0 bg-secondary" style={{ borderRadius: 'min(var(--kryo-radius), 6px)' }} />
                  )}
                  <span className="min-w-0 grow truncate">{game?.title ?? e.title}</span>
                  {!game ? <DownloadIcon className="size-3 shrink-0" aria-label="Not installed" /> : null}
                </button>
              )
            })}
            {listed.length === 0 ? <p className="px-2 py-3 text-xs text-muted-foreground">Nothing here.</p> : null}
          </>
        ) : null}
        {shelf === 'installed' ? <Group label="All" count={games.length} /> : null}
        {(shelf === 'installed' ? list : []).map((g) => (
          <button
            key={g.id}
            type="button"
            aria-current={g.id === selectedId ? 'page' : undefined}
            onClick={() => onSelect(g.id)}
            onDoubleClick={() => onPlay(g)}
            onContextMenu={(e) => {
              e.preventDefault()
              onContext(g, e.clientX, e.clientY)
            }}
            className={cn(
              'kryo-pill flex h-9 w-full items-center gap-2.5 px-2 text-left text-xs transition-colors',
              g.id === selectedId ? 'bg-secondary text-foreground' : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground',
            )}
          >
            {g.cover ? (
              <img src={artSrc(g.cover) ?? undefined} onError={() => reportArt(g.cover!, 'sidebar')} alt="" className={cn("size-6 shrink-0 object-cover", adultBlur(g.nsfw, showAdult) && "blur-[3px]")} style={{ borderRadius: 'min(var(--kryo-radius), 6px)' }} />
            ) : (
              <span className="size-6 shrink-0 bg-secondary" style={{ borderRadius: 'min(var(--kryo-radius), 6px)' }} />
            )}
            <span className="min-w-0 grow truncate">{g.title}</span>
            {running.has(g.id) ? (
              <span className="flex items-center gap-1 text-[9px] font-bold uppercase tracking-wider text-success">
                <span className="kryo-blink size-1.5 rounded-full bg-success" />
                Running
              </span>
            ) : null}
          </button>
        ))}

        {pending.length ? (
          <>
            <Group label="Downloading" count={pending.length} />
            {pending.map((d) => (
              <button
                key={d.id}
                type="button"
                onClick={onDownloads}
                className="kryo-pill grid w-full gap-1 px-2 py-2 text-left text-xs text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
              >
                <span className="truncate">{d.meta.title}</span>
                <span className="kryo-ascii-art flex justify-between text-[10px] tracking-normal">
                  {/* Drawn for the eye; a screen reader hears the figure beside it. */}
                  <span aria-hidden>{asciiTrack(progressOf(d), 18)}</span>
                  <span>{d.status === 'paused' ? 'paused' : `${Math.round(progressOf(d) * 100)}%`}</span>
                </span>
              </button>
            ))}
          </>
        ) : null}

        {games.length > 0 && list.length === 0 && pending.length === 0 ? (
          <p className="px-2 py-3 text-xs text-muted-foreground">Nothing matches &quot;{query}&quot;.</p>
        ) : null}
      </div>
    </aside>
  )
}

function Group({ label, count }: { label: string; count: number }) {
  return (
    <div className="flex items-baseline justify-between px-2 pb-1.5 pt-3">
      <span className="text-[10px] uppercase tracking-[0.25em] text-primary">{label}</span>
      <span className="text-[10px] tabular-nums text-muted-foreground">{count}</span>
    </div>
  )
}
