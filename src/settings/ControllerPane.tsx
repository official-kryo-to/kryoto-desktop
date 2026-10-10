import { useEffect, useMemo, useState } from 'react'
import { BatteryCharging, BatteryMedium, Gamepad2, RotateCcw, Vibrate } from 'lucide-react'
import { Button, Caption, Check, Matrix, Section, Segmented } from '@/ui'
import { PadArtView, PadBadge, type Callout } from '@/controller/PadArtView'
import { PadSetup } from '@/controller/PadSetup'
import { openExternal } from '@/lib/window'
import { savePadConfig, usePadInput, usePadStore } from '@/hooks/usePads'
import { isWindowsHost } from '@/lib/library'
import {
  actsAs,
  assign,
  backOf,
  confirmOf,
  FAMILY_NAMES,
  familyOf,
  isIdentity,
  padApi,
  padLabel,
  REMAPPABLE,
  VIRTUAL_DRIVER_URL,
  type PadConfig,
  type PadInfo,
  type PadPrefs,
} from '@/lib/pad'
import { PAD_FAMILIES, type PadControl, type PadFamily } from '@/lib/pad-art'
import { cn } from '@/lib/utils'

/**
 * Settings > Controller (kryo.to feature flag `controller`): the pads that are
 * plugged in or were before, drawn as they look and lit up as you press, what
 * each button is called on it, which job it does in games, and how the pad
 * moves around Kryoto. A pad Kryoto does not know (or reads wrong) is set up
 * here, one button at a time (PadSetup).
 */

const MATCHED: Record<PadInfo['model']['matched'], string> = {
  ids: 'recognized by its USB ids',
  name: 'recognized by its name',
  maker: 'recognized by its maker',
  fallback: 'not recognized, drawn as a universal pad',
}

