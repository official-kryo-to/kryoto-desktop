import { useCallback, useEffect, useMemo, useState } from 'react'
import { useOnline } from '@/lib/online'
import { Art } from '@/library/Art'
import { artSrc } from '@/lib/art'
import { ArrowUpRight, Clock, Eye, MessageSquare, RotateCw, ShieldCheck, Users } from 'lucide-react'
import { AsciiBar, Button, Caption, IconButton, Label, Matrix } from '@/ui'
import { isTauri } from '@/lib/bridge'
import { useFriends } from '@/hooks/useFriends'
import { adultBlur, useHideAdult, useShowAdult, withoutAdult } from '@/lib/adult'
import type { LibraryGame } from '@/lib/library'
import { formatPlaytime } from '@/lib/format'
import { cn } from '@/lib/utils'
import { catalogApiUrl } from '@/lib/endpoint'
import { openExternal } from '@/lib/window'
import { DISCORD_URL, REDDIT_URL, YOUTUBE_URL } from '@/lib/community'

/**
 * Community: what everyone on kryo.to is playing, looking at and talking
 * about, from kryo.to's `/api/community/stats`. Play time comes from Kryoto
 * Desktop itself - every game closed adds to it - so the numbers here are
 * the client's own community.
 */

type Card = { slug: string; title: string; cover: string | null; nsfw: boolean }
type Stats = {
  generatedAt: string
  play: {
    hoursWeek: number
    sessionsWeek: number
    playersWeek: number
    hoursAllTime: number
    longestSessionHours: number
    byHour: number[]
    byDay: { day: string; hours: number }[]
    mostPlayed: (Card & { hours: number; players: number })[]
    board: { username: string; displayName: string | null; avatarUrl: string | null; hours: number; topGame: string | null }[]
  }
  /** Being played right now. Absent from a kryo.to that predates it. */
  live?: { players: number; games: (Card & { players: number })[] }
  popular: (Card & { views: number; downloads: number })[]
  talk: {
    commentsToday: number
    commentsTotal: number
    topCommenters: { username: string | null; displayName: string | null; avatarUrl: string | null; count: number }[]
    recent: { who: string; username?: string | null; avatarUrl: string | null; gameSlug: string; gameTitle: string; body: string; createdAt: string }[]
  }
  testing: { verified: number; live: number; testers: number }
}

const steam = (id: number) => `https://cdn.cloudflare.steamstatic.com/steam/apps/${id}/library_600x900.jpg`
const PREVIEW: Stats = {
  generatedAt: new Date().toISOString(),
  play: {
    hoursWeek: 1284.5,
    sessionsWeek: 2210,
    playersWeek: 318,
    hoursAllTime: 40211,
    longestSessionHours: 11.2,
    byHour: [4, 3, 2, 1, 1, 1, 1, 2, 3, 5, 6, 8, 9, 10, 12, 14, 17, 22, 28, 34, 38, 33, 21, 9],
    byDay: Array.from({ length: 14 }, (_, i) => ({ day: `d${i}`, hours: 60 + Math.round(40 * Math.sin(i / 2) + i * 4) })),
    mostPlayed: [
      { slug: 'hades-ii', title: 'Hades II', cover: steam(1145350), nsfw: false, hours: 212.4, players: 61 },
      { slug: 'celeste', title: 'Celeste', cover: steam(504230), nsfw: false, hours: 98.1, players: 40 },
      { slug: 'terraria', title: 'Terraria', cover: steam(105600), nsfw: false, hours: 77, players: 22 },
      { slug: 'stardew-valley', title: 'Stardew Valley', cover: steam(413150), nsfw: false, hours: 64.6, players: 19 },
    ],
    board: [
      { username: 'mira', displayName: 'Mira', avatarUrl: null, hours: 41.2, topGame: 'Hades II' },
      { username: 'k0bold', displayName: null, avatarUrl: null, hours: 37.9, topGame: 'Terraria' },
      { username: 'nox', displayName: 'Nox', avatarUrl: null, hours: 30.3, topGame: 'Celeste' },
    ],
  },
  live: {
    players: 7,
    games: [
      { slug: 'goat-simulator', title: 'Goat Simulator', cover: steam(265930), nsfw: false, players: 2 },
      { slug: 'hades-ii', title: 'Hades II', cover: steam(1145350), nsfw: false, players: 2 },
      { slug: 'terraria', title: 'Terraria', cover: steam(105600), nsfw: false, players: 1 },
    ],
  },
  popular: [
    { slug: 'terraria', title: 'Terraria', cover: steam(105600), nsfw: false, views: 18211, downloads: 3120 },
    { slug: 'hades-ii', title: 'Hades II', cover: steam(1145350), nsfw: false, views: 15002, downloads: 2890 },
    { slug: 'stardew-valley', title: 'Stardew Valley', cover: steam(413150), nsfw: false, views: 9921, downloads: 1204 },
  ],
  talk: {
    commentsToday: 42,
    commentsTotal: 9120,
    topCommenters: [{ username: 'mira', displayName: 'Mira', avatarUrl: null, count: 31 }],
    recent: [
      { who: 'Nox', avatarUrl: null, gameSlug: 'celeste', gameTitle: 'Celeste', body: 'Runs perfectly on the Deck with the launch option from the page.', createdAt: new Date().toISOString() },
    ],
  },
  testing: { verified: 812, live: 1706, testers: 24 },
}

