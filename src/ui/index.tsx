import { useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { containDialog } from './dialog-focus'
import { Check as CheckIcon, ChevronDown, Copy, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { isTauri } from '@/lib/bridge'

export { Busy } from './Busy'
export { Matrix, type MatrixState } from './Matrix'
import { closeMenu, justClosed, onPopupChange, openMenu, popupOpen } from '@/lib/popup'

/**
 * The client's primitives, in kryo.to's voice: tracked uppercase labels,
 * hairline borders on near-black cards, pill chrome, a white pill for the one
 * thing that matters on a surface. Icons are lucide, as on the site and in
 * Forge - never a typed character.
 */

/* ── Type ────────────────────────────────────────────────── */

/** The site's section heading: `text-xs uppercase tracking-[0.25em]`. */
export function Label({ children, className }: { children: ReactNode; className?: string }) {
  return <h2 className={cn('text-xs uppercase tracking-[0.25em] text-primary', className)}>{children}</h2>
}

/** The small caption under or beside a value. */
export function Caption({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span className={cn('text-[10px] uppercase tracking-wider text-muted-foreground', className)}>{children}</span>
  )
}

/* ── Buttons ─────────────────────────────────────────────── */

type ButtonProps = React.ComponentProps<'button'> & {
  variant?: 'primary' | 'outline' | 'ghost' | 'danger'
  size?: 'sm' | 'md' | 'lg'
}

/** Pill buttons. `primary` is the white pill - one per surface. */
export function Button({ variant = 'outline', size = 'md', className, ...rest }: ButtonProps) {
  return (
    <button
      type="button"
      {...rest}
      className={cn(
        'kryo-pill kryo-press inline-flex shrink-0 items-center justify-center gap-2 font-bold uppercase tracking-wider disabled:opacity-40',
        size === 'sm' && 'h-7 px-3 text-[10px]',
        size === 'md' && 'h-9 px-4 text-[11px]',
        size === 'lg' && 'h-12 px-8 text-sm tracking-[0.2em]',
        variant === 'primary' && 'bg-primary text-primary-foreground hover:opacity-90',
        variant === 'outline' &&
          'border border-border text-muted-foreground hover:border-foreground hover:text-foreground',
        variant === 'ghost' && 'text-muted-foreground hover:bg-secondary hover:text-foreground',
        variant === 'danger' &&
          'border border-destructive/50 text-destructive hover:bg-destructive hover:text-destructive-foreground',
        className,
      )}
    />
  )
}

/** A round icon button, for chrome. */
export function IconButton({
  label,
  className,
  children,
  ...rest
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      {...rest}
      className={cn(
        'kryo-pill kryo-press inline-grid size-8 shrink-0 place-items-center border border-border text-muted-foreground hover:border-foreground hover:text-foreground disabled:opacity-30',
        className,
      )}
    >
      {children}
    </button>
  )
}

/* ── Surfaces ────────────────────────────────────────────── */

export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return <section className={cn('border border-border bg-card p-5', className)}>{children}</section>
}

export function Modal({
  title,
  onClose,
  children,
  footer,
  wide = false,
  fill = false,
}: {
  title: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  wide?: boolean
  /** The body is one pane that fills the dialog (a `Panes` layout). */
  fill?: boolean
}) {
  const overlay = useRef<HTMLDivElement>(null)
  const dialog = useRef<HTMLDivElement>(null)
  const close = useRef(onClose)
  close.current = onClose
  useLayoutEffect(() => {
    if (overlay.current && dialog.current) return containDialog(overlay.current, dialog.current, () => close.current())
  }, [])
  return createPortal(
    <div
      ref={overlay}
      className="kryo-fade fixed inset-0 z-[500] grid place-items-center bg-black/70 p-6 backdrop-blur-sm"
      onPointerDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        ref={dialog}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={cn(
          'kryo-pop kryo-radius grid max-h-[calc(100vh-48px)] w-full grid-rows-[auto_1fr_auto] overflow-hidden border border-border bg-card shadow-2xl shadow-black/60',
          wide ? 'h-[min(620px,calc(100vh-48px))] max-w-4xl' : 'max-w-lg',
        )}
      >
        <header className="flex items-center justify-between gap-4 border-b border-border px-5 py-3.5">
          <Label>{title}</Label>
          <IconButton label="Close" onClick={onClose} className="size-7">
            <X className="size-3.5" />
          </IconButton>
        </header>
        <div className={cn('grid min-h-0 p-5', fill ? 'grid-rows-[minmax(0,1fr)] overflow-hidden' : 'content-start gap-4 overflow-auto')}>
          {children}
        </div>
        {footer ? (
          <footer className="flex items-center justify-end gap-2 border-t border-border bg-background/40 px-5 py-3">
            {footer}
          </footer>
        ) : null}
      </div>
    </div>, document.body,
  )
}

/** Steam's two-pane dialog body (Properties, Settings), in Kryoto's rail. */
export function Panes<T extends string>({
  tabs,
  current,
  onChange,
  children,
}: {
  tabs: readonly (readonly [T, string])[]
  current: T
  onChange: (tab: T) => void
  children: ReactNode
}) {
  return (
    <div className="-m-5 grid h-full min-h-0 grid-cols-[180px_1fr]">
      <nav aria-label="Sections" className="grid content-start gap-1 border-r border-border p-3">
        {tabs.map(([id, label]) => (
          <button
            key={id}
            type="button"
            aria-current={current === id}
            onClick={() => onChange(id)}
            className={cn(
              'kryo-pill px-3 py-2 text-left text-[11px] uppercase tracking-wider transition-colors',
              current === id
                ? 'bg-primary font-bold text-primary-foreground'
                : 'text-muted-foreground hover:bg-secondary hover:text-foreground',
            )}
          >
            {label}
          </button>
        ))}
      </nav>
      <div className="grid min-h-0 content-start gap-6 overflow-auto p-5">{children}</div>
    </div>
  )
}

export function Section({ title, children, hint }: { title: string; children: ReactNode; hint?: ReactNode }) {
  return (
    <section className="grid gap-2.5">
      <Caption className="font-bold text-foreground/80">{title}</Caption>
      {hint ? <p className="text-xs leading-relaxed text-muted-foreground">{hint}</p> : null}
      {children}
    </section>
  )
}

/* ── Menus ───────────────────────────────────────────────── */

export type MenuEntry =
  | { label: string; onSelect: () => void; hint?: string; disabled?: boolean; danger?: boolean; icon?: ReactNode; checked?: boolean }
  | { separator: true }
  | { heading: string }

export function MenuList({ items, onDone }: { items: MenuEntry[]; onDone: () => void }) {
  const ref = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => { ref.current?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus() }, [])
  return (
    <div ref={ref} onKeyDown={(e) => {
      if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return
      e.preventDefault()
      const buttons = [...ref.current!.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)')]
      const current = buttons.indexOf(document.activeElement as HTMLElement)
      const index = e.key === 'Home' ? 0 : e.key === 'End' ? buttons.length - 1 : (current + (e.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length
      buttons[index]?.focus()
    }}>
      {items.map((item, i) =>
        'separator' in item ? (
          <div key={i} className="my-1 h-px bg-border" />
        ) : 'heading' in item ? (
          <div key={i} className="px-3 pb-1 pt-2">
            <Caption>{item.heading}</Caption>
          </div>
        ) : (
          <button
            key={i}
            type="button"
            role="menuitem"
            disabled={item.disabled}
            onClick={() => {
              onDone()
              item.onSelect()
            }}
            className={cn(
              'flex w-full items-center gap-2.5 px-3 py-2 text-left text-xs transition-colors disabled:opacity-40',
              item.danger
                ? 'text-destructive hover:bg-destructive hover:text-destructive-foreground focus:bg-destructive focus:text-destructive-foreground'
                : 'text-foreground hover:bg-primary hover:text-primary-foreground focus:bg-primary focus:text-primary-foreground',
            )}
          >
            {item.icon ? <span className="grid size-3.5 place-items-center [&>svg]:size-3.5">{item.icon}</span> : null}
            <span className="grow truncate">{item.label}</span>
            {item.hint ? <span className="text-[10px] tracking-wider opacity-60">{item.hint}</span> : null}
          </button>
        ),
      )}
    </div>
  )
}

const MENU_PANEL = 'kryo-pop kryo-radius absolute z-[400] min-w-52 overflow-hidden border border-border bg-popover py-1 shadow-2xl shadow-black/60'

/** Close on a click elsewhere or Escape. */
export function useDismiss(open: boolean, ref: React.RefObject<HTMLElement | null>, close: () => void) {
  useEffect(() => {
    if (!open) return
    const onDown = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) close()
    }
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && close()
    window.addEventListener('pointerdown', onDown)
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('pointerdown', onDown)
      window.removeEventListener('keydown', onKey)
    }
  }, [open, ref, close])
}

