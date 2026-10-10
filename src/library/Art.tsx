import { adultBlur, useShowAdult } from '@/lib/adult'
import { useArt, withSharpHeroes } from '@/lib/art'
import { cn } from '@/lib/utils'
import { Busy } from '@/ui'

/**
 * A game's picture: the first of `src` and `fallback` that exists, through
 * the art cache. A soft shimmer while it loads; the title on a plain card
 * only when there is no picture at all.
 */
export function Art({
  src,
  fallback = [],
  title,
  className,
  adult = false,
  where = 'library',
}: {
  src: string | null | undefined
  /** Tried in order when `src` does not load. */
  fallback?: (string | null | undefined)[]
  title: string
  className?: string
  /** An adult game: blurred unless Settings says to show it. */
  adult?: boolean
  /** Named in the log when nothing loads. */
  where?: string
}) {
  const showAdult = useShowAdult()
  const art = useArt([src, ...fallback], where)
  if (art.status === 'loading') return <span aria-hidden className={cn('kryo-shimmer block bg-secondary', className)} />
  if (!art.src) {
    return (
      <span className={cn('grid place-items-center bg-secondary p-3 text-center text-xs font-bold uppercase tracking-wider text-muted-foreground', className)}>
        {title}
      </span>
    )
  }
  return <img src={art.src} alt="" className={cn('kryo-fade', className, adultBlur(adult, showAdult))} />
}

/**
 * The wide banner across the top of a game's page, Steam's library hero with
 * the title logo on it. It never shows a stand-in and then swaps: a spinner
 * until the real banner is in, then it fades up. The logo, when there is one,
 * is the title; otherwise the title in the page's own type.
 *
 * The box keeps the hero's own shape (Steam draws them 96:31, key art in the
 * middle) and only stops growing at about half the window's height, so a wide
 * window shows the picture instead of a strip off its top. It used to be a
 * fixed 320px cut from the top edge, which on a 1600px-wide page kept a third
 * of the art. A picture too small to fill the width without going soft (a
 * game with only Steam's 460px header) is drawn at its own size over a
 * blurred wash of itself, never stretched.
 */
export function GameBanner({
  title,
  banners,
  logo,
  adult,
  caption,
  pending = false,
}: {
  title: string
  /** Best first: the hero, then whatever else is wide enough (a screenshot, the header). */
  banners: (string | null | undefined)[]
  logo: string | null | undefined
  adult: boolean
  caption?: React.ReactNode
  /** Better art is on its way: keep the spinner until it arrives. */
  pending?: boolean
}) {
  const showAdult = useShowAdult()
  const found = useArt(pending ? [] : withSharpHeroes(banners), 'banner')
  const hero = pending ? { src: null, status: 'loading' as const, width: undefined } : found
  const mark = useArt(pending ? [] : [logo], 'logo')
  const blur = adultBlur(adult, showAdult)
  // Under 1200px wide, filling a page that is usually wider means stretching it.
  const small = !!hero.width && hero.width < 1200
  return (
    <div className="relative aspect-[96/31] max-h-[min(52vh,36rem)] min-h-56 w-full overflow-hidden bg-card">
      {hero.status === 'loading' ? (
        <div className="absolute inset-0 grid place-items-center text-muted-foreground">
          <Busy className="size-5" />
        </div>
      ) : hero.src && small ? (
        <>
          <img src={hero.src} alt="" aria-hidden className={cn('kryo-fade absolute inset-0 size-full scale-110 object-cover opacity-50 blur-2xl', blur)} />
          <img src={hero.src} alt="" className={cn('kryo-fade absolute inset-y-0 right-0 h-full w-auto max-w-[70%] object-contain', blur)} />
        </>
      ) : hero.src ? (
        <img src={hero.src} alt="" className={cn('kryo-fade absolute inset-0 size-full object-cover object-center', blur)} />
      ) : (
        <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_top_left,var(--secondary),transparent_70%)]" />
      )}
      <div className="hero-fade absolute inset-0" />
      <div className="absolute inset-x-8 bottom-8 grid gap-2">
        {caption}
        {pending ? (
          <div className="h-14" />
        ) : mark.status === 'ready' && mark.src ? (
          <img src={mark.src} alt={title} className="kryo-fade max-h-36 max-w-[42%] object-contain object-left drop-shadow-[0_6px_18px_rgba(0,0,0,0.7)]" />
        ) : mark.status === 'loading' ? (
          <div className="h-14" />
        ) : (
          <h1 className="max-w-3xl text-4xl font-bold leading-tight text-foreground drop-shadow-[0_4px_20px_rgba(0,0,0,0.8)]">{title}</h1>
        )}
      </div>
    </div>
  )
}
