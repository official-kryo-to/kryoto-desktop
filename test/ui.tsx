// Development-only harness: render the real controls and hooks with deferred replies.
import { useState } from 'react'
import { createRoot } from 'react-dom/client'
import { Modal, Dropdown, Segmented, MenuButton } from '@/ui'
import { useWebBackend } from '@/lib/bridge'
import { useDownloads, type Download } from '@/lib/downloads'
import { useLibrary } from '@/hooks/useLibrary'
import { DownloadsPage } from '@/downloads/DownloadsPage'
import { LibraryHome } from '@/library/LibraryHome'
import { AddGameDialog } from '@/library/AddGameDialog'
import type { LibraryGame } from '@/lib/library'
import '../src/styles.css'

type Pending = { command: string; resolve: (v: unknown) => void; reject: (e: unknown) => void }
const pending: Pending[] = []
const listeners = new Map<string, Set<(v: unknown) => void>>()
const game = { id: 'fixture', title: 'Fixture game', slug: null, cover: null, hero: null, installDir: 'fixture', executable: 'game.exe', defaultArgs: '', entries: [], source: null, preferredEntry: null, launchOptions: '', compatTool: null, applyOverrides: false, playtimeSeconds: 0, lastPlayed: null, addedAt: 1, version: null, short: null, developer: null, nsfw: false } satisfies LibraryGame
useWebBackend({
  call: (command) => {
    if (command === 'downloads_list' || command === 'library_list') return new Promise((resolve, reject) => pending.push({ command, resolve, reject }))
    if (command === 'game_running') return Promise.resolve([])
    if (command === 'settings_get') return Promise.resolve({ libraryFolders: [], libraryDir: 'fixture', catalogEndpoint: '', showAdult: false })
    return Promise.resolve(null)
  },
  on: (event, fn) => {
    const entries = listeners.get(event) ?? new Set()
    entries.add(fn); listeners.set(event, entries)
    return () => { entries.delete(fn) }
  },
})
const fixture = {
  pending: (command: string) => pending.filter(p => p.command === command).length,
  resolve: (command: string, value: unknown, newest = false) => {
    const at = newest ? pending.findLastIndex(p => p.command === command) : pending.findIndex(p => p.command === command)
    if (at < 0) throw new Error(`No pending ${command}`)
    pending.splice(at, 1)[0]!.resolve(value)
  },
  reject: (command: string) => {
    const at = pending.findIndex(p => p.command === command)
    if (at < 0) throw new Error(`No pending ${command}`)
    pending.splice(at, 1)[0]!.reject(new Error('synthetic failure'))
  },
  emit: (event: string, value: unknown) => { listeners.get(event)?.forEach(fn => fn(value)) },
  game,
}
Object.assign(window, { fixture })

function Harness() {
  const [dialog, setDialog] = useState(false)
  const [nested, setNested] = useState(false)
  const [search, setSearch] = useState(false)
  const [value, setValue] = useState('a')
  const dl = useDownloads()
  const lib = useLibrary()
  return <main className="min-h-screen bg-background p-4 text-foreground">
    <button id="open" onClick={() => setDialog(true)}>Open controls</button>
    <button id="background">Background</button>
    <button id="search" onClick={() => setSearch(true)}>Add fixture game</button>
    <output id="library-error">{lib.error}</output>
    <output id="library-games">{lib.games.map(g => g.title).join(',')}</output>
    <button id="reload-library" onClick={() => void lib.reload()}>Reload library</button>
    <section id="downloads"><DownloadsPage list={dl.list} loading={dl.loading} error={dl.error} onRetry={dl.retry} onOpenGame={() => {}} onStore={() => {}} onDonate={null} /></section>
    <section id="library"><LibraryHome games={[game]} running={new Set()} onOpen={() => {}} onPlay={() => {}} onContext={() => {}} /></section>
    {search ? <AddGameDialog onClose={() => setSearch(false)} onAdded={() => {}} /> : null}
    {dialog ? <Modal title="Fixture controls" onClose={() => setDialog(false)} footer={<button id="last">Last control</button>}>
      <Segmented label="Fixture choice" value={value} options={[{ value: 'a', label: 'Alpha' }, { value: 'b', label: 'Beta' }]} onChange={setValue} />
      <Dropdown label="Fixture dropdown" value={value} options={[{ value: 'a', label: 'Alpha' }, { value: 'b', label: 'Beta' }]} onChange={setValue} />
      <MenuButton label="Fixture menu" trigger="Actions" className="p-2" items={[{ label: 'First action', onSelect: () => {} }, { label: 'Disabled action', disabled: true, onSelect: () => {} }, { label: 'Last action', onSelect: () => {} }]} />
      <button id="nested" onClick={() => setNested(true)}>Nested dialog</button>
      {nested ? <Modal title="Nested fixture" onClose={() => setNested(false)}><button id="nested-last">Nested control</button></Modal> : null}
    </Modal> : null}
  </main>
}
createRoot(document.getElementById('root')!).render(<Harness />)