/**
 * A trigger with a drop-down menu under it.
 *
 * With `native` (a menu id), in the app the menu opens in the menu view
 * (`lib/popup.ts`) so it can sit over the Store; `nativeOpen` does the opening
 * for anything richer than a list. In the browser preview it falls back to
 * the page menu below.
 */
export function MenuButton({
  trigger,
  items,
  className,
  align = 'left',
  label,
  panel,
  native,
  nativeOpen,
}: {
  trigger: ReactNode
  items?: MenuEntry[]
  className: string
  align?: 'left' | 'right'
  label?: string
  /** Custom panel content instead of `items`. */
  panel?: (close: () => void) => ReactNode
  native?: string
  nativeOpen?: (anchor: DOMRect) => void
}) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement | null>(null)
  const closeBrowser = () => {
    setOpen(false)
    ref.current?.querySelector<HTMLButtonElement>('[aria-haspopup="menu"]')?.focus()
  }
  useDismiss(open, ref, closeBrowser)
  const nativeOpenNow = useNativeOpen(native)
  const useNative = !!native && isTauri()
  const openNative = (el: HTMLElement) => {
    const anchor = el.getBoundingClientRect()
    if (nativeOpen) nativeOpen(anchor)
    else void openMenu(native!, anchor, items ?? [], align)
  }
  return (
    <div ref={ref} className="relative flex">
      <button
        type="button"
        className={className}
        data-menu={native}
        aria-haspopup="menu"
        aria-expanded={useNative ? nativeOpenNow : open}
        aria-label={label}
        onKeyDown={(e) => {
          if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
          e.preventDefault()
          if (useNative) openNative(e.currentTarget)
          else setOpen(true)
        }}
        onClick={(e) => {
          if (!useNative) return setOpen(!open)
          // Pointing at it already opened it (a menu bar, below): the click
          // that follows is the same intent, not "close it again".
          if (hoverOpened?.menu === native && Date.now() - hoverOpened.at < 1500) {
            hoverOpened = null
            if (popupOpen() !== native) openNative(e.currentTarget)
            return
          }
          // A second press on the trigger closes its menu.
          if (nativeOpenNow || justClosed(native!)) return closeMenu()
          openNative(e.currentTarget)
        }}
        // Like a menu bar: with one of its menus open, pointing at a sibling
        // (same id prefix, `title:...`) opens that one instead.
        onPointerEnter={(e) => {
          const cur = popupOpen()
          if (useNative && cur && cur !== native && cur.split(':')[0] === native!.split(':')[0]) {
            hoverOpened = { menu: native!, at: Date.now() }
            openNative(e.currentTarget)
          }
        }}
      >
        {trigger}
      </button>
      {open && !useNative ? (
        <div role="menu" className={cn(MENU_PANEL, 'top-[calc(100%+6px)]', align === 'right' ? 'right-0' : 'left-0')}>
          {panel ? panel(closeBrowser) : <MenuList items={items ?? []} onDone={closeBrowser} />}
        </div>
      ) : null}
    </div>
  )
}