const num = (n: number) => n.toLocaleString(undefined, { maximumFractionDigits: 1 })

function ago(iso: string) {
  const s = Math.max(1, Math.round((Date.now() - new Date(iso).getTime()) / 1000))
  if (s < 60) return 'just now'
  if (s < 3600) return `${Math.round(s / 60)}m ago`
  if (s < 86400) return `${Math.round(s / 3600)}h ago`
  return `${Math.round(s / 86400)}d ago`
}

/**
 * The last statistics, kept for the session: coming back to the page shows
 * them at once and refreshes behind them, instead of a loader every visit.
 */
let lastStats: { at: number; stats: Stats } | null = null
const FRESH_MS = 60_000

function withoutHidden(stats: Stats | null, hidden: string[]): Stats | null {
  if (!stats || hidden.length === 0) return stats
  const set = new Set(hidden.map((h) => h.toLowerCase()))
  const out = (u: string | null | undefined) => Boolean(u && set.has(u.toLowerCase()))
  return {
    ...stats,
    play: { ...stats.play, board: stats.play.board.filter((b) => !out(b.username)) },
    talk: {
      ...stats.talk,
      topCommenters: stats.talk.topCommenters.filter((c) => !out(c.username)),
      recent: stats.talk.recent.filter((c) => !out(c.username)),
    },
  }
}

