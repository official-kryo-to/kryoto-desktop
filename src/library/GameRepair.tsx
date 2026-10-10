import { useMemo } from 'react'
import { call, on } from '@/lib/bridge'
import { openExternal } from '@/lib/window'
import { RepairPanel, type RepairBackend, type RepairProgress } from './RepairPanel'

export function GameRepair({ gameId, initialOnline = false }: { gameId: string; initialOnline?: boolean }) {
  const backend = useMemo<RepairBackend>(() => ({
    sources: () => call('repair_sources'),
    state: () => call('repair_state', { gameId }),
    apply: (source) => call('repair_apply', { gameId, source }),
    undo: () => call('repair_undo', { gameId }),
    support: () => call('repair_support', { gameId }),
    openSupport: async () => { await openExternal(await call<string>('repair_support_url', { gameId })) },
    listen: (handler) => on<RepairProgress>('repair-progress', (progress) => { if (progress.gameId === gameId) handler(progress) }),
  }), [gameId])
  return <RepairPanel backend={backend} initialOnline={initialOnline} />
}