/**
 * The menu the pointer opened by moving onto its trigger, and when. The
 * click that usually follows must not toggle it shut: that made switching
 * between title menus take two clicks.
 */
let hoverOpened: { menu: string; at: number } | null = null

/** Whether the menu view is showing this menu id right now. */
export function useNativeOpen(menu: string | undefined) {
  const [open, setOpen] = useState(() => !!menu && popupOpen() === menu)
  useEffect(() => {
    if (!menu) return
    return onPopupChange((m) => setOpen(m === menu))
  }, [menu])
  return open
}

/** A right-click menu at the pointer, kept on screen. */
export function ContextMenu({ x, y, items, onClose }: { x: number; y: number; items: MenuEntry[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement | null>(null)
  const trigger = useRef(document.activeElement instanceof HTMLElement ? document.activeElement : null)
  useLayoutEffect(() => () => { if (trigger.current?.isConnected) trigger.current.focus() }, [])
  const [pos, setPos] = useState({ x, y })
  useDismiss(true, ref, onClose)
  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const r = el.getBoundingClientRect()
    setPos({ x: Math.min(x, window.innerWidth - r.width - 8), y: Math.min(y, window.innerHeight - r.height - 8) })
  }, [x, y])
  return (
    <div ref={ref} role="menu" className={cn(MENU_PANEL, 'fixed')} style={{ left: pos.x, top: pos.y }}>
      <MenuList items={items} onDone={onClose} />
    </div>
  )
}

