import { useCallback, useEffect, useState } from 'react'
import { ChevronDown, EyeOff, Trophy } from 'lucide-react'
import { AsciiBar, Button, Caption, Card, Label } from '@/ui'
import { call, on } from '@/lib/bridge'
import type { LibraryGame } from '@/lib/library'
import { cn } from '@/lib/utils'

/** One achievement and whether it is yours (achievements.rs, game_achievements). */
type GameAchievement = {
  name: string
  displayName: string
  description: string
  hidden: boolean
  icon: string | null
  iconGray: string | null
  percent: number | null
  unlocked: boolean
  unlockedAt: number | null
}
type GameAchievements = { appid: string | null; total: number; unlocked: number; list: GameAchievement[] }

const SHOWN = 8

/**
 * A game's achievements: how far along you are, the latest unlocks and the
 * rest, rarest last. Read from the emulator's own save, so it fills in as you
 * play, here and in a notification. Hidden ones keep their secret until
 * they are unlocked. Not drawn for a game with none.
 */
export function AchievementsCard({ game }: { game: LibraryGame }) {
  const [data, setData] = useState<GameAchievements | null>(null)
  const [all, setAll] = useState(false)

  const load = useCallback(() => {
    void call<GameAchievements>('game_achievements', { id: game.id })
      .then(setData)
      .catch(() => setData(null))
  }, [game.id])

  useEffect(() => {
    setData(null)
    setAll(false)
    load()
  }, [load])

  // A new unlock while the page is open.
  useEffect(() => {
    let stop: (() => void) | undefined
    let cancelled = false
    void on<{ gameId: string }>('achievement-unlocked', (e) => {
      if (e.gameId === game.id) load()
    }).then((fn) => (cancelled ? fn() : (stop = fn)))
    return () => {
      cancelled = true
      stop?.()
    }
  }, [game.id, load])

  if (!data || data.total === 0) return null
  const shown = all ? data.list : data.list.slice(0, SHOWN)

  return (
    <Card className="grid gap-4">
      <div className="flex items-baseline justify-between gap-3">
        <Label>Achievements</Label>
        <Caption>
          {data.unlocked} of {data.total}
        </Caption>
      </div>
      <AsciiBar fraction={data.unlocked / data.total} cells={32} />
      <ul className="grid gap-2">
        {shown.map((a) => (
          <Row key={a.name} a={a} />
        ))}
      </ul>
      {data.list.length > SHOWN ? (
        <Button size="sm" variant="outline" className="justify-self-start" onClick={() => setAll((v) => !v)}>
          <ChevronDown className={cn('size-3 transition-transform', all && 'rotate-180')} aria-hidden />
          {all ? 'Show fewer' : `Show all ${data.list.length}`}
        </Button>
      ) : null}
    </Card>
  )
}

function Row({ a }: { a: GameAchievement }) {
  const secret = a.hidden && !a.unlocked
  const picture = a.unlocked ? a.icon : (a.iconGray ?? a.icon)
  return (
    <li className={cn('grid grid-cols-[40px_1fr_auto] items-center gap-3', !a.unlocked && 'opacity-60')}>
      <span className="grid size-10 place-items-center overflow-hidden border border-border bg-background">
        {secret ? (
          <EyeOff className="size-4 text-muted-foreground" aria-hidden />
        ) : picture ? (
          <img src={picture} alt="" className={cn('size-full object-cover', !a.unlocked && !a.iconGray && 'grayscale')} />
        ) : (
          <Trophy className="size-4 text-muted-foreground" aria-hidden />
        )}
      </span>
      <span className="min-w-0">
        <span className="block truncate text-xs font-bold text-foreground">{secret ? 'Hidden achievement' : a.displayName || a.name}</span>
        <span className="block text-[11px] leading-snug text-muted-foreground">
          {secret ? 'Keep playing to find out.' : a.description}
        </span>
      </span>
      <span className="text-right text-[10px] tabular-nums text-muted-foreground">
        {a.unlocked && a.unlockedAt ? new Date(a.unlockedAt * 1000).toLocaleDateString() : a.percent != null ? `${a.percent.toFixed(1)}%` : null}
      </span>
    </li>
  )
}
