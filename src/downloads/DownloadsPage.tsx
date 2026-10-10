import { useEffect, useRef, useState } from 'react'
import { ArrowUpToLine, ChevronDown, ChevronUp, Download, FolderOpen, HeartHandshake, Pause, Play, RotateCw, X } from 'lucide-react'
import { AsciiBar, AsciiSpark, Busy, Button, Caption, IconButton, Label, Matrix, type MatrixState } from '@/ui'
import { downloads as api, formatBytes, formatEta, formatLeft, isActive, isWorking, phaseOf, progressOf, type Download as Dl } from '@/lib/downloads'
import { Art } from '@/library/Art'
import { capsulesFor, library } from '@/lib/library'
import { EmptyState } from '@/ui/EmptyState'
import { INBOX } from '@/ui/ascii/scenes'

/**
 * Downloads: the game moving now across the top with its speed drawn in
 * block characters, then what is waiting, then what finished - all in the
 * site's ASCII progress voice.
 */
export function DownloadsPage({
  list,
  onOpenGame,
  onStore,
  onDonate,
  loading,
  error,
  onRetry,
}: {
  list: Dl[]
  onOpenGame: (id: string) => void
  onStore: () => void
  /** kryo.to's donate page; absent for supporters, who are never asked. */
  onDonate: (() => void) | null
  loading: boolean
  error: string | null
  onRetry: () => void
}) {
  const current = list.find(isWorking)
  // In queue order, which the arrows change.
  const waiting = list
    .filter((d) => d !== current && (isActive(d) || d.status === 'paused' || d.status === 'failed'))
    .sort((a, b) => (a.queueOrder || Number.MAX_SAFE_INTEGER) - (b.queueOrder || Number.MAX_SAFE_INTEGER) || a.addedAt - b.addedAt)
  const done = list.filter((d) => d.status === 'installed' || d.status === 'canceled')
  const stopped = waiting.filter((d) => d.status === 'paused' || d.status === 'failed')

  if (!list.length && (loading || error)) {
    return <EmptyState art={INBOX} title={error ? 'Downloads unavailable' : 'Loading downloads'} body={error ?? 'Getting your download list.'}>
      {error ? <Button onClick={onRetry}>Retry</Button> : null}
    </EmptyState>
  }
  if (list.length === 0) {
    return (
      <EmptyState art={INBOX} title="No downloads" body="Press Download on a game in the store. It installs itself and appears in your library.">
        <Button variant="primary" onClick={onStore}>
          Browse the store
        </Button>
      </EmptyState>
    )
  }

  return (
    <div className="grid min-h-0 grow content-start gap-8 overflow-auto p-6">
      {error ? <div role="alert" className="flex items-center gap-3 text-xs text-destructive"><p>{error}</p><Button size="sm" onClick={onRetry}>Retry</Button></div> : null}
      {onDonate ? <Donate onDonate={onDonate} /> : null}
      {current ? <Current d={current} /> : null}
      {waiting.length ? (
        <section className="grid gap-3">
          <div className="flex items-center justify-between gap-3">
            <Label>Up next · {waiting.length}</Label>
            {/* Everything stopped, back in the queue in one press: after a
                restart, or a night the connection kept dropping. */}
            {stopped.length > 1 ? (
              <Button
                size="sm"
                onClick={() => {
                  for (const d of stopped) void api.resume(d.id).catch(() => {})
                }}
              >
                <Download className="size-3" />
                Resume all
              </Button>
            ) : null}
          </div>
          {waiting.map((d, i) => (
            <Row key={d.id} d={d} onOpenGame={onOpenGame} queue={{ first: i === 0, last: i === waiting.length - 1 }} />
          ))}
        </section>
      ) : null}
      {done.length ? (
        <section className="grid gap-3">
          <div className="flex items-center justify-between gap-3">
            <Label>Finished · {done.length}</Label>
            {/* One press for the whole list instead of one per row. The games
                stay installed; only the history goes. */}
            <Button
              size="sm"
              onClick={() => {
                for (const d of done) void api.remove(d.id).catch(() => {})
              }}
            >
              <X className="size-3" />
              Clear all
            </Button>
          </div>
          {done.map((d) => (
            <Row key={d.id} d={d} onOpenGame={onOpenGame} />
          ))}
        </section>
      ) : null}
    </div>
  )
}

