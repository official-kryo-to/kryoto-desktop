import { useEffect, useState } from 'react'
import { Download, Globe, Undo2 } from 'lucide-react'
import { Button, Caption, Card, Label, Modal } from '@/ui'
import { errorText } from '@/lib/bridge'
import { fetchAddons, library, type KryoAddon, type LibraryGame } from '@/lib/library'
import { GameRepair } from './GameRepair'

const isOnline = (a: { label: string | null; source: string | null }) => /online/i.test(`${a.label ?? ''} ${a.source ?? ''}`)

/**
 * A game's add-ons: what kryo.to has for it (language packs, the Online
 * add-on), what is applied, and Kryoto Online set up on this PC. Applying one
 * opens the game's download window; it applies itself when it lands. Undo
 * deletes what the add-on wrote.
 */
export function AddonsCard({
  game,
  onGet,
  onChanged,
}: {
  game: LibraryGame
  onGet: () => void
  onChanged: (g: LibraryGame) => void
}) {
  const [available, setAvailable] = useState<KryoAddon[] | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [repairOpen, setRepairOpen] = useState(false)
  const [confirm, setConfirm] = useState<{ kind: 'addon'; file: string; label: string; count: number } | { kind: 'online' } | null>(null)

  useEffect(() => {
    setAvailable(null)
    if (!game.slug) return setAvailable([])
    let cancelled = false
    void fetchAddons(game.slug)
      .then((a) => !cancelled && setAvailable(a))
      .catch(() => !cancelled && setAvailable([]))
    return () => {
      cancelled = true
    }
  }, [game.slug])

  // Kryoto Online is offered where Steam says the game plays online and the
  // release has no way online of its own (online.rs, online_unneeded).
  // Unknown - offline, or kryo.to did not answer - counts as not offered.
  const [onlineOffered, setOnlineOffered] = useState(false)
  useEffect(() => {
    setOnlineOffered(false)
    if (!game.slug || game.online) return
    let cancelled = false
    void library
      .onlineCheck(game.id)
      .then((why) => !cancelled && setOnlineOffered(why === null))
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [game.id, game.slug, game.online])

  const applied = game.addons ?? []
  const appliedFiles = new Set(applied.map((a) => a.file.toLowerCase()))
  const isApplied = (a: KryoAddon) => a.links.some((l) => l.name && appliedFiles.has(l.name.toLowerCase()))
  // The Online add-on only where Kryoto Online is offered at all: Steam lists
  // online play and the release has no way online of its own. kryo.to says
  // so too (`offered`); either one saying no hides it.
  const wanted = (a: KryoAddon) => !isOnline(a) || (onlineOffered && a.offered !== false)
  const offers = (available ?? []).filter((a) => !isApplied(a) && wanted(a))
  const kryoOnline = offers.find(isOnline)
  const showOnline = !!game.online || onlineOffered

  const run = async (key: string, job: () => Promise<LibraryGame>) => {
    setBusy(key)
    setError(null)
    try {
      onChanged(await job())
    } catch (e) {
      setError(errorText(e))
    } finally {
      setBusy(null)
    }
  }

  if (available === null) return null
  const nothing = offers.length === 0 && applied.length === 0 && !showOnline
  if (nothing) return null

  return (
    <Card className="grid gap-4">
      <Label>Add-ons</Label>

      {offers.map((a) => (
        <Row
          key={a.id}
          title={a.label || 'Add-on'}
          sub={[a.note, a.download_size].filter(Boolean).join(' · ')}
          action={
            // kryo.to's download window, where it is under Optional extras:
            // the download has to pass the site's check there. Once it
            // lands it goes into this game by itself.
            <Button size="sm" onClick={onGet}>
              <Download className="size-3" />
              Apply add-on
            </Button>
          }
        />
      ))}

      {applied.map((a) => (
        <Row
          key={a.file}
          title={a.label}
          sub={`Applied · ${a.files.length} file${a.files.length === 1 ? '' : 's'}`}
          action={
            <Button size="sm" variant="ghost" disabled={!!busy} onClick={() => setConfirm({ kind: 'addon', file: a.file, label: a.label, count: a.files.length })}>
              <Undo2 className="size-3" />
              Undo
            </Button>
          }
        />
      ))}

      {showOnline ? (
        <Row
          title="Kryoto Online on this PC"
          sub={
            game.online
              ? `Set up · version ${game.online.version}`
              : kryoOnline
                ? 'Or use the Online add-on above - either works.'
                : 'Plays online through Steam, set up here instead of downloaded.'
          }
          action={
            game.online ? (
              <Button size="sm" variant="ghost" disabled={!!busy} onClick={() => setConfirm({ kind: 'online' })}>
                <Undo2 className="size-3" />
                Undo
              </Button>
            ) : (
              <Button size="sm" disabled={!!busy} onClick={() => setRepairOpen(true)}>
                <Globe className="size-3" />
                {busy === 'online' ? 'Setting up' : 'Set up'}
              </Button>
            )
          }
        />
      ) : null}

      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {repairOpen ? <Modal title="Repair game" onClose={() => setRepairOpen(false)}><GameRepair gameId={game.id} initialOnline /></Modal> : null}

      {confirm ? (
        <Modal
          title={confirm.kind === 'online' ? 'Remove Kryoto Online' : `Undo ${confirm.label}`}
          onClose={() => setConfirm(null)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setConfirm(null)}>
                Cancel
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  const c = confirm
                  setConfirm(null)
                  if (c.kind === 'online') void run('online', () => library.onlineUndo(game.id))
                  else void run(c.file, () => library.addonUndo(game.id, c.file))
                }}
              >
                {confirm.kind === 'online' ? 'Remove' : 'Delete its files'}
              </Button>
            </>
          }
        >
          <p className="text-xs leading-relaxed text-muted-foreground">
            {confirm.kind === 'online'
              ? "Deletes Kryoto Online's files and puts back the game's own."
              : `Deletes the ${confirm.count} file${confirm.count === 1 ? '' : 's'} it added. Files it replaced are not brought back; download the game again if it needs them.`}
          </p>
        </Modal>
      ) : null}
    </Card>
  )
}

function Row({ title, sub, action }: { title: string; sub?: string; action: React.ReactNode }) {
  return (
    <div className="kryo-radius flex items-center gap-3 border border-border p-3">
      <span className="grid min-w-0 grow gap-0.5">
        <span className="truncate text-xs font-bold text-foreground">{title}</span>
        {sub ? <Caption className="normal-case tracking-normal">{sub}</Caption> : null}
      </span>
      {action}
    </div>
  )
}