export function CommunityPage({ games, onGame, onProfile }: { games: LibraryGame[]; onGame: (slug: string) => void; onProfile: (username: string) => void }) {
  const [shared, setStats] = useState<Stats | null>(isTauri() ? (lastStats?.stats ?? null) : PREVIEW)
  // kryo.to's numbers are the same for everyone; people with a block either
  // way are left out here, on this side (the list comes with the friends report).
  const hiddenList = useFriends()?.hidden
  const stats = useMemo(() => withoutHidden(shared, hiddenList ?? []), [shared, hiddenList])
  const [error, setError] = useState<string | null>(null)
  const showAdult = useShowAdult()
  const hideAdult = useHideAdult()
  const online = useOnline()

  const load = useCallback(async (force = false) => {
    if (!isTauri() || !navigator.onLine) return
    if (!force && lastStats && Date.now() - lastStats.at < FRESH_MS) return
    setError(null)
    try {
      // kryo.to caches these for a minute itself; no need to go around it.
      const res = await fetch(await catalogApiUrl('/api/community/stats'))
      if (!res.ok) throw new Error(`kryo.to answered ${res.status}`)
      const next = (await res.json()) as Stats
      lastStats = { at: Date.now(), stats: next }
      setStats(next)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])
  useEffect(() => {
    void load()
    const t = window.setInterval(() => void load(true), 120_000)
    return () => window.clearInterval(t)
  }, [load, online])

  // Your own week, from this PC: the library already counts it.
  const mine = useMemo(() => {
    const total = games.reduce((n, g) => n + g.playtimeSeconds, 0)
    const top = [...games].sort((a, b) => b.playtimeSeconds - a.playtimeSeconds)[0]
    return { total, top: top && top.playtimeSeconds > 0 ? top.title : null, count: games.filter((g) => g.playtimeSeconds > 0).length }
  }, [games])

  if (!stats) {
    return (
      <div className="grid grow place-content-center justify-items-center gap-4 text-center">
        {!online ? (
          <>
            <Matrix state="connect" className="size-5 text-muted-foreground" />
            <Label>Community</Label>
            <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">
              What the community is playing comes from kryo.to, and this PC is offline. It shows up here by itself once the connection is back.
            </p>
          </>
        ) : error ? (
          <>
            <Label>Community</Label>
            <p className="max-w-sm text-xs text-muted-foreground">The statistics did not load ({error}).</p>
            <Button onClick={() => void load(true)}>
              <RotateCw className="size-3" />
              Try again
            </Button>
          </>
        ) : (
          <AsciiBar fraction={null} cells={18} showPct={false} className="text-muted-foreground" />
        )}
      </div>
    )
  }

  const p = stats.play
  const maxHour = Math.max(1, ...p.byHour)
  const maxDay = Math.max(1, ...p.byDay.map((d) => d.hours))
  const peak = p.byHour.indexOf(Math.max(...p.byHour))

  return (
    <div className="min-h-0 grow overflow-auto">
      <div className="mx-auto grid max-w-6xl gap-9 px-8 py-8">
        <header className="flex items-end justify-between gap-4">
          <div className="grid gap-2">
            <Label>Community</Label>
            <h1 className="text-3xl font-bold tracking-tight text-foreground">This week on kryo.to</h1>
            <p className="text-xs text-muted-foreground">{liveSentence(stats)}</p>
            <div className="mt-1 flex flex-wrap gap-2">
              {SOCIAL.map((l) => (
                <button
                  key={l.href}
                  type="button"
                  onClick={() => void openExternal(l.href)}
                  className="kryo-pill flex items-center gap-1.5 border border-border bg-card/60 px-3 py-1 text-[11px] text-muted-foreground backdrop-blur-md transition-colors hover:border-foreground/40 hover:text-foreground"
                >
                  {l.label}
                  <ArrowUpRight className="size-3" aria-hidden />
                </button>
              ))}
            </div>
          </div>
          <div className="flex items-center gap-3">
            <Caption>updated {ago(stats.generatedAt)}</Caption>
            <IconButton label="Refresh" onClick={() => void load(true)}>
              <RotateCw className="size-3.5" />
            </IconButton>
          </div>
        </header>

        {stats.live?.games.length ? (
          <Shelf title={`Playing right now · ${stats.live.players} in a game`}>
            {withoutAdult(stats.live.games, hideAdult).map((g) => (
              <Poster key={g.slug} game={g} blur={adultBlur(g.nsfw, showAdult)} onClick={() => onGame(g.slug)} line={`${g.players} playing now`} />
            ))}
          </Shelf>
        ) : null}

        <dl className="grid grid-cols-2 gap-3 lg:grid-cols-5">
          <Tile icon={<Clock />} label="Hours played" value={num(p.hoursWeek)} note={`${num(p.hoursAllTime)} all time`} big />
          <Tile icon={<Users />} label="Players" value={num(p.playersWeek)} note={`${num(p.sessionsWeek)} sessions`} />
          <Tile icon={<MessageSquare />} label="Comments today" value={num(stats.talk.commentsToday)} note={`${num(stats.talk.commentsTotal)} in all`} />
          <Tile icon={<Clock />} label="Longest session" value={`${num(p.longestSessionHours)}h`} note="in one sitting" />
          {/* The fifth tile: the full width under the other four when they pair up two by two. */}
          <div className="kryo-radius col-span-2 grid content-between gap-2 border border-border bg-card p-4 lg:col-span-1">
            <dt className="flex items-center gap-2 text-[10px] uppercase tracking-wider text-muted-foreground">
              <ShieldCheck className="size-3.5" />
              Verified working
            </dt>
            <dd className="m-0 grid gap-1.5">
              <span className="text-lg font-bold tabular-nums text-foreground">
                {num(stats.testing.verified)}
                <span className="text-xs font-normal text-muted-foreground"> / {num(stats.testing.live)}</span>
              </span>
              <AsciiBar fraction={stats.testing.live ? stats.testing.verified / stats.testing.live : 0} cells={14} showPct={false} className="text-[11px]" />
              {stats.testing.testers > 0 ? (
              <span className="text-[10px] text-muted-foreground">
                by {stats.testing.testers} tester{stats.testing.testers === 1 ? '' : 's'}
              </span>
            ) : null}
            </dd>
          </div>
        </dl>

        {mine.total > 0 ? (
          <p className="kryo-radius border border-border bg-card/50 px-4 py-3 text-xs text-muted-foreground">
            You: <b className="text-foreground">{formatPlaytime(mine.total)}</b> across {mine.count} game{mine.count === 1 ? '' : 's'}
            {mine.top ? (
              <>
                , most of it in <b className="text-foreground">{mine.top}</b>
              </>
            ) : null}
            .
          </p>
        ) : null}

        {p.mostPlayed.length ? (
          <Shelf title="Most played">
            {withoutAdult(p.mostPlayed, hideAdult).map((g) => (
              <Poster key={g.slug} game={g} blur={adultBlur(g.nsfw, showAdult)} onClick={() => onGame(g.slug)} line={`${num(g.hours)}h · ${g.players} player${g.players === 1 ? "" : "s"}`} />
            ))}
          </Shelf>
        ) : null}

        {stats.popular.length ? (
          <Shelf title="Game highlights">
            {withoutAdult(stats.popular, hideAdult).map((g) => (
              <Poster
                key={g.slug}
                game={g}
                blur={adultBlur(g.nsfw, showAdult)}
                onClick={() => onGame(g.slug)}
                line={
                  <span className="inline-flex items-center gap-1">
                    <Eye className="size-3" />
                    {num(g.views)}
                  </span>
                }
              />
            ))}
          </Shelf>
        ) : null}

        <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
          <section className="grid content-start gap-3">
            <Label>Play-time board</Label>
            <ol className="kryo-radius overflow-hidden border border-border">
              {p.board.length === 0 ? (
                <li className="p-4 text-xs text-muted-foreground">Nobody has played through Kryoto Desktop this week yet.</li>
              ) : (
                p.board.map((b, i) => (
                  <li key={b.username} className="border-b border-border last:border-b-0">
                    <button
                      type="button"
                      onClick={() => onProfile(b.username)}
                      className="kryo-square flex w-full items-center gap-3 px-4 py-2.5 text-left hover:bg-secondary"
                    >
                      <span className={cn('w-5 text-right text-xs font-bold tabular-nums', i < 3 ? 'text-foreground' : 'text-muted-foreground')}>{i + 1}</span>
                      {b.avatarUrl ? (
                        <img src={artSrc(b.avatarUrl) ?? undefined} alt="" className="kryo-pill size-7 object-cover" />
                      ) : (
                        <span className="kryo-pill grid size-7 place-items-center bg-secondary text-[10px] font-bold">
                          {(b.displayName || b.username).slice(0, 1).toUpperCase()}
                        </span>
                      )}
                      <span className="grid min-w-0 grow">
                        <span className="truncate text-xs text-foreground">{b.displayName || b.username}</span>
                        {b.topGame ? <span className="truncate text-[10px] text-muted-foreground">mostly {b.topGame}</span> : null}
                      </span>
                      <span className="text-xs tabular-nums text-foreground">{num(b.hours)}h</span>
                    </button>
                  </li>
                ))
              )}
            </ol>
          </section>

          <section className="grid content-start gap-3">
            <Label>When people play</Label>
            <div className="kryo-radius grid gap-4 border border-border bg-card p-4">
              <Bars values={p.byHour} max={maxHour} labels={['00', '06', '12', '18', '23']} />
              <p className="text-[11px] text-muted-foreground">
                Busiest around <b className="text-foreground">{String(peak).padStart(2, '0')}:00 UTC</b>, over the last 30 days.
              </p>
              {p.byDay.length ? (
                <>
                  <div className="h-px bg-border" />
                  <Caption>Hours a day, last two weeks</Caption>
                  <Bars values={p.byDay.map((d) => d.hours)} max={maxDay} labels={['14d ago', '7d', 'today']} />
                </>
              ) : null}
            </div>
          </section>
        </div>

        {stats.talk.recent.length ? (
          <section className="grid gap-3">
            <Label>What people are saying</Label>
            <ul className="grid gap-2 lg:grid-cols-2">
              {stats.talk.recent.map((c, i) => (
                <li key={i}>
                  <button
                    type="button"
                    onClick={() => onGame(c.gameSlug)}
                    className="kryo-radius grid w-full gap-1.5 border border-border bg-card p-3 text-left hover:border-foreground/40"
                  >
                    <span className="flex items-center gap-2 text-[10px] uppercase tracking-wider text-muted-foreground">
                      <b className="normal-case tracking-normal text-foreground">{c.who}</b>
                      on {c.gameTitle}
                      <span className="ml-auto normal-case tracking-normal">{ago(c.createdAt)}</span>
                    </span>
                    <span className="line-clamp-2 text-xs leading-relaxed text-foreground/85">{c.body}</span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ) : null}
      </div>
    </div>
  )
}

const SOCIAL = [
  { label: 'Join our Discord', href: DISCORD_URL },
  { label: 'Join our subreddit', href: REDDIT_URL },
  { label: 'Follow us on YouTube', href: YOUTUBE_URL },
]

/** Under the title: who is in a game right now, or the week when nobody is. */
function liveSentence(stats: Stats): string {
  const people = (n: number) => `${num(n)} ${n === 1 ? 'person' : 'people'}`
  const top = stats.live?.games.find((g) => !g.nsfw)
  if (!stats.live || stats.live.players === 0 || !top) {
    return stats.play.playersWeek > 0
      ? `${people(stats.play.playersWeek)} played ${num(stats.play.hoursWeek)} hours this week.`
      : 'What everyone is playing, looking at and talking about.'
  }
  const lead = top.players === 1 ? `Someone is playing ${top.title}` : `${people(top.players)} are playing ${top.title}`
  const rest = stats.live.players - top.players
  if (rest <= 0) return `${lead} right now.`
  return rest === 1 ? `${lead} right now, and 1 more person is in another game.` : `${lead} right now, and ${num(rest)} more people are in other games.`
}

function Tile({ icon, label, value, note, big = false }: { icon: React.ReactNode; label: string; value: string; note?: string; big?: boolean }) {
  return (
    <div className={cn('kryo-radius grid content-between gap-2 border border-border p-4', big ? 'bg-primary text-primary-foreground' : 'bg-card')}>
      <dt className={cn('flex items-center gap-2 text-[10px] uppercase tracking-wider [&>svg]:size-3.5', big ? 'opacity-70' : 'text-muted-foreground')}>
        {icon}
        {label}
      </dt>
      <dd className="m-0 grid">
        <span className={cn('font-bold tabular-nums', big ? 'text-3xl' : 'text-2xl text-foreground')}>{value}</span>
        {note ? <span className={cn('text-[10px]', big ? 'opacity-70' : 'text-muted-foreground')}>{note}</span> : null}
      </dd>
    </div>
  )
}

/** Two, four or eight across: the API sends up to eight, so a full shelf ends on a full row. */
function Shelf({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="grid gap-3">
      <Label>{title}</Label>
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-8">{children}</div>
    </section>
  )
}

function Poster({ game, blur, onClick, line }: { game: Card; blur: string; onClick: () => void; line: React.ReactNode }) {
  return (
    <button type="button" onClick={onClick} className="kryo-square group grid gap-2 text-left">
      <span className="kryo-radius block aspect-[2/3] overflow-hidden border border-border bg-card">
        <Art src={game.cover} title={game.title} where="community" className={cn('size-full object-cover transition-transform duration-300 group-hover:scale-[1.03]', blur)} />
      </span>
      <span className="grid">
        <span className="truncate text-xs font-bold text-foreground">{game.title}</span>
        <span className="text-[10px] text-muted-foreground">{line}</span>
      </span>
    </button>
  )
}

/** A bar chart in whole steps, so it reads as blocks rather than as a graph. */
function Bars({ values, max, labels }: { values: number[]; max: number; labels: string[] }) {
  const STEPS = 8
  return (
    <div className="grid gap-1.5">
      <div className="flex h-24 items-end gap-[3px]" aria-hidden>
        {values.map((v, i) => {
          const steps = v > 0 ? Math.max(1, Math.round((v / max) * STEPS)) : 0
          return (
            <span key={i} className="flex h-full flex-1 flex-col justify-end gap-[2px]" title={`${num(v)}h`}>
              {Array.from({ length: steps }).map((_, j) => (
                <span key={j} className={cn('h-[calc((100%-14px)/8)] w-full bg-foreground', j === 0 ? 'opacity-100' : 'opacity-55')} />
              ))}
            </span>
          )
        })}
      </div>
      <div className="flex justify-between text-[9px] uppercase tracking-wider text-muted-foreground">
        {labels.map((l) => (
          <span key={l}>{l}</span>
        ))}
      </div>
    </div>
  )
}