/**
 * Every file comes from kryo.to's own filehost, and storage is what it costs:
 * said where the downloads are, so it is seen by the people using it.
 */
function Donate({ onDonate }: { onDonate: () => void }) {
  return (
    <section className="kryo-radius flex items-center gap-4 border border-primary/60 bg-primary/10 p-4">
      <HeartHandshake className="size-6 shrink-0 text-primary" aria-hidden />
      <p className="grow text-xs leading-relaxed text-muted-foreground">
        Donations help pay for Kryoto-hosted downloads.
      </p>
      <Button variant="primary" size="sm" onClick={onDonate}>
        <HeartHandshake className="size-3.5" />
        Donate
      </Button>
    </section>
  )
}

/**
 * An action that takes a moment to land (pausing has to wait for the
 * connections to stop): which one was pressed, until the download's status
 * moves or ten seconds pass. The button shows it is working instead of
 * looking like it ignored the click.
 */
function usePending(d: Dl) {
  const [pending, setPending] = useState<string | null>(null)
  useEffect(() => setPending(null), [d.status])
  useEffect(() => {
    if (!pending) return
    const t = setTimeout(() => setPending(null), 10_000)
    return () => clearTimeout(t)
  }, [pending])
  const run = (what: string, action: () => Promise<unknown>) => {
    if (pending) return
    setPending(what)
    void action().catch(() => setPending(null))
  }
  return { pending, run }
}

/**
 * A download's state as one glyph of the matrix alphabet - the same family
 * as the site's toasts and the Store's loading marks. Downloading is the one
 * determinate state: the glyph fills, bottom row first, with the real
 * progress, so a glance at the list says how far each one is.
 */
const GLYPH: Partial<Record<Dl['status'], MatrixState>> = {
  queued: 'queue',
  resolving: 'connect',
  verifying: 'scan',
  extracting: 'process',
  paused: 'idle',
  failed: 'error',
  installed: 'success',
  canceled: 'unavailable',
}

function StatusGlyph({ d, className }: { d: Dl; className?: string }) {
  const tone =
    d.status === 'failed' ? 'text-destructive' : d.status === 'installed' ? 'text-success' : 'text-foreground'
  if (d.status === 'downloading') {
    return <Matrix progress={d.total ? d.received / d.total : 0} className={`${tone} ${className ?? ''}`} />
  }
  return <Matrix state={GLYPH[d.status] ?? 'busy'} className={`${tone} ${className ?? ''}`} />
}

function Spinner() {
  return <Busy className="size-3" />
}

