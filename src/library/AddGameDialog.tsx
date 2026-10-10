import { useEffect, useRef, useState } from 'react'
import { pickPath } from '@/lib/pick'
import { ArrowLeft, Download, FolderOpen, Search } from 'lucide-react'
import { AsciiBar, Button, Modal, Section, inputCls } from '@/ui'
import { errorText, isTauri } from '@/lib/bridge'
import { fetchCatalogGame, library, slugFrom, type CatalogGame, type LibraryGame } from '@/lib/library'
import { adultBlur, useHideAdult, useShowAdult, withoutAdult } from '@/lib/adult'
import { cn } from '@/lib/utils'
import { catalogApiUrl } from '@/lib/endpoint'

type Hit = { slug: string; title: string; developer: string | null; year: number | null; cover_vertical: string | null; cover: string | null; nsfw: boolean }

async function search(q: string): Promise<Hit[]> {
  const res = await fetch(await catalogApiUrl(`/api/games/search?q=${encodeURIComponent(q)}&limit=6`))
  if (!res.ok) throw new Error('Search is unavailable right now. Try again.')
  return ((await res.json()) as { results?: Hit[] }).results ?? []
}

/**
 * Add a game: find it on kryo.to, then either point at the copy already on
 * this PC or go and download it. A game kryo.to does not have can still be
 * added from its .exe.
 */
