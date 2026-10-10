import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { PopupApp } from './popup/PopupApp'
import { GlobalTooltip } from './ui/GlobalTooltip'
import { installErrorLogging } from './lib/log'
import { installWindowDrag } from './lib/window-drag'
import { call } from './lib/bridge'
import { isTauri } from './lib/window'
import './styles.css'

/** The menu view (menus.rs) loads this same page; its label says which it is. */
function windowLabel(): string {
  try {
    const internals = (window as unknown as { __TAURI_INTERNALS__?: { metadata?: { currentWebview?: { label?: string } } } })
      .__TAURI_INTERNALS__
    return internals?.metadata?.currentWebview?.label ?? 'main'
  } catch {
    return 'main'
  }
}

const label = windowLabel()
document.documentElement.dataset.window = label
installErrorLogging(label)
installWindowDrag()
// Keep custom app menus; never expose the browser's Back/Forward/Reload menu.
document.addEventListener('contextmenu', e => e.preventDefault())

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {label === 'popup' ? (
      <PopupApp />
    ) : (
      <>
        <App />
        {/* Every `title` as Kryoto's own tooltip. */}
        <GlobalTooltip />
      </>
    )}
  </StrictMode>,
)

// The first frame is on screen: tell the Linux build this start drew, so it
// does not fall back to its compatible renderer next time (display_env.rs).
if (label === 'main' && isTauri()) {
  requestAnimationFrame(() => requestAnimationFrame(() => void call('display_rendered').catch(() => {})))
}