/* ── Controls ────────────────────────────────────────────── */

/** A checkbox drawn by the client, never the OS widget. */
export function Check({
  checked,
  onChange,
  label,
  disabled = false,
}: {
  checked: boolean
  onChange: (next: boolean) => void
  label: ReactNode
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="kryo-square flex items-center gap-2.5 text-left text-xs text-foreground disabled:opacity-40"
    >
      <span
        className={cn(
          'grid size-4 shrink-0 place-items-center border transition-colors',
          checked ? 'border-primary bg-primary text-primary-foreground' : 'border-border',
        )}
        style={{ borderRadius: 'min(var(--kryo-radius), 5px)' }}
      >
        {checked ? <CheckIcon className="size-3" strokeWidth={3} /> : null}
      </span>
      <span>{label}</span>
    </button>
  )
}

/** A choice among a few, as a pill segment - the site's toggle groups. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T
  options: { value: T; label: string }[]
  onChange: (v: T) => void
  label?: string
}) {
  return (
    <div role="radiogroup" aria-label={label ?? options.map(o => o.label).join(' or ')} className="kryo-pill inline-flex w-fit flex-wrap gap-1 border border-border p-1">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          tabIndex={value === o.value || (!options.some(option => option.value === value) && o === options[0]) ? 0 : -1}
          onKeyDown={(e) => {
            if (!['ArrowRight', 'ArrowLeft', 'ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return
            e.preventDefault()
            const current = options.indexOf(o)
            const index = e.key === 'Home' ? 0 : e.key === 'End' ? options.length - 1 : (current + (e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : -1) + options.length) % options.length
            const next = options[index]
            if (next) onChange(next.value)
            e.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="radio"]').item(index)?.focus()
          }}
          onClick={() => onChange(o.value)}
          className={cn(
            'kryo-pill h-7 px-3 text-[10px] font-bold uppercase tracking-wider transition-colors',
            value === o.value ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground',
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}

/**
 * A choice from a list, drawn by the client - never the OS `<select>`.
 * Used inside dialogs, where the Store is already out of the way.
 */
