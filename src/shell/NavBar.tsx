import { useEffect, useRef, useState, type ReactNode } from 'react'
import { ArrowLeft, ArrowRight } from 'lucide-react'
import { IconButton, MenuList, useDismiss, useNativeOpen, type MenuEntry } from '@/ui'
import { isTauri } from '@/lib/bridge'
import { closeMenu, onPopupHover, openMenu, popupOpen } from '@/lib/popup'
import { cn } from '@/lib/utils'

export type TopTab = 'store' | 'library' | 'community' | 'profile'

export type NavTabSpec = {
  id: TopTab
  label: string
  onOpen: () => void
  items: MenuEntry[]
}

/**
 * Back / forward, then STORE · LIBRARY · COMMUNITY · <NAME> as kryo.to's
 * segmented pill - the white segment is where you are. Each tab opens its
 * section on click and shows its pages on hover, after a short pause so a
 * pointer sweeping across does not flash menus. Local pages keep menus in
 * the shell; over the Store they use the native menu view. The right side
 * carries whatever the current page adds: the Store's address, the Library's
 * actions.
 */
export function NavBar({
  current,
  tabs,
  canBack,
  canForward,
  onBack,
  onForward,
  overStore = false,
  right,
}: {
  current: TopTab
  tabs: NavTabSpec[]
  canBack: boolean
  canForward: boolean
  onBack: () => void
  onForward: () => void
  /** Only the visible Store needs menus in a separate native web view. */
  overStore?: boolean
  right?: ReactNode
}) {
  return (
    <nav aria-label="Main" className="relative z-40 flex h-14 shrink-0 items-center gap-3 border-b border-border bg-background px-3">
      <div className="flex gap-1.5">
        <IconButton label="Back (Alt+Left)" disabled={!canBack} onClick={onBack}>
          <ArrowLeft className="size-4" />
        </IconButton>
        <IconButton label="Forward (Alt+Right)" disabled={!canForward} onClick={onForward}>
          <ArrowRight className="size-4" />
        </IconButton>
      </div>
      <div className="kryo-pill flex items-center gap-1 border border-border bg-card p-1">
        {tabs.map((t) => (
          <NavTab key={t.id} tab={t} current={current === t.id} native={overStore && isTauri()} />
        ))}
      </div>
      <div className="flex min-w-0 grow items-center justify-end gap-2">{right}</div>
    </nav>
  )
}

function NavTab({ tab, current, native }: { tab: NavTabSpec; current: boolean; native: boolean }) {
  const menuId = `tab:${tab.id}`
  const [domOpen, setDomOpen] = useState(false)
  const nativeOpen = useNativeOpen(native ? menuId : undefined)
  const open = native ? nativeOpen : domOpen
  const timer = useRef<number | null>(null)
  const onTab = useRef(false)
  const inPopup = useRef(false)
  const button = useRef<HTMLButtonElement | null>(null)
  const wrapper = useRef<HTMLDivElement | null>(null)
  const closeLocal = () => {
    setDomOpen(false)
    if (wrapper.current?.contains(document.activeElement)) button.current?.focus({ preventScroll: true })
  }
  useDismiss(!native && domOpen, wrapper, closeLocal)

  const clear = () => {
    if (timer.current) window.clearTimeout(timer.current)
    timer.current = null
  }
  const show = () => {
    if (!tab.items.length) return
    if (!native) return setDomOpen(true)
    const r = button.current?.getBoundingClientRect()
    if (r) void openMenu(menuId, r, tab.items)
  }
  const hideSoon = (delay: number) => {
    clear()
    timer.current = window.setTimeout(() => {
      if (onTab.current || inPopup.current) return
      if (!native) closeLocal()
      else if (popupOpen() === menuId) closeMenu()
    }, delay)
  }

  // Leaving the menu (not into the tab) closes it, like leaving the tab.
  useEffect(() => {
    if (!native) return
    return onPopupHover((inside) => {
      if (popupOpen() !== menuId) return
      inPopup.current = inside
      if (!inside) hideSoon(220)
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [native, menuId])
  useEffect(() => () => {
    clear()
    if (native && popupOpen() === menuId) closeMenu()
  }, [native, menuId])
  useEffect(() => { if (!open) inPopup.current = false }, [open])
  useEffect(() => { setDomOpen(false) }, [native])

  return (
    <div
      ref={wrapper}
      className="relative"
      onPointerEnter={() => {
        onTab.current = true
        clear()
        if (open) return
        // Moving over from another tab's open menu switches straight away.
        const other = native && popupOpen()?.startsWith('tab:')
        timer.current = window.setTimeout(show, other ? 0 : 280)
      }}
      onPointerLeave={() => {
        onTab.current = false
        // Time to cross the gap into the menu.
        hideSoon(native ? 350 : 160)
      }}
    >
      <button
        ref={button}
        type="button"
        data-menu={native ? menuId : undefined}
        aria-current={current ? 'page' : undefined}
        aria-haspopup={tab.items.length ? 'menu' : undefined}
        aria-expanded={tab.items.length ? open : undefined}
        onKeyDown={(e) => {
          if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
          e.preventDefault()
          clear()
          show()
        }}
        onClick={() => {
          clear()
          if (native) closeMenu()
          else setDomOpen(false)
          tab.onOpen()
        }}
        className={cn(
          'kryo-pill h-9 max-w-56 truncate px-4 text-xs font-bold uppercase tracking-[0.2em] transition-colors',
          current ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:bg-secondary hover:text-foreground',
        )}
      >
        {tab.label}
      </button>
      {!native && domOpen ? (
        <div
          role="menu"
          className="absolute left-0 top-full z-[400] min-w-48 pt-2"
        >
          <div className="kryo-pop kryo-radius overflow-hidden border border-border bg-popover py-1 shadow-2xl shadow-black/60">
            <MenuList items={tab.items} onDone={() => setDomOpen(false)} />
          </div>
        </div>
      ) : null}
    </div>
  )
}
