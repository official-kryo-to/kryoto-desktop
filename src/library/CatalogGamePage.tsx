import { useEffect, useRef, useState } from 'react'
import { gameContext } from './context'
import { ChevronDown, Globe, Layers } from 'lucide-react'
import { AsciiBar, Button, Caption, Card, IconButton, Label, MenuList, Modal, useDismiss } from '@/ui'
import { fetchCatalogGame, type CatalogGame } from '@/lib/library'
import { isActive, phaseOf, progressOf, type Download as DownloadItem } from '@/lib/downloads'
import { errorText } from '@/lib/bridge'
import { GameBanner } from '@/library/Art'
import { STATUSES, STATUS_LABEL, type SavedStatus } from '@/hooks/useSaved'
import { InstallButton, VersionsList, type GetOurs } from '@/library/Versions'

/**
 * A kryo.to game that is on one of your lists but not on this PC, opened in
 * the Library like any other game instead of sending you to the Store: its
 * art, what kryo.to says about it, its place on your list, and Install. The
 * Store page stays one press away for everything else it has.
 *
 * Once it is installed the Library shows its real game page in its place.
 */
export function CatalogGamePage({
  slug,
  fallback,
  download,
  savedStatus,
  onSetStatus,
  onInstall,
  onStorePage,
  onDownloads,
  onContext,
}: {
  slug: string
  /** What the list already knows, drawn while kryo.to answers. */
  fallback: { title: string; cover: string | null }
  /** Its download, when one is going. */
  download: DownloadItem | null
  savedStatus: SavedStatus | null
  onSetStatus: (status: SavedStatus | null) => void
  /** Our own copy, through the Store's sheet; `releaseId` for a build other than the current one. */
  onInstall: GetOurs
  onStorePage: () => void
  onDownloads: () => void
  onContext?: (x: number, y: number) => void
}) {
  const [game, setGame] = useState<CatalogGame | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [statusOpen, setStatusOpen] = useState(false)
  const [versions, setVersions] = useState(false)
  const [installError, setInstallError] = useState<string | null>(null)
  const statusRef = useRef<HTMLDivElement | null>(null)
  useDismiss(statusOpen, statusRef, () => setStatusOpen(false))

  useEffect(() => {
    setGame(null)
    setError(null)
    let cancelled = false
    fetchCatalogGame(slug)
      .then((g) => !cancelled && setGame(g))
      .catch((e) =>
        !cancelled &&
        setError(navigator.onLine ? errorText(e) : 'This PC is offline. Its description and art come from kryo.to and show up once the connection is back.'),
      )
    return () => {
      cancelled = true
    }
  }, [slug])

  const title = game?.title ?? fallback.title
  // Unknown counts as adult until kryo.to has said, like the rail's covers.
  const adult = game ? game.nsfw : true
  const going = download && (isActive(download) || download.status === 'paused') ? download : null

  return (
    <section aria-label={title} className="min-h-0 grow overflow-auto" {...gameContext(onContext)}>
      <GameBanner
        title={title}
        adult={adult}
        pending={!game && !error}
        banners={[game?.hero, ...(game?.screenshots.slice(0, 1) ?? []), game?.header, fallback.cover]}
        logo={game?.logo}
        caption={<Caption>Not on this PC</Caption>}
      />

      <div className="kryo-radius relative z-10 mx-6 -mt-2 flex flex-wrap items-center gap-6 border border-border bg-card/90 p-4 backdrop-blur">
        {going ? (
          <button type="button" onClick={onDownloads} className="kryo-square grid min-w-56 gap-1.5 text-left" title="Open Downloads">
            <Caption>{going.status === 'paused' ? 'Paused' : phaseOf(going)}</Caption>
            <span className="flex items-center gap-3 text-sm text-foreground">
              <AsciiBar fraction={progressOf(going)} cells={18} />
            </span>
          </button>
        ) : (
          <InstallButton
            slug={slug}
            title={title}
            onOurs={onInstall}
            onError={setInstallError}
            onVersions={() => setVersions(true)}
            disabled={!game && !error}
          />
        )}

        <div ref={statusRef} className="relative">
          <button
            type="button"
            aria-expanded={statusOpen}
            onClick={() => setStatusOpen((o) => !o)}
            className="kryo-square grid gap-1 text-left"
            title="Your list on kryo.to"
          >
            <Caption>On kryo.to</Caption>
            <span className="flex items-center gap-1 text-sm text-foreground">
              {savedStatus ? STATUS_LABEL[savedStatus] : 'Not listed'}
              <ChevronDown className="size-3 text-muted-foreground" />
            </span>
          </button>
          {statusOpen ? (
            <div className="kryo-pop kryo-radius absolute left-0 top-[calc(100%+8px)] z-50 min-w-48 overflow-hidden border border-border bg-popover py-1 shadow-2xl shadow-black/60">
              <MenuList
                onDone={() => setStatusOpen(false)}
                items={[
                  ...STATUSES.map((st) => ({
                    label: STATUS_LABEL[st],
                    checked: savedStatus === st,
                    hint: savedStatus === st ? 'now' : undefined,
                    onSelect: () => onSetStatus(st),
                  })),
                  ...(savedStatus ? [{ separator: true } as const, { label: 'Take off my list', onSelect: () => onSetStatus(null) }] : []),
                ]}
              />
            </div>
          ) : null}
        </div>

        <div className="ml-auto flex gap-2">
          <IconButton label="Builds" onClick={() => setVersions(true)}>
            <Layers className="size-4" />
          </IconButton>
          <IconButton label="Store page" onClick={onStorePage}>
            <Globe className="size-4" />
          </IconButton>
        </div>
      </div>

      {installError ? (
        <p className="kryo-radius mx-6 mt-3 border border-destructive/50 bg-destructive/10 px-3 py-2 text-xs text-destructive">{installError}</p>
      ) : null}
      {versions ? (
        <Modal title={`${title}: builds`} onClose={() => setVersions(false)}>
          <VersionsList
            slug={slug}
            title={title}
            installed={null}
            pinned={null}
            onOurs={(id) => {
              setVersions(false)
              onInstall(id)
            }}
            onError={setInstallError}
            onStarted={() => setVersions(false)}
          />
        </Modal>
      ) : null}

      <div className="grid grid-cols-[minmax(0,1fr)_320px] items-start gap-4 p-6">
        <Card>
          <Label className="mb-3">About</Label>
          {error ? (
            <p className="text-sm leading-relaxed text-destructive">{error}</p>
          ) : !game ? (
            <AsciiBar fraction={null} cells={16} showPct={false} className="text-muted-foreground" />
          ) : (
            <p className="text-sm leading-relaxed text-muted-foreground">
              {game.short ?? 'kryo.to has no description for this one yet. Its store page has everything else.'}
            </p>
          )}
        </Card>
        <Card className="grid gap-3">
          <Label>Info</Label>
          {game ? (
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs">
              {game.developer ? <Fact k="Developer" v={game.developer} /> : null}
              {game.source ? <Fact k="Release" v={game.source} /> : null}
              {game.version ? <Fact k="Build" v={game.version} /> : null}
            </dl>
          ) : null}
          <Button variant="outline" size="sm" onClick={onStorePage} className="w-fit">
            <Globe className="size-3" />
            Store page
          </Button>
        </Card>
      </div>
    </section>
  )
}

function Fact({ k, v }: { k: string; v: string }) {
  return (
    <>
      <dt className="text-[10px] uppercase tracking-wider text-muted-foreground">{k}</dt>
      <dd className="m-0 select-text break-all text-foreground/90">{v}</dd>
    </>
  )
}
