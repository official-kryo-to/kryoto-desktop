import { useEffect, useState } from 'react'
import { isTauri } from '@/lib/bridge'
import { logWarn } from '@/lib/log'
import { isOnline } from '@/lib/online'

/**
 * Game art, through the client's own cache (`art.rs`): the first load keeps
 * the image on disk and every later one, online or not, comes from there.
 */
export function artSrc(url: string | null | undefined): string | null {
  const u = url?.trim()
  if (!u) return null
  // kryo.to hands out some images by path (its image proxy).
  const absolute = u.startsWith('/') ? `https://kryo.to${u}` : u
  if (!/^https?:\/\//i.test(absolute) || !isTauri()) return absolute
  const convert = (window as unknown as { __TAURI_INTERNALS__?: { convertFileSrc?: (p: string, protocol: string) => string } })
    .__TAURI_INTERNALS__?.convertFileSrc
  return convert ? convert(absolute, 'kimg') : absolute
}

const failed = new Set<string>()
const loaded = new Set<string>()

/**
 * Say once that an image did not load, with its address (not when offline:
 * that is expected). A warning on this PC, not a report: art.rs has already
 * said why, and every place that draws art has a fallback.
 */
export function reportArt(url: string, where: string) {
  if (failed.has(url)) return
  failed.add(url)
  if (isOnline()) logWarn('art', `${where}: ${url} did not load`)
}

/** The pixel size of every image that has loaded, so a banner can tell a 4K hero from a thumbnail. */
const sizes = new Map<string, { width: number; height: number }>()
/** Addresses that failed to load this session: skipped, not asked again. */
const broken = new Set<string>()

function probe(src: string): Promise<boolean> {
  if (loaded.has(src)) return Promise.resolve(true)
  if (broken.has(src)) return Promise.resolve(false)
  return new Promise((resolve) => {
    const img = new Image()
    img.decoding = 'async'
    img.onload = () => {
      loaded.add(src)
      sizes.set(src, { width: img.naturalWidth, height: img.naturalHeight })
      resolve(true)
    }
    img.onerror = () => {
      broken.add(src)
      resolve(false)
    }
    img.src = src
  })
}

export type ArtState = {
  src: string | null
  status: 'loading' | 'ready' | 'none'
  /** The picture's own size, once it is ready. */
  width?: number
  height?: number
}

function ready(src: string): ArtState {
  return { src, status: 'ready', ...sizes.get(src) }
}

/**
 * What is already known about a list, in its order: the best candidate that
 * has loaded, provided nothing before it is still unknown. A candidate that
 * has not been tried yet comes first, even when a worse one further down is
 * in the cache.
 *
 * It used to take the first CACHED candidate wherever it was in the list. The
 * library grid caches every game's portrait cover, so a game's page found the
 * cover in the cache before it ever asked for the hero, and drew a 600x900
 * cover stretched across the banner, cropped and soft.
 */
function known(list: string[]): { state: ArtState; from: number } {
  for (let i = 0; i < list.length; i++) {
    const src = artSrc(list[i])
    if (!src || broken.has(src)) continue
    if (loaded.has(src)) return { state: ready(src), from: list.length }
    return { state: { src: null, status: 'loading' }, from: i }
  }
  return { state: { src: null, status: 'none' }, from: list.length }
}

/**
 * The first of `candidates` that loads: `loading` while it looks (draw a
 * spinner, never a stand-in that is swapped out a moment later), then
 * `ready` with the image, or `none` when no candidate exists.
 */
export function useArt(candidates: (string | null | undefined)[], where: string): ArtState {
  const list = candidates.filter((c): c is string => !!c?.trim())
  const key = list.join('\n')
  const [state, setState] = useState<ArtState>(() => known(list).state)
  useEffect(() => {
    let cancelled = false
    // A new set of candidates starts over, from what is already known.
    const start = known(list)
    setState(start.state)
    if (start.from >= list.length) {
      if (start.state.status === 'none' && list[0]) reportArt(list[0], where)
      return
    }
    void (async () => {
      for (const url of list.slice(start.from)) {
        const src = artSrc(url)
        if (!src) continue
        if (await probe(src)) {
          if (!cancelled) setState(ready(src))
          return
        }
        if (cancelled) return
      }
      // Only when NONE of them loaded: a missing hero with a header that
      // drew fine is the fallback working, not a failure.
      if (list[0]) reportArt(list[0], where)
      if (!cancelled) setState({ src: null, status: 'none' })
    })()
    return () => {
      cancelled = true
    }
    // `key` stands for the list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, where])
  return state
}

/**
 * Steam keeps every library hero at two sizes: `library_hero.jpg` (1920x620)
 * and `library_hero_2x.jpg` (3840x1240). Art saved before kryo.to resolved the
 * 2x one, and the guessed app-id address, name the small one, which a wide or
 * high-DPI window stretches until it is soft. Ask for the 2x first; the small
 * one stays behind it for the games that only have that.
 */
export function withSharpHeroes(candidates: (string | null | undefined)[]): (string | null | undefined)[] {
  return candidates.flatMap((c) => {
    const big = c?.replace(/\/library_hero\.jpg(?=$|\?)/, '/library_hero_2x.jpg')
    return big && big !== c ? [big, c] : [c]
  })
}
