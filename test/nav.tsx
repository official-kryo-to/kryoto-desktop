// Use the real native menu decision and download hook without touching the installed app.
import { useState } from 'react'
import { createRoot } from 'react-dom/client'
import { useWebBackend } from '@/lib/bridge'
import { useDownloads, type Download } from '@/lib/downloads'
import { NavBar } from '@/shell/NavBar'
import '../src/styles.css'

Object.assign(window, { __TAURI_INTERNALS__: {} })
const listeners = new Map<string, Set<(value: unknown) => void>>()
const calls: string[] = []
let update = 0
let selected = -1
const download = { id: 'fixture', status: 'extracting', received: 0 } as Download
const emit = (event: string, value: unknown) => listeners.get(event)?.forEach(fn => fn(value))
useWebBackend({
  call: async command => { calls.push(command); return command === 'downloads_list' ? [download] : null },
  on: (event, fn) => {
    const entries = listeners.get(event) ?? new Set()
    entries.add(fn); listeners.set(event, entries)
    return () => { entries.delete(fn) }
  },
})
const fixture = {
  calls,
  tick: () => emit('downloads', [{ ...download, received: ++update }]),
  selected: () => selected,
}
Object.assign(window, { navFixture: fixture })
function Harness() {
  const dl = useDownloads()
  const [store, setStore] = useState(false)
  const progress = dl.list[0]?.received ?? 0
  return <main className="h-screen bg-background text-foreground">
    <NavBar current="library" overStore={store} canBack={false} canForward={false} onBack={() => {}} onForward={() => {}}
      tabs={['store', 'library', 'community', 'profile'].map(id => ({
        id: id as 'store' | 'library' | 'community' | 'profile', label: id,
        onOpen: () => setStore(id === 'store'),
        items: [{ label: 'First action', onSelect: () => { selected = progress } }, { label: 'Downloads', onSelect: () => { selected = progress } }],
      }))} />
    <output id="progress">{progress}</output>
    <button id="local" onClick={() => setStore(false)}>Local page</button>
  </main>
}
createRoot(document.getElementById('root')!).render(<Harness />)