export function AddGameDialog({
  initialSlug,
  onAdded,
  onDownload,
  onClose,
}: {
  initialSlug: string | null
  onAdded: (game: LibraryGame) => void
  onDownload: (slug: string) => void
  onClose: () => void
}) {
  const showAdult = useShowAdult()
  const hideAdult = useHideAdult()
  const [query, setQuery] = useState('')
  const [hits, setHits] = useState<Hit[] | null>(null)
  const [searchError, setSearchError] = useState(false)
  const [searching, setSearching] = useState(false)
  const [retry, setRetry] = useState(0)
  const [found, setFound] = useState<CatalogGame | null>(null)
  const [manual, setManual] = useState(false)
  const [title, setTitle] = useState('')
  const [looking, setLooking] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const input = useRef<HTMLInputElement | null>(null)

  async function pick(slug: string) {
    setLooking(true)
    setError(null)
    try {
      setFound(await fetchCatalogGame(slug))
    } catch (e) {
      setError(errorText(e))
    } finally {
      setLooking(false)
    }
  }

  useEffect(() => {
    if (initialSlug) void pick(initialSlug)
    else input.current?.focus()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Type to search; a pasted kryo.to link goes straight to the game.
  useEffect(() => {
    const q = query.trim()
    const slug = slugFrom(q)
    if (slug && /kryo\.to\/game\//.test(q)) {
      void pick(slug)
      return
    }
    setSearchError(false)
    setHits(null)
    if (q.length < 2) { setSearching(false); return }
    setSearching(true)
    let cancelled = false
    const t = window.setTimeout(() => {
      void search(q)
        .then((h) => !cancelled && setHits(withoutAdult(h, hideAdult)))
        .catch(() => !cancelled && setSearchError(true))
        .finally(() => !cancelled && setSearching(false))
    }, 180)
    return () => {
      cancelled = true
      window.clearTimeout(t)
    }
  }, [query, retry])

  async function chooseExe() {
    setError(null)
    let picked: string | null
    if (isTauri()) {
      picked = await pickPath({
        title: found?.executable ? `Find ${found.executable}` : "Choose the game's .exe",
        extensions: ['exe', 'bat'],
      })
    } else {
      picked = `C:\\Games\\${title || found?.title || 'Game'}\\${found?.executable || 'Game.exe'}`
    }
    if (!picked) return
    setBusy(true)
    try {
      onAdded(
        await library.add(picked, {
          title: found?.title ?? title.trim(),
          slug: found?.slug ?? null,
          cover: found?.cover ?? null,
          hero: found?.hero ?? null,
          executable: found?.executable ?? '',
          defaultArgs: found?.defaultArgs ?? '',
          entries: found?.entries ?? [],
          source: found?.source ?? null,
          version: found?.version ?? null,
          short: found?.short ?? null,
          developer: found?.developer ?? null,
          nsfw: found?.nsfw ?? false,
        }),
      )
    } catch (e) {
      setError(errorText(e))
    } finally {
      setBusy(false)
    }
  }

  const back = () => {
    setFound(null)
    setManual(false)
    setError(null)
    requestAnimationFrame(() => input.current?.focus())
  }

  return (
    <Modal title="Add a game" onClose={onClose}>
      {found ? (
        <>
          <div className="flex gap-4">
            {found.cover ? (
              <img
                src={found.cover}
                alt=""
                className={cn('w-20 shrink-0 object-cover', adultBlur(found.nsfw, showAdult))}
                style={{ aspectRatio: '2 / 3', borderRadius: 'min(var(--kryo-radius), 8px)' }}
              />
            ) : null}
            <div className="grid content-center gap-1">
              <b className="text-base text-foreground">{found.title}</b>
              {found.developer ? <span className="text-xs text-muted-foreground">{found.developer}</span> : null}
              {found.executable ? <span className="text-[11px] text-muted-foreground">Starts {found.executable}</span> : null}
            </div>
          </div>
          <div className="grid gap-2 sm:grid-cols-2">
            <Choice icon={<FolderOpen />} title="It's on this PC" body={found.executable ? `Point at ${found.executable}.` : 'Point at its .exe.'} disabled={busy} onClick={() => void chooseExe()} />
            <Choice icon={<Download />} title="Download it" body="Opens its page in the store." onClick={() => onDownload(found.slug)} />
          </div>
          <Button variant="ghost" size="sm" className="w-fit" onClick={back}>
            <ArrowLeft className="size-3" />
            Another game
          </Button>
        </>
      ) : manual ? (
        <>
          <Section title="Name">
            <input className={inputCls} value={title} autoFocus placeholder="Taken from the .exe when empty" onChange={(e) => setTitle(e.target.value)} />
          </Section>
          <div className="flex gap-2">
            <Button variant="primary" disabled={busy} onClick={() => void chooseExe()}>
              <FolderOpen className="size-3.5" />
              {busy ? 'Adding' : 'Find the .exe'}
            </Button>
            <Button variant="ghost" onClick={back}>
              Back
            </Button>
          </div>
        </>
      ) : (
        <>
          <label className="kryo-pill flex h-10 items-center gap-2 border border-border bg-background px-4 text-muted-foreground focus-within:border-foreground">
            <Search className="size-4 shrink-0" />
            <input
              ref={input}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search kryo.to"
              aria-label="Search kryo.to"
              className="kryo-square min-w-0 grow bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
            />
          </label>
          {looking ? <AsciiBar fraction={null} cells={18} showPct={false} className="text-muted-foreground" /> : null}
          {searching ? <p role="status" className="text-xs text-muted-foreground">Searching…</p> : null}
          {searchError ? <div role="alert" className="grid gap-2 text-xs text-destructive">
            <p>Search is unavailable right now. Try again.</p>
            <Button size="sm" onClick={() => setRetry(n => n + 1)}>Retry</Button>
          </div> : null}
          {hits && hits.length ? (
            <ul className="grid gap-1">
              {hits.map((h) => (
                <li key={h.slug}>
                  <button
                    type="button"
                    onClick={() => void pick(h.slug)}
                    className="kryo-radius flex w-full items-center gap-3 p-2 text-left hover:bg-secondary"
                  >
                    {h.cover_vertical || h.cover ? (
                      <img
                        src={(h.cover_vertical || h.cover)!}
                        alt=""
                        className={cn('h-12 w-8 shrink-0 object-cover', adultBlur(h.nsfw, showAdult))}
                        style={{ borderRadius: 'min(var(--kryo-radius), 4px)' }}
                      />
                    ) : (
                      <span className="h-12 w-8 shrink-0 bg-secondary" />
                    )}
                    <span className="grid min-w-0">
                      <span className="truncate text-sm text-foreground">{h.title}</span>
                      <span className="truncate text-[11px] text-muted-foreground">{[h.developer, h.year].filter(Boolean).join(' · ')}</span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          ) : hits ? (
            <p className="text-xs text-muted-foreground">Nothing on kryo.to by that name.</p>
          ) : null}
          <button
            type="button"
            onClick={() => {
              setManual(true)
              setTitle(query.trim())
            }}
            className="kryo-square w-fit text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
          >
            Not on kryo.to? Add any .exe
          </button>
        </>
      )}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
    </Modal>
  )
}

function Choice({ icon, title, body, onClick, disabled }: { icon: React.ReactNode; title: string; body: string; onClick: () => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className="kryo-radius kryo-press grid gap-1.5 border border-border p-4 text-left hover:border-foreground disabled:opacity-40 [&>svg]:size-5"
    >
      {icon}
      <b className="text-sm text-foreground">{title}</b>
      <span className="text-xs text-muted-foreground">{body}</span>
    </button>
  )
}