export function ControllerPane() {
  const { pads, config, requested } = usePadStore()
  const known = useMemo(() => {
    // Plugged in first, then the ones seen before.
    const list: { guid: string; pad: PadInfo | null; name: string }[] = pads.map((p) => ({ guid: p.guid, pad: p, name: p.name }))
    for (const [guid, prefs] of Object.entries(config?.pads ?? {}))
      if (!list.some((k) => k.guid === guid)) list.push({ guid, pad: null, name: prefs.name || 'Controller' })
    return list
  }, [pads, config])

  const [chosen, setChosen] = useState<string | null>(null)
  const [setup, setSetup] = useState(false)
  const [preview, setPreview] = useState<PadFamily>('xbox')
  const current = known.find((k) => k.guid === chosen) ?? known[0] ?? null
  // "Configure" on a pad's notification opens this page on that pad.
  useEffect(() => {
    if (!requested) return
    setChosen(requested.guid)
    // A pad whose buttons Kryoto does not know goes straight to its setup.
    if (pads.some((p) => p.guid === requested.guid && !p.mapped)) setSetup(true)
  }, [requested, pads])

  const prefs: PadPrefs = (current && config?.pads[current.guid]) || { name: current?.name ?? '', family: null, remap: {}, mapping: null, base: null, sdl: null }
  const family = current ? familyOf(current.pad, prefs) : preview
  const { pressed, axes, last, clearLast } = usePadInput(current?.pad?.id ?? null)
  const [hover, setHover] = useState<PadControl | null>(null)
  const focus = hover ?? last?.control ?? null
  useEffect(() => {
    if (!last) return
    const t = window.setTimeout(clearLast, 1500)
    return () => window.clearTimeout(t)
  }, [last, clearLast])

  // The last button pressed: its row comes into view.
  useEffect(() => {
    if (last) document.getElementById(`pad-row-${last.control}`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  }, [last])

  const setPrefs = (patch: Partial<PadPrefs>) => {
    if (!config || !current) return
    const next: PadConfig = { ...config, pads: { ...config.pads, [current.guid]: { ...prefs, ...patch } } }
    void savePadConfig(next).catch(() => {})
  }
  const setFlag = (key: 'navigate' | 'notify' | 'games' | 'haptics' | 'virtualPad', v: boolean) => config && void savePadConfig({ ...config, [key]: v }).catch(() => {})
  const windows = isWindowsHost()
  const name = (c: PadControl) => padLabel(family, c).long
  const pad = current?.pad ?? null
  const unknown = !!pad && !pad.mapped
  // Windows: is the virtual-controller driver there?
  const [driver, setDriver] = useState<boolean | null>(null)
  const checkDriver = () => void padApi.virtualDriver().then(setDriver).catch(() => setDriver(false))
  useEffect(() => {
    if (windows) checkDriver()
  }, [windows])
  const forgetSetup = () =>
    pad && void padApi.saveSetup(pad.guid, pad.name, null).then((c) => savePadConfig(c)).catch(() => {})

  return (
    <>
      <div className="grid gap-3">
        {known.length ? (
          <div role="tablist" aria-label="Controllers" className="flex flex-wrap gap-2">
            {known.map((k) => (
              <button
                key={k.guid}
                type="button"
                role="tab"
                aria-selected={k.guid === current?.guid}
                onClick={() => setChosen(k.guid)}
                className={cn(
                  'kryo-pill flex h-9 items-center gap-2 border px-3 text-[11px]',
                  k.guid === current?.guid ? 'border-foreground text-foreground' : 'border-border text-muted-foreground hover:text-foreground',
                )}
              >
                <span className={cn('size-1.5 rounded-full', k.pad ? 'bg-success' : 'bg-muted-foreground/40')} aria-hidden />
                <span className="font-bold">{k.name}</span>
                {k.pad?.battery != null ? (
                  <span className="flex items-center gap-1 text-muted-foreground">
                    {k.pad.charging ? <BatteryCharging className="size-3.5" /> : <BatteryMedium className="size-3.5" />}
                    {k.pad.battery}%
                  </span>
                ) : null}
                {!k.pad ? <span className="text-muted-foreground">Not connected</span> : null}
              </button>
            ))}
          </div>
        ) : (
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <Gamepad2 className="size-4" aria-hidden />
            Plug in a controller, or connect one over Bluetooth. Kryoto asks to set up each new one.
          </p>
        )}

        {unknown ? (
          <div className="kryo-radius flex flex-wrap items-center gap-4 border border-primary/50 bg-primary/5 p-4">
            <Matrix state="wait" className="size-5 text-primary" />
            <div className="min-w-0 grow">
              <p className="text-sm font-bold text-foreground">Kryoto does not know this controller&apos;s buttons yet</p>
              <p className="mt-0.5 text-xs text-muted-foreground">
                Set it up once: press each button when it asks. About a minute, and then Kryoto and your games read it right.
              </p>
            </div>
            <Button variant="primary" size="sm" onClick={() => setSetup(true)}>
              Set up controller
            </Button>
          </div>
        ) : null}

        <div
          className="kryo-radius relative grid justify-items-center gap-3 border border-border bg-card/40 px-6 pb-5 pt-6"
          // A soft light behind the pad, so a black controller reads on a black page.
          style={{ backgroundImage: 'radial-gradient(closest-side, color-mix(in oklab, var(--foreground) 9%, transparent), transparent)' }}
        >
          <PadArtView
            family={family}
            pressed={pressed}
            axes={axes}
            focus={focus}
            onHover={setHover}
            names={name}
            callouts={callouts(family, prefs.remap, !!current)}
            className={cn('max-w-[960px]', (!current?.pad || unknown) && 'opacity-60')}
          />
          <p className="min-h-4 text-center text-xs text-muted-foreground" aria-live="polite">
            {focus ? (
              <>
                <b className="text-foreground">{padLabel(family, focus).long}</b>
                {REMAPPABLE.includes(focus) && actsAs(prefs.remap, focus) !== focus
                  ? `, acts as ${padLabel(family, actsAs(prefs.remap, focus)).long} in games`
                  : null}
              </>
            ) : unknown ? (
              'Set it up first: until then Kryoto cannot tell its buttons apart.'
            ) : current?.pad ? (
              `${current.pad.name}, ${current.pad.custom ? 'set up here' : MATCHED[current.pad.model.matched]}. Press a button to find it.`
            ) : current ? (
              'Not connected. Its settings are kept for when it is back.'
            ) : (
              'How each kind of controller is drawn and named.'
            )}
          </p>
          {pad && !unknown ? (
            <p className="flex flex-wrap items-center justify-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
              <span>Buttons not right?</span>
              <button type="button" className="font-bold text-foreground underline-offset-2 hover:underline" onClick={() => setSetup(true)}>
                Set up this controller again
              </button>
              {pad.custom ? (
                <button type="button" className="underline-offset-2 hover:text-foreground hover:underline" onClick={forgetSetup}>
                  Use the standard setup
                </button>
              ) : null}
            </p>
          ) : null}
        </div>
      </div>
      {setup && pad ? <PadSetup pad={pad} family={family} onClose={() => setSetup(false)} /> : null}

      <Section title="Layout" hint="Which buttons this controller has printed on it. Picked from the controller itself; change it if a pad pretends to be another (many PC pads say they are Xbox ones).">
        {current ? (
          <Segmented<PadFamily | 'auto'>
            label="Layout"
            value={prefs.family ?? 'auto'}
            options={[
              { value: 'auto', label: `Auto (${FAMILY_NAMES[current.pad?.model.family ?? familyOf(null, prefs)]})` },
              ...PAD_FAMILIES.map((f) => ({ value: f, label: FAMILY_NAMES[f] })),
            ]}
            onChange={(v) => setPrefs({ family: v === 'auto' ? null : v })}
          />
        ) : (
          <Segmented<PadFamily> label="Layout" value={preview} options={PAD_FAMILIES.map((f) => ({ value: f, label: FAMILY_NAMES[f] }))} onChange={setPreview} />
        )}
        {current?.pad ? (
          <div className="flex flex-wrap gap-2">
            <Button size="sm" disabled={!current.pad.rumble} onClick={() => current.pad && void padApi.rumble(current.pad.id).catch(() => {})} title={current.pad.rumble ? undefined : 'This controller cannot rumble.'}>
              <Vibrate className="size-3.5" />
              Test rumble
            </Button>
          </div>
        ) : null}
      </Section>

      <Section
        title="Buttons"
        hint={
          windows
            ? 'What each button does in games. Give a button another job and the button that had it takes over the old one. Games get it through the virtual Xbox controller (below).'
            : 'What each button does in games. Give a button another job and the button that had it takes over the old one. SDL games, Wine and Proton all follow this.'
        }
      >
        <div className="grid gap-x-6 gap-y-1 sm:grid-cols-2">
          {REMAPPABLE.map((c) => {
            const job = actsAs(prefs.remap, c)
            const lit = pressed.has(c) || focus === c
            return (
              <div
                key={c}
                id={`pad-row-${c}`}
                onMouseEnter={() => setHover(c)}
                onMouseLeave={() => setHover(null)}
                className={cn('kryo-radius flex items-center gap-3 px-2 py-1.5 transition-colors', lit ? 'bg-secondary' : 'hover:bg-secondary/60')}
              >
                <PadBadge family={family} control={c} label={padLabel(family, c).long} />
                <span className="min-w-0 grow truncate text-xs text-foreground">{padLabel(family, c).long}</span>
                {!current ? null : (
                  <select
                    aria-label={`${padLabel(family, c).long} acts as`}
                    value={job}
                    onChange={(e) => setPrefs({ remap: assign(prefs.remap, c, e.target.value as PadControl) })}
                    className={cn(
                      'kryo-pill h-7 border bg-background px-2 text-[11px]',
                      job !== c ? 'border-foreground font-bold text-foreground' : 'border-border text-muted-foreground',
                    )}
                  >
                    {REMAPPABLE.map((j) => (
                      <option key={j} value={j}>
                        {j === c ? `${padLabel(family, j).short} (own)` : `as ${padLabel(family, j).short}`}
                      </option>
                    ))}
                  </select>
                )}
              </div>
            )
          })}
        </div>
        {current && !isIdentity(prefs.remap) ? (
          <div className="flex flex-wrap items-center gap-3">
            <Button size="sm" variant="ghost" onClick={() => setPrefs({ remap: {} })}>
              <RotateCcw className="size-3.5" />
              Reset all buttons
            </Button>
            {!windows && current.pad && !prefs.sdl ? (
              <span className="text-[11px] text-warning">Set this controller up first, so games can be given these changes.</span>
            ) : null}
          </div>
        ) : null}
      </Section>

      {config ? (
        <Section
          title="In games"
          hint={
            windows
              ? 'While a game from your library runs, a controller that is not an Xbox one plays as a virtual Xbox controller, with its setup and button changes, rumble included. Every game that supports Xbox controllers then works with it.'
              : 'Games get this controller’s setup and button changes: SDL games read them, and so do Wine and Proton.'
          }
        >
          {windows ? (
            <>
              <Check checked={config.virtualPad} onChange={(v) => setFlag('virtualPad', v)} label="Play as an Xbox controller in games" />
              {config.virtualPad ? (
                driver ? (
                  <p className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Matrix state="success" className="size-3.5 text-success" />
                    The virtual-controller driver is installed.
                  </p>
                ) : driver === false ? (
                  <div className="kryo-radius flex flex-wrap items-center gap-3 border border-border p-3">
                    <Matrix state="unavailable" className="size-3.5 text-warning" />
                    <p className="min-w-0 grow text-xs text-muted-foreground">
                      This needs ViGEmBus, a free driver for virtual controllers that DS4Windows and similar tools use too. Install it once, then press Check again.
                    </p>
                    <Button size="sm" onClick={() => void openExternal(VIRTUAL_DRIVER_URL)}>
                      Get the driver
                    </Button>
                    <Button size="sm" variant="ghost" onClick={checkDriver}>
                      Check again
                    </Button>
                  </div>
                ) : null
              ) : null}
              {pad?.xinput ? (
                <p className="text-[11px] text-muted-foreground">This is an Xbox controller: games read it as it is, so it does not need the virtual one.</p>
              ) : null}
            </>
          ) : (
            <Check checked={config.games} onChange={(v) => setFlag('games', v)} label="Give games my setup and button changes" />
          )}
        </Section>
      ) : null}

      {config ? (
        <Section title="Using a controller">
          <Check checked={config.navigate} onChange={(v) => setFlag('navigate', v)} label="Move around Kryoto with a controller" />
          {config.navigate ? (
            <Check checked={config.haptics} onChange={(v) => setFlag('haptics', v)} label="Feel it in the controller as the selection moves (a light rumble)" />
          ) : null}
          <Check checked={config.notify} onChange={(v) => setFlag('notify', v)} label="Ask to set up a controller the first time it is connected" />
          {config.navigate ? <NavLegend family={family} /> : null}
        </Section>
      ) : null}
    </>
  )
}

/** Named around the pad, in the order they sit. */
const CALLOUTS: PadControl[] = [
  'lefttrigger', 'leftshoulder', 'leftstick', 'dpup', 'back', 'guide', 'start',
  'righttrigger', 'rightshoulder', 'y', 'x', 'b', 'a', 'rightstick', 'touchpad',
]

/**
 * The names beside the drawing: what is printed on each control, and, when a
 * remap gives it another job in games, that job after an arrow.
 */
function callouts(family: PadFamily, remap: PadPrefs['remap'], remaps: boolean): Callout[] {
  const own: Partial<Record<PadControl, string>> = { leftstick: 'Left stick', rightstick: 'Right stick', dpup: 'D-pad', guide: 'Home' }
  return CALLOUTS.filter((c) => c !== 'touchpad' || family === 'playstation').map((c) => {
    const job = remaps ? actsAs(remap, c) : c
    const label = own[c] ?? padLabel(family, c).short
    return job === c ? { control: c, label } : { control: c, label: `${label} > ${padLabel(family, job).short}`, changed: true }
  })
}

/** The navigation controls, with this pad's own names on them. */
function NavLegend({ family }: { family: PadFamily }) {
  const row = (controls: PadControl[], what: string) => (
    <li className="flex items-center gap-2">
      <span className="flex gap-1">
        {controls.map((c) => (
          <PadBadge key={c} family={family} control={c} label={padLabel(family, c).long} />
        ))}
      </span>
      <span className="text-xs text-muted-foreground">{what}</span>
    </li>
  )
  return (
    <div className="grid gap-2 pt-1">
      <Caption>In Kryoto</Caption>
      <ul className="grid gap-2 sm:grid-cols-2">
        {row(['dpup', 'dpdown'], 'Move (or the left stick)')}
        {row([confirmOf(family)], 'Select')}
        {row([backOf(family)], 'Back, or close a menu')}
        {row(['leftshoulder', 'rightshoulder'], 'Previous or next page')}
        {row(['rightstick'], 'Scroll (move the right stick)')}
      </ul>
      <p className="text-[11px] text-muted-foreground">
        On this page the controller is yours to try out; hold {padLabel(family, backOf(family)).short} to leave.
      </p>
    </div>
  )
}