function Current({ d }: { d: Dl }) {
  const samples = useSpeedHistory(d)
  const { pending, run } = usePending(d)
  // Past the download: checking the hash, then unpacking.
  const extracting = d.status === 'extracting' || d.status === 'verifying'
  const unpackRate = useRate(extracting ? d.extracted : null, `${d.id}:${d.status}`)
  const resolving = d.status === 'resolving'
  const fraction = resolving ? null : extracting ? (d.extractTotal ? d.extracted / d.extractTotal : null) : d.total ? d.received / d.total : null
  return (
    <section className="kryo-radius kryo-in grid grid-cols-[260px_1fr] gap-6 border border-border bg-card p-5">
      <Art adult={d.meta.nsfw} src={capsulesFor(d.meta)[0]} fallback={capsulesFor(d.meta).slice(1)} title={d.meta.title} where="downloads" className="kryo-radius aspect-[460/215] w-full object-cover" />
      <div className="grid content-start gap-4">
        <div className="flex items-start justify-between gap-4">
          <div className="grid gap-1">
            <Caption className="flex items-center gap-2">
              <StatusGlyph d={d} className="size-3" />
              {phaseOf(d)}
              {d.addon ? ` · ${d.addon}` : ''}
              {d.mirror && d.status !== 'resolving' ? ` · from ${d.mirror.host}` : ''}
              {d.release ? ` · build ${d.release}` : ''}
            </Caption>
            <h2 className="text-xl font-bold text-foreground">{d.meta.title}</h2>
          </div>
          {!extracting ? (
            <div className="flex gap-2">
              <Button size="sm" disabled={Boolean(pending)} onClick={() => run('pause', () => api.pause(d.id))}>
                {pending === 'pause' ? <Spinner /> : <Pause className="size-3" />}
                {pending === 'pause' ? 'Pausing' : 'Pause'}
              </Button>
              <IconButton label={pending === 'cancel' ? 'Cancelling' : 'Cancel'} disabled={Boolean(pending)} onClick={() => run('cancel', () => api.cancel(d.id))}>
                {pending === 'cancel' ? <Busy className="size-3.5" /> : <X className="size-3.5" />}
              </IconButton>
            </div>
          ) : null}
        </div>
        <AsciiBar fraction={fraction} cells={44} className="text-sm" />
        {resolving ? (
          <p className="text-xs leading-relaxed text-muted-foreground">
            Getting the file's address from {d.mirror?.host ?? 'the mirror'}. Hosts with a check open their page in a window of its own:
            if one appears, pass the check there and the download carries on here.
          </p>
        ) : null}
        <dl className={resolving ? 'hidden' : 'flex flex-wrap gap-8'}>
          {extracting ? (
            <>
              <Stat
                k={d.status === 'verifying' ? 'Checked' : 'Unpacked'}
                v={`${formatBytes(d.extracted)}${d.extractTotal ? ` / ${formatBytes(d.extractTotal)}` : ''}`}
              />
              {unpackRate > 0 ? <Stat k="Speed" v={`${formatBytes(unpackRate)}/s`} /> : null}
              {unpackRate > 0 && d.extractTotal ? (
                <Stat k="Time left" v={formatLeft((d.extractTotal - d.extracted) / unpackRate)} />
              ) : null}
            </>
          ) : (
            <>
              <Stat k="Speed" v={`${formatBytes(d.speed)}/s`} />
              <Stat k="Downloaded" v={`${formatBytes(d.received)}${d.total ? ` / ${formatBytes(d.total)}` : ''}`} />
              {formatEta(d) ? <Stat k="Time left" v={formatEta(d)!} /> : null}
            </>
          )}
        </dl>
        {d.notice ? (
          <p className="flex items-center gap-2 text-xs leading-relaxed text-muted-foreground" role="status">
            <Busy className="size-3" />
            {d.notice}
          </p>
        ) : null}
        {!extracting && !resolving ? <AsciiSpark samples={samples} width={56} /> : null}
      </div>
    </section>
  )
}

