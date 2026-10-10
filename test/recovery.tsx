import { createRoot } from 'react-dom/client'
import App from '@/App'
import { useWebBackend } from '@/lib/bridge'
import type { Settings } from '@/lib/settings'
import '../src/styles.css'

const settings: Settings = {
  recoveryError: 'Synthetic damaged file', libraryDir: 'fixture', libraryFolders: [], deleteArchives: true,
  startPage: 'library', defaultCompatTool: null, minimizeOnPlay: false, notifyDownloads: true,
  palette: 'monochrome', radius: 'sharp', font: 'mono', showAdult: false, followAccount: false,
  sendReports: false, sharePlaytime: false, closeToTray: false, startWithSystem: false, pressEffect: false,
  connections: 16, speedLimitMb: 0, catalogEndpoint: '', linuxMangohud: false, linuxGamemode: false,
  linuxFsr: false, playerNameMode: 'account', playerName: '',
}
let attempts = 0
const state = { saved: null as Settings | null }
Object.assign(window, { recoveryFixture: state })
localStorage.removeItem('kryoto.guest')
useWebBackend({
  call: async command => {
    if (command === 'settings_get') return settings
    if (command === 'settings_recover') {
      attempts++
      if (attempts === 1) throw new Error('Synthetic permission refusal. Original kept.')
      state.saved = { ...settings, recoveryError: null }
      return state.saved
    }
    return null
  },
  on: (event, fn) => { if (event === 'account-state') fn(null); return () => {} },
})
createRoot(document.getElementById('root')!).render(<App />)
