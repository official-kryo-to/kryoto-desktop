import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { errorText } from '@/lib/bridge'
import { library, type LibraryGame } from '@/lib/library'

/**
 * The Library's live state: the saved games, which are running, and the last
 * thing that went wrong. Playtime is written by Rust when a game closes, so a
 * `game-state` event reloads the list rather than guessing the new totals.
 */
export function useLibrary() {
  const [games, setGames] = useState<LibraryGame[]>([])
  const [running, setRunning] = useState<Set<string>>(new Set())
  /**
   * Pressed Play, process not there yet.
   *
   * Before the game's process exists the client finds its exe, picks the
   * Proton or Wine to run it with and writes the start of its log; on Linux,
   * with a prefix to set up, that is seconds. The Play button used to sit
   * there until the process appeared, which read as a press that did nothing,
   * so the game counts as running from the press. The process's own
   * `game-state` takes over when it comes.
   */
  const [starting, setStarting] = useState<Set<string>>(new Set())
  /** Stop was pressed before the process existed: stop it as soon as it does. */
  const stopWhenUp = useRef<Set<string>>(new Set())
  const without = (set: Set<string>, id: string) => {
    if (!set.has(id)) return set
    const next = new Set(set)
    next.delete(id)
    return next
  }
  const [loaded, setLoaded] = useState(false)
  const [error, setErrorState] = useState<string | null>(null)
  const [refreshError, setRefreshError] = useState<string | null>(null)
  const generation = useRef(0)
  /**
   * The current error is an expected outcome to tell the player, not a fault
   * to report to kryo.to (Shell logs the rest).
   */
  const [errorQuiet, setErrorQuiet] = useState(false)
  const setError = useCallback((message: string | null, quiet = false) => {
    setErrorState(message)
    setErrorQuiet(quiet)
  }, [])

  const reload = useCallback(async () => {
    const request = ++generation.current
    try {
      const [list, live] = await Promise.all([library.list(), library.running()])
      if (request !== generation.current) return
      setGames(list)
      setRunning(new Set(live))
      setRefreshError(null)
    } catch (e) {
      if (request === generation.current) setRefreshError(errorText(e))
    } finally {
      if (request === generation.current) setLoaded(true)
    }
  }, [])

  // A download that just installed adds a game from the Rust side.
  useEffect(() => {
    let stop: (() => void) | undefined
    let cancelled = false
    void library
      .onChanged(() => void reload())
      .then((fn) => {
        if (cancelled) fn()
        else stop = fn
      })
    return () => {
      cancelled = true
      stop?.()
    }
  }, [reload])

  useEffect(() => {
    void reload()
    let stop: (() => void) | undefined
    let cancelled = false
    void library
      .onState((event) => {
        setRunning((current) => {
          const next = new Set(current)
          if (event.running) next.add(event.id)
          else next.delete(event.id)
          return next
        })
        setStarting((current) => without(current, event.id))
        if (event.running && stopWhenUp.current.delete(event.id)) void library.stop(event.id).catch(() => {})
        if (!event.running) {
          void reload()
          // A game gone within seconds almost never ran - say so instead of
          // leaving a Play button that silently did nothing.
          if ((event.seconds ?? 0) < 5) {
            const [message, quiet] = quickExit(event.code)
            setError(message, quiet)
          }
        }
      })
      .then((fn) => {
        if (cancelled) fn()
        else stop = fn
      })
    return () => {
      cancelled = true
      stop?.()
    }
  }, [reload])

  const play = useCallback(async (id: string, entry: number | null, joinLobby?: string) => {
    setError(null)
    setStarting((current) => new Set(current).add(id))
    try {
      await library.launch(id, entry, joinLobby)
      // A launch that went through always reports its process; one that
      // somehow did not must not leave the button on Running for good.
      window.setTimeout(() => {
        setStarting((current) => {
          if (!current.has(id)) return current
          void reload()
          return without(current, id)
        })
      }, 20_000)
    } catch (e) {
      // It never started: back to Play, and say why.
      setStarting((current) => without(current, id))
      stopWhenUp.current.delete(id)
      setError(errorText(e))
    }
  }, [reload])

  const stopGame = useCallback(async (id: string) => {
    if (starting.has(id) && !running.has(id)) {
      stopWhenUp.current.add(id)
      return
    }
    try {
      await library.stop(id)
    } catch (e) {
      setError(errorText(e))
    }
  }, [starting, running])

  const upsert = useCallback((game: LibraryGame) => {
    setGames((list) => {
      const i = list.findIndex((g) => g.id === game.id)
      if (i < 0) return [...list, game]
      const next = list.slice()
      next[i] = game
      return next
    })
  }, [])

  const drop = useCallback((id: string) => setGames((list) => list.filter((g) => g.id !== id)), [])

  // Starting counts as running everywhere the client shows it.
  const live = useMemo(() => (starting.size ? new Set([...running, ...starting]) : running), [running, starting])

  return { games, running: live, loaded, error: error ?? refreshError, errorQuiet: error ? errorQuiet : false, setError, reload, play, stop: stopGame, upsert, drop }
}

/**
 * What to say when a game was gone within seconds, and whether that is worth
 * reporting. The client already follows a launcher that hands over to the real
 * game (handoff.rs), so reaching this means nothing from the game's folder was
 * running afterwards either.
 */
function quickExit(code: number | null | undefined): [string, boolean] {
  // 0x80131700: the .NET runtime could not start, so it is missing.
  if (code === -2146232576) {
    return [
      'It closed straight away because the .NET Framework it needs is not installed. Install .NET Framework 4.8 (and 3.5 for older games) from Microsoft, then press Play again.',
      true,
    ]
  }
  // 0xC0000135: a DLL it needs is missing, almost always a Visual C++ runtime.
  if (code === -1073741515) {
    return [
      'It closed straight away because a DLL it needs is missing. Install the Visual C++ Redistributables (2015-2022, x64 and x86) and the DirectX runtime, then press Play again.',
      true,
    ]
  }
  if (code === 0 || code == null) {
    return [
      'It started and closed again without an error. If no window opened, pick the game\'s own .exe (not a launcher) in Properties.',
      true,
    ]
  }
  return [
    `It closed again straight away (exit code ${code}). Check the launch options and the exe in Properties.`,
    false,
  ]
}

export type LibraryState = ReturnType<typeof useLibrary>