function Row({
  d,
  onOpenGame,
  queue,
}: {
  d: Dl
  onOpenGame: (id: string) => void
  /** Set for rows in the queue: where it is, for the arrows. */
  queue?: { first: boolean; last: boolean }
}) {
  const pct = progressOf(d)
  const { pending, run } = usePending(d)
  const status =
    d.status === 'installed'
      ? `Installed${d.finishedAt ? ` ${new Date(d.finishedAt * 1000).toLocaleDateString()}` : ''}`
      : d.status === 'canceled'
        ? 'Cancelled'
        : d.status === 'failed'
          ? 'Failed'
          : d.status === 'paused'
            ? 'Paused'
            : d.status === 'resolving'
              ? phaseOf(d)
              : 'Queued'
  return (
    <div className="kryo-radius grid grid-cols-[150px_1fr_auto] items-center gap-4 border border-border bg-card p-3">
      <Art adult={d.meta.nsfw} src={capsulesFor(d.meta)[0]} fallback={capsulesFor(d.meta).slice(1)} title={d.meta.title} where="downloads" className="kryo-radius aspect-[460/215] w-full object-cover" />
      <div className="grid min-w-0 gap-1.5">
        <b className="truncate text-sm text-foreground">{d.meta.title}</b>
        <span className="flex items-center gap-2 text-[10px] uppercase tracking-wider text-muted-foreground">
          <StatusGlyph d={d} className="size-2.5" />
          {status}
          {d.mirror ? ` · ${d.mirror.host}` : ''}
          {d.release ? ` · build ${d.release}` : ''}
          {d.total ? ` · ${formatBytes(d.status === 'installed' ? d.total : d.received)}${d.status === 'installed' ? '' : ` of ${formatBytes(d.total)}`}` : ''}
        </span>
        {d.status === 'paused' || d.status === 'queued' ? <AsciiBar fraction={pct} cells={30} /> : null}
        {d.error ? <span className="text-xs text-destructive">{d.error}</span> : null}
        {d.warning ? <span className="text-xs leading-relaxed text-warning">{d.warning}</span> : null}
      </div>
      <div className="flex items-center gap-2">
        {queue ? (
          <div className="flex items-center gap-1">
            {!queue.first ? (
              <IconButton label="Download now" onClick={() => run('now', () => api.move(d.id, 'now'))} disabled={Boolean(pending)}>
                {pending === 'now' ? <Busy className="size-3.5" /> : <ArrowUpToLine className="size-3.5" />}
              </IconButton>
            ) : null}
            <IconButton label="Move up" disabled={queue.first} onClick={() => void api.move(d.id, 'up')}>
              <ChevronUp className="size-3.5" />
            </IconButton>
            <IconButton label="Move down" disabled={queue.last} onClick={() => void api.move(d.id, 'down')}>
              <ChevronDown className="size-3.5" />
            </IconButton>
          </div>
        ) : null}
        {d.status === 'installed' && d.installDir ? (
          <IconButton label="Show folder" onClick={() => void library.openFolder(d.installDir!)}>
            <FolderOpen className="size-3.5" />
          </IconButton>
        ) : null}
        {d.status === 'installed' && d.gameId ? (
          <Button variant="primary" size="sm" onClick={() => onOpenGame(d.gameId!)}>
            <Play className="size-3 fill-current" />
            Play
          </Button>
        ) : null}
        {d.status === 'paused' || d.status === 'failed' ? (
          <Button variant="primary" size="sm" disabled={Boolean(pending)} onClick={() => run('resume', () => api.resume(d.id))}>
            {pending === 'resume' ? <Spinner /> : d.status === 'failed' ? <RotateCw className="size-3" /> : <Download className="size-3" />}
            {d.status === 'failed' ? 'Retry' : 'Resume'}
          </Button>
        ) : null}
        {d.status === 'paused' || d.status === 'queued' || d.status === 'failed' ? (
          <Button size="sm" variant="ghost" disabled={Boolean(pending)} onClick={() => run('cancel', () => api.cancel(d.id))}>
            {pending === 'cancel' ? <Spinner /> : null}
            {pending === 'cancel' ? 'Cancelling' : 'Cancel'}
          </Button>
        ) : null}
        {d.status === 'installed' || d.status === 'canceled' ? (
          <IconButton label="Clear from list" onClick={() => void api.remove(d.id)}>
            <X className="size-3.5" />
          </IconButton>
        ) : null}
      </div>
    </div>
  )
}

function Stat({ k, v }: { k: string; v: string }) {
  return (
    <div className="grid gap-1">
      <Caption>{k}</Caption>
      <dd className="m-0 text-sm tabular-nums text-foreground">{v}</dd>
    </div>
  )
}

/**
 * Bytes a second a counter is moving at, smoothed over a few seconds. For
 * unpacking and checking, whose speed the app does not report itself. `key`
 * starts it over (a new download, or checking turning into unpacking).
 */
function useRate(value: number | null, key: string) {
  const [rate, setRate] = useState(0)
  const last = useRef<{ key: string; value: number; at: number } | null>(null)
  useEffect(() => {
    if (value === null) {
      last.current = null
      setRate(0)
      return
    }
    const now = performance.now()
    const prev = last.current
    if (!prev || prev.key !== key || value < prev.value) {
      last.current = { key, value, at: now }
      setRate(0)
      return
    }
    const dt = (now - prev.at) / 1000
    // Updates come every ~200 ms; measuring over at least a second keeps the
    // figure from jumping with each one.
    if (dt < 1) return
    const instant = (value - prev.value) / dt
    setRate((r) => (r ? r * 0.6 + instant * 0.4 : instant))
    last.current = { key, value, at: now }
  }, [value, key])
  return rate
}

/** The last minute or so of speed readings, for the graph. */
function useSpeedHistory(d: Dl) {
  const [samples, setSamples] = useState<number[]>([])
  const id = useRef(d.id)
  useEffect(() => {
    if (id.current !== d.id) {
      id.current = d.id
      setSamples([])
    }
    setSamples((s) => [...s.slice(-79), d.speed])
  }, [d.id, d.speed, d.received])
  return samples
}