export function Dropdown<T extends string>({
  value,
  options,
  onChange,
  className,
  label,
  size = 'md',
  emphasis = false,
}: {
  value: T
  options: { value: T; label: string; hint?: string }[]
  onChange: (v: T) => void
  className?: string
  label?: string
  /** `sm`: a compact pill for a row (Settings > Controller's buttons). */
  size?: 'sm' | 'md'
  /** Drawn bold with a strong border: the value is not the default. */
  emphasis?: boolean
}) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement | null>(null)
  const id = useId()
  useLayoutEffect(() => {
    if (open) (ref.current?.querySelector<HTMLElement>('[aria-selected="true"]') ?? ref.current?.querySelector<HTMLElement>('[role="option"]'))?.focus()
  }, [open])
  const dismiss = () => {
    setOpen(false)
    ref.current?.querySelector<HTMLButtonElement>('[aria-haspopup]')?.focus()
  }
  useDismiss(open, ref, dismiss)
  const current = options.find((o) => o.value === value)
  return (
    <div ref={ref} className={cn('relative', className)}>
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={label}
        aria-controls={open ? id : undefined}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); setOpen(true) }
        }}
        onClick={() => setOpen((o) => !o)}
        className={cn(
          'kryo-pill flex w-full items-center gap-2 border bg-background text-left text-foreground transition-colors hover:border-foreground/60 aria-expanded:border-foreground',
          size === 'sm' ? 'h-7 pl-3 pr-2 text-[11px]' : 'h-9 pl-4 pr-3 text-xs',
          emphasis ? 'border-foreground font-bold' : 'border-border',
        )}
      >
        <span className="grow truncate">{current?.label ?? 'Choose'}</span>
        <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
      </button>
      {open ? (
        <div id={id} role="listbox" aria-label={label ?? current?.label ?? 'Choose'} onKeyDown={(e) => {
          if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return
          e.preventDefault()
          const items = [...e.currentTarget.querySelectorAll<HTMLButtonElement>('[role="option"]')]
          const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement)
          const index = e.key === 'Home' ? 0 : e.key === 'End' ? items.length - 1 : (currentIndex + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length
          items[index]?.focus()
        }} className={cn(MENU_PANEL, 'left-0 right-0 top-[calc(100%+4px)] max-h-64 overflow-auto')}>
          {options.map((o) => (
            <button
              key={o.value}
              type="button"
              role="option"
              aria-selected={o.value === value}
              onClick={() => {
                dismiss()
                onChange(o.value)
              }}
              className="kryo-square flex w-full items-center gap-2.5 px-3 py-2 text-left text-xs text-foreground transition-colors hover:bg-primary hover:text-primary-foreground"
            >
              <span className="grid size-3.5 place-items-center">{o.value === value ? <CheckIcon className="size-3.5" /> : null}</span>
              <span className="grow truncate">{o.label}</span>
              {o.hint ? <span className="text-[10px] opacity-60">{o.hint}</span> : null}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  )
}

export const inputCls =
  'kryo-pill h-9 w-full border border-border bg-background px-4 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-foreground'

/** One command line, selectable, with a copy button - the site's CommandBlock. */
export function CommandLine({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <div className="kryo-radius flex items-stretch overflow-hidden border border-border bg-background">
      <code className="kryo-ascii-art min-w-0 flex-1 select-text break-all px-3 py-2.5 text-[11px] leading-relaxed text-foreground">
        {text}
      </code>
      <button
        type="button"
        aria-label="Copy"
        title={copied ? 'Copied' : 'Copy'}
        className="kryo-square grid w-10 shrink-0 place-items-center border-l border-border text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        onClick={() =>
          void navigator.clipboard
            .writeText(text)
            .then(() => {
              setCopied(true)
              setTimeout(() => setCopied(false), 1400)
            })
            .catch(() => {})
        }
      >
        {copied ? <CheckIcon className="size-3.5 text-success" /> : <Copy className="size-3.5" />}
      </button>
    </div>
  )
}

/* ── ASCII ───────────────────────────────────────────────── */

const FILLED = '█'
const PARTIAL = ['', '▏', '▎', '▍', '▌', '▋', '▊', '▉']
const EMPTY = '·'

/** kryo.to's AsciiBar track: whole blocks, an eighth-block head, dotted rest. */
export function asciiTrack(fraction: number, cells: number): string {
  const f = Math.max(0, Math.min(1, fraction))
  const exact = f * cells
  const whole = Math.floor(exact)
  const head = whole >= cells ? '' : (PARTIAL[Math.floor((exact - whole) * 8)] ?? '')
  return FILLED.repeat(Math.min(whole, cells)) + head + EMPTY.repeat(Math.max(0, cells - whole - (head ? 1 : 0)))
}

function sweep(tick: number, cells: number): string {
  const width = 5
  const max = Math.max(0, cells - width)
  const step = max ? Math.abs(tick) % (max * 2) : 0
  const pos = step <= max ? step : max * 2 - step
  let out = ''
  for (let i = 0; i < cells; i++) out += i >= pos && i < pos + width ? FILLED : EMPTY
  return out
}

/** Progress as the site draws it. `null` sweeps, for work with no total. */
export function AsciiBar({
  fraction,
  cells = 28,
  className,
  showPct = true,
}: {
  fraction: number | null
  cells?: number
  className?: string
  showPct?: boolean
}) {
  const [tick, setTick] = useState(0)
  useEffect(() => {
    if (fraction !== null) return
    const t = window.setInterval(() => setTick((n) => n + 1), 110)
    return () => window.clearInterval(t)
  }, [fraction])
  return (
    <span
      role="progressbar"
      aria-valuenow={fraction === null ? undefined : Math.round(fraction * 100)}
      aria-valuemin={0}
      aria-valuemax={100}
      className={cn('kryo-ascii-art inline-flex items-baseline gap-2 whitespace-pre text-xs leading-none', className)}
    >
      <span>{fraction === null ? sweep(tick, cells) : asciiTrack(fraction, cells)}</span>
      {showPct ? (
        <span className="w-12 text-right tabular-nums text-muted-foreground">
          {fraction === null ? '···' : `${(fraction * 100).toFixed(fraction >= 0.995 ? 0 : 1)}%`}
        </span>
      ) : null}
    </span>
  )
}

const SPARK = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█']

/** A speed graph in block characters, newest on the right. */
export function AsciiSpark({ samples, width = 48, className }: { samples: number[]; width?: number; className?: string }) {
  const recent = samples.slice(-width)
  const max = Math.max(1, ...recent)
  const line = recent.map((v) => SPARK[Math.min(7, Math.round((v / max) * 7))] ?? '▁').join('')
  return (
    <span className={cn('kryo-ascii-art whitespace-pre text-sm leading-none text-foreground/80', className)} aria-hidden>
      {'·'.repeat(Math.max(0, width - recent.length))}
      {line}
    </span>
  )
}
