import { useEffect, useState } from 'react'
import { call, on } from '@/lib/bridge'
import type { LaunchEntry } from '@/lib/library'

/** Mirrors `src-tauri/src/downloads.rs`. */
export type DownloadStatus =
  | 'queued'
  | 'resolving'
  | 'downloading'
  | 'paused'
  | 'verifying'
  | 'extracting'
  | 'installed'
  | 'failed'
  | 'canceled'

export type Download = {
  id: string
  slug: string | null
  url: string
  fileName: string
  archivePath: string
  total: number | null
  received: number
  speed: number
  extracted: number
  extractTotal: number | null
  status: DownloadStatus
  error: string | null
  installDir: string | null
  gameId: string | null
  addedAt: number
  finishedAt: number | null
  /** kryo.to's SHA-256 for this file, when it lists one. */
  sha256: string | null
  verified: boolean
  /** Set when the file is one of the game's add-ons: its name. */
  addon: string | null
  /** Installed, but the unpacker reported damaged files: what to tell the player. */
  warning?: string | null
  /** From a mirror rather than our own copy. */
  mirror?: { page: string; host: string } | null
  /** A build other than the current one, picked under Versions. */
  release?: string | null
  /** Place in the queue: lower goes first. */
  queueOrder?: number
  /** The file's ETag when it started, so a resume can tell a replaced file. */
  etag?: string | null
  /** What it is waiting on right now ("Connection lost. Trying again in 8 s."). */
  notice?: string | null
  meta: {
    title: string
    cover: string | null
    hero: string | null
    logo?: string | null
    header?: string | null
    executable: string
    entries: LaunchEntry[]
    source: string | null
    version: string | null
    sizeBytes: number | null
    nsfw?: boolean
  }
}

export type Notice = { title: string; body: string; gameId: string | null }

export const downloads = {
  list: () => call<Download[]>('downloads_list'),
  pause: (id: string) => call<void>('download_pause', { id }),
  resume: (id: string) => call<void>('download_resume', { id }),
  cancel: (id: string) => call<void>('download_cancel', { id }),
  remove: (id: string) => call<void>('download_remove', { id }),
  /** Reorder the queue. `now` puts it first and steps the running one aside, like Steam. */
  move: (id: string, to: 'up' | 'down' | 'top' | 'now') => call<void>('download_move', { id, to }),
  /** Download a game from one of its mirrors. `release` names a build other than the current one. */
  mirror: (url: string, slug: string, title: string | null, release: string | null) =>
    call<void>('download_mirror', { url, slug, title, release }),
}

export const isActive = (d: Download) =>
  d.status === 'downloading' || d.status === 'verifying' || d.status === 'extracting' || d.status === 'queued' || d.status === 'resolving'

/** The one moving now: asking its mirror, fetching, checking or unpacking. */
export const isWorking = (d: Download) =>
  d.status === 'resolving' || d.status === 'downloading' || d.status === 'verifying' || d.status === 'extracting'

/** Overall progress 0..1: downloading to 85%, checking to 90%, unpacking the rest. */
export function progressOf(d: Download): number {
  if (d.status === 'installed') return 1
  const part = d.extractTotal ? Math.min(1, d.extracted / d.extractTotal) : 0
  if (d.status === 'verifying') return 0.85 + 0.05 * part
  if (d.status === 'extracting') return d.extractTotal ? 0.9 + 0.1 * part : 0.95
  return d.total ? Math.min(1, d.received / d.total) * 0.85 : 0
}

/** What the download is doing, in one word, for labels. */
export function phaseOf(d: Download): string {
  if (d.status === 'resolving') return d.mirror ? `Asking ${d.mirror.host}` : 'Starting'
  if (d.status === 'verifying') return 'Checking'
  if (d.status === 'extracting') return d.addon ? 'Applying' : 'Installing'
  return 'Downloading'
}

/** The live list, kept current by the `downloads` event. */
export function useDownloads() {
  const [list, setList] = useState<Download[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  useEffect(() => {
    let stop: (() => void) | undefined
    let cancelled = false
    let receivedEvent = false
    setLoading(true)
    setError(null)
    void downloads.list().then((l) => {
      if (!cancelled && !receivedEvent) { setList(l); setLoading(false) }
    }).catch(() => {
      if (!cancelled && !receivedEvent) { setError('Couldn’t load your downloads. Try again.'); setLoading(false) }
    })
    void on<Download[]>('downloads', (l) => {
      if (cancelled) return
      receivedEvent = true
      setList(l)
      setLoading(false)
      setError(null)
    }).then((fn) => {
      if (cancelled) fn()
      else stop = fn
    }).catch(() => {
      if (!cancelled) setError('Couldn’t watch download updates. Try again.')
    })
    return () => {
      cancelled = true
      stop?.()
    }
  }, [attempt])
  return { list, loading, error, retry: () => setAttempt(n => n + 1) }
}

export function formatBytes(n: number | null | undefined): string {
  if (!n || n <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)))
  const v = n / 1024 ** i
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`
}

export function formatEta(d: Download): string | null {
  if (d.status !== 'downloading' || !d.speed || !d.total) return null
  return formatLeft((d.total - d.received) / d.speed)
}

/** Seconds still to go, said the way the Downloads page says it. */
export function formatLeft(seconds: number): string {
  const s = Math.max(0, seconds)
  if (s < 60) return `${Math.ceil(s)}s left`
  if (s < 3600) return `${Math.ceil(s / 60)} min left`
  return `${(s / 3600).toFixed(1)} h left`
}
