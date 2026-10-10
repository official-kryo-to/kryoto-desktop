import { useCallback, useEffect, useRef, useState } from 'react'
import { Copy, FolderOpen, RotateCw, Send } from 'lucide-react'
import { Button, Check, Section } from '@/ui'
import { call, errorText } from '@/lib/bridge'
import { library } from '@/lib/library'

/**
 * Settings > Logs: what the client wrote down, and whether errors go to
 * kryo.to. The same file is in the log folder for attaching to a support
 * message.
 */
export function LogsPane({ sendReports, onSendReports }: { sendReports: boolean; onSendReports: (v: boolean) => void }) {
  const [text, setText] = useState('')
  const [folder, setFolder] = useState('')
  const [note, setNote] = useState<string | null>(null)
  const box = useRef<HTMLPreElement | null>(null)

  const refresh = useCallback(async () => {
    const t = await call<string>('logs_tail', { lines: 400 }).catch((e: unknown) => `Could not read the log: ${errorText(e)}`)
    setText(t)
    requestAnimationFrame(() => box.current && (box.current.scrollTop = box.current.scrollHeight))
  }, [])
  useEffect(() => {
    void refresh()
    void call<string>('logs_folder').then(setFolder).catch(() => {})
    const t = window.setInterval(() => void refresh(), 3000)
    return () => window.clearInterval(t)
  }, [refresh])

  const lines = text ? text.split('\n') : []
  return (
    <>
      <Section title="Error reports" hint="Sends error details, app version, OS, your signed-in username and a persistent app install ID to Kryoto. Error details may include game names and file paths.">
        <Check checked={sendReports} onChange={onSendReports} label="Send error reports to kryo.to" />
      </Section>
      <Section title="Log">
        <pre
          ref={box}
          className="kryo-radius kryo-ascii-art m-0 h-72 select-text overflow-auto border border-border bg-background p-3 text-[10.5px] leading-relaxed"
        >
          {lines.length ? (
            lines.map((l, i) => (
              <div key={i} className={/ ERROR | CRASH /.test(l) ? 'text-destructive' : / WARN /.test(l) ? 'text-warning' : 'text-muted-foreground'}>
                {l}
              </div>
            ))
          ) : (
            <span className="text-muted-foreground">Nothing logged yet.</span>
          )}
        </pre>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" onClick={() => void library.openFolder(folder, true)} disabled={!folder}>
            <FolderOpen className="size-3" />
            Open log folder
          </Button>
          <Button
            size="sm"
            onClick={() =>
              void navigator.clipboard
                .writeText(text)
                .then(() => setNote('Copied.'))
                .catch(() => setNote('Could not copy.'))
            }
          >
            <Copy className="size-3" />
            Copy
          </Button>
          <Button size="sm" onClick={() => void refresh()}>
            <RotateCw className="size-3" />
            Refresh
          </Button>
          <Button
            size="sm"
            disabled={!sendReports}
            onClick={() =>
              void call<number>('logs_send')
                .then((n) => setNote(n ? `Sent ${n} report${n === 1 ? '' : 's'}.` : 'Nothing waiting to send.'))
                .catch((e: unknown) => setNote(errorText(e)))
            }
          >
            <Send className="size-3" />
            Send now
          </Button>
          {note ? <span className="text-[11px] text-muted-foreground">{note}</span> : null}
        </div>
      </Section>
    </>
  )
}
