import { useEffect, useRef, useState } from 'react'
import { Check, LifeBuoy, RotateCcw, X } from 'lucide-react'
import { Button, Busy, Dropdown, Segmented } from '@/ui'
import { MARK_H, MARK_PATH, MARK_W } from '@/ui/ascii/mark'
import { errorText } from '@/lib/bridge'

export type RepairSource = { id: string; label: string; online: boolean }
export type RepairProgress = { text: string; percent?: number | null; gameId?: string | null }
export type RepairReport = { game: { title: string; slug: string }; source: string; version: string; warnings: string[] }
export type RepairBackend = {
  sources: () => Promise<RepairSource[]>
  state: () => Promise<{ undo_available: boolean; interrupted: boolean; report?: RepairReport | null }>
  apply: (source: string) => Promise<RepairReport>
  undo: () => Promise<void>
  support: () => Promise<string>
  openSupport: () => Promise<void>
  listen: (handler: (progress: RepairProgress) => void) => Promise<() => void>
}

/** One panel for Desktop and the portable app; source options come from Rust. */
export function RepairPanel({ backend, portable = false, initialOnline = false }: { backend: RepairBackend; portable?: boolean; initialOnline?: boolean }) {
  const [sources, setSources] = useState<RepairSource[]>([])
  const [mode, setMode] = useState<'offline' | 'online'>(initialOnline ? 'online' : 'offline')
  const [source, setSource] = useState(initialOnline ? 'online' : 'gbe_fork')
  const [status, setStatus] = useState('Choose a source, then press the logo to repair.')
  const [busy, setBusy] = useState(false)
  const [undo, setUndo] = useState(false)
  const [result, setResult] = useState<RepairReport | null>(null)
  const [answer, setAnswer] = useState<'working' | 'broken' | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [report, setReport] = useState<string | null>(null)
  const inFlight = useRef(false)

  useEffect(() => {
    let cancelled = false
    let stop: (() => void) | undefined
    void backend.sources().then((all) => !cancelled && setSources(all)).catch((e: unknown) => !cancelled && setError(errorText(e)))
    void backend.state().then((state) => {
      if (cancelled) return
      setUndo(state.undo_available)
      if (state.interrupted) setStatus('A repair was interrupted. Undo it to restore the previous files.')
      else if (state.undo_available && state.report) {
        setResult(state.report)
        setStatus('Done. Start the game and check whether it works.')
      }
    }).catch((e: unknown) => !cancelled && setError(errorText(e)))
    void backend.listen((progress) => {
      if (!cancelled) setStatus(progress.text + (progress.percent != null ? ` ${progress.percent}%` : ''))
    }).then((fn) => cancelled ? fn() : (stop = fn)).catch((e: unknown) => !cancelled && setError(errorText(e)))
    return () => { cancelled = true; stop?.() }
  }, [backend])
  useEffect(() => {
    const applied = result && sources.find((s) => s.label === result.source)
    if (applied) { setMode(applied.online ? 'online' : 'offline'); setSource(applied.id) }
  }, [sources, result])

  const choices = sources.filter((s) => s.online === (mode === 'online'))
  function changeMode(next: 'offline' | 'online') {
    setMode(next)
    setSource(sources.find((s) => s.online === (next === 'online'))?.id ?? '')
  }

  async function run() {
    if (inFlight.current || undo || !sources.some((s) => s.id === source)) return
    inFlight.current = true; setBusy(true); setError(null); setResult(null); setAnswer(null); setReport(null)
    try {
      const fixed = await backend.apply(source)
      setResult(fixed); setUndo(true)
      setStatus('Done. Start the game and check whether it works.')
    } catch (e) {
      setError(errorText(e)); setStatus('Repair could not finish.')
      await backend.state().then((s) => setUndo(s.undo_available)).catch(() => {})
    } finally { inFlight.current = false; setBusy(false) }
  }

  async function restore() {
    if (inFlight.current) return
    inFlight.current = true; setBusy(true); setError(null)
    try { await backend.undo(); setUndo(false); setResult(null); setAnswer(null); setReport(null); setStatus('Previous files restored. You can try another source.') }
    catch (e) { setError(errorText(e)) }
    finally { inFlight.current = false; setBusy(false) }
  }

  async function contact() {
    setAnswer('broken'); setError(null)
    try { setReport(await backend.support()); setStatus('Please contact support and press Ctrl+V. Your repair report was copied.') }
    catch (e) { setError(errorText(e)); setStatus('The report could not be copied. Keep the repair log and contact support.') }
  }

  return (
    <section className="grid justify-items-center gap-3 text-center" aria-label="Repair game">
      <fieldset disabled={busy || undo} className="grid justify-items-center gap-2 disabled:opacity-60">
        <Segmented value={mode} options={[{ value: 'offline', label: 'Offline' }, { value: 'online', label: 'Online' }]} onChange={changeMode} label="Play mode" />
        <Dropdown value={source} options={choices.map((s) => ({ value: s.id, label: s.label }))} onChange={setSource} label="Repair source" className="w-56 text-left" />
      </fieldset>
      <p className="max-w-sm text-[11px] leading-relaxed text-muted-foreground">Close the game first. Changing its source can break a working game. Kryoto backs up the emulator files; use Undo if the game stops working.</p>
      {mode === 'online' ? <p className="text-[11px] text-muted-foreground">Kryoto Online needs Steam running and does not work with every game.</p> : null}
      <button type="button" onClick={() => void run()} disabled={busy || undo || !sources.length} aria-label="Start game repair" className="grid h-24 w-44 place-items-center text-primary transition-opacity hover:opacity-75 focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary disabled:cursor-default disabled:opacity-50">
        {busy ? <Busy className="size-9" /> : <svg viewBox={`0 0 ${MARK_W} ${MARK_H}`} className="w-36" aria-hidden="true"><path d={MARK_PATH} fill="currentColor" /></svg>}
      </button>
      <p className="min-h-9 max-w-sm text-xs leading-relaxed" role="status" aria-live="polite" aria-atomic="true">{status}</p>
      {result && !answer ? <div className="flex gap-2">
        <Button size="sm" onClick={() => { setAnswer('working'); setStatus(portable ? 'You may delete auto-fixer-gbef.exe.' : 'The game works. Your backup is kept for Undo.') }}><Check className="size-4" aria-hidden="true" /> Working</Button>
        <Button size="sm" variant="outline" onClick={() => void contact()}><X className="size-4" aria-hidden="true" /> Not working</Button>
      </div> : null}
      {result?.warnings.length ? <details className="max-w-sm text-left text-xs text-muted-foreground"><summary>Repair notes</summary><ul className="mt-2 grid gap-1">{result.warnings.map((warning, i) => <li key={i}>{warning}</li>)}</ul></details> : null}
      <div className="flex flex-wrap justify-center gap-2">
        {undo ? <Button size="sm" variant="ghost" disabled={busy} onClick={() => void restore()}><RotateCcw className="size-3.5" aria-hidden="true" /> Undo repair</Button> : null}
        {(answer === 'broken' || error) ? <Button size="sm" variant="ghost" disabled={busy} onClick={() => void contact()}><LifeBuoy className="size-3.5" aria-hidden="true" /> Copy support report</Button> : null}
        {report ? <Button size="sm" variant="outline" onClick={() => void backend.openSupport().catch((e: unknown) => setError(errorText(e)))}>Contact support</Button> : null}
      </div>
      {error ? <p className="max-w-sm break-words text-xs text-destructive" role="alert">{error}</p> : null}
    </section>
  )
}
