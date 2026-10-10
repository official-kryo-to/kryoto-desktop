import { useEffect, useMemo, useRef, useState } from 'react'
import { BellOff, Globe, MessageSquare, MessageSquarePlus, Plus, Search, ShieldCheck, Star, UserPlus, Users, Gamepad2, X } from 'lucide-react'
import { artSrc } from '@/lib/art'
import { Button, Caption, IconButton, inputCls, Label } from '@/ui'
import type { Account } from '@/hooks/useAccount'
import { friendName, useFriends, type Friend, type FriendFace } from '@/hooks/useFriends'
import { AsciiArt } from '@/ui/ascii/AsciiArt'
import { CHAT } from '@/ui/ascii/scenes'
import { ChatSetup } from './ChatSetup'
import { ChatPane } from './ChatPane'
import {
  chatConversations,
  chatGroups,
  chatMessageRequest,
  chatMessageRequestRespond,
  chatPeople,
  chatSetContext,
  groupConversationId,
  groupTarget,
  useChatStatus,
  type ChatPersonInfo,
  type GroupView,
  type Invite,
} from '@/lib/chat'
import { GroupCreateDialog, GroupMembersDialog } from './GroupDialogs'
import { RoomPane } from './RoomPane'
import type { InviteGame } from './InvitePicker'
import { call, errorText, on } from '@/lib/bridge'

/**
 * Friends & chat - laid out the way it will work (your card, the friends
 * list, the chat pane), with the parts that do not exist yet saying so.
 * The plan is in FRIENDS-AND-CHAT.md at the top of the workspace.
 */
export function FriendsPage({
  account,
  onProfile,
  onDiscord,
  onWeb,
  chatWith,
  games = [],
  onJoinInvite,
}: {
  account: Account
  onProfile: () => void
  onDiscord: () => void
  /** Open a kryo.to page in the Store (a profile, /friends). */
  onWeb: (path: string) => void
  /** A `kryoto://chat/<username>` link to open (`at` makes each one new). */
  chatWith?: { username: string; at: number } | null
  /** Library games that can be invited to. */
  games?: InviteGame[]
  onJoinInvite?: (invite: Invite) => void
}) {
  const name = account.displayName || account.username
  const state = useFriends()
  const [search, setSearch] = useState('')
  const live = account.friends && state
  const needle = search.trim().toLowerCase()
  const matches = (f: FriendFace & { nickname?: string | null }) =>
    !needle || [f.nickname, f.displayName, f.username].some((v) => v?.toLowerCase().includes(needle))
  const favourites = live ? state.friends.filter((f) => f.favourite && matches(f)) : []
  const others = live ? state.friends.filter((f) => !f.favourite && matches(f)) : []
  const inGame = others.filter((f) => f.playing)
  const online = others.filter((f) => !f.playing && f.online)
  const offline = others.filter((f) => !f.playing && !f.online)

  // Chat: who this PC is in chat (kept through a short disconnect), which
  // conversation is open, and unread counts per friend.
  const chat = useChatStatus()
  const myIdRef = useRef<string | null>(null)
  if (chat?.state === 'online') myIdRef.current = chat.userId
  const chatOn = chat != null && ['online', 'connecting', 'offline'].includes(chat.state) && myIdRef.current != null
  const [selected, setSelected] = useState<string | null>(null)
  const [unread, setUnread] = useState<Record<string, number>>({})
  // Group chats: the list, names of members who are not friends, dialogs.
  const groupsOn = !!account.groups
  const [groups, setGroups] = useState<GroupView[]>([])
  const [extraPeople, setExtraPeople] = useState<Record<string, ChatPersonInfo>>({})
  const [creatingGroup, setCreatingGroup] = useState(false)
  const [membersOf, setMembersOf] = useState<string | null>(null)

  // People who are not friends: their requests, ours, and accepted chats.
  // `opened` and `answered` cover the moments before the next report arrives.
  const [opened, setOpened] = useState<FriendFace[]>([])
  const [answered, setAnswered] = useState<Record<string, 'accepted' | 'declined'>>({})
  const [composing, setComposing] = useState(false)
  const [to, setTo] = useState('')
  const [toError, setToError] = useState<string | null>(null)
  const [asking, setAsking] = useState(false)
  const strangers = useMemo(() => {
    if (!live) return { requests: [] as FriendFace[], chats: [] as FriendFace[], outgoing: new Set<string>() }
    const friendIds = new Set(state.friends.map((f) => f.id))
    const hidden = new Set(state.hidden)
    const keep = (p: FriendFace) => !friendIds.has(p.id) && !hidden.has(p.username.toLowerCase()) && answered[p.id] !== 'declined'
    const mr = state.messageRequests
    const requests = mr.incoming.filter((p) => keep(p) && answered[p.id] !== 'accepted')
    const seen = new Set(requests.map((p) => p.id))
    const chats: FriendFace[] = []
    for (const p of [...mr.incoming.filter((p) => answered[p.id] === 'accepted'), ...mr.accepted, ...mr.outgoing, ...opened]) {
      if (keep(p) && !seen.has(p.id)) {
        seen.add(p.id)
        chats.push(p)
      }
    }
    const accepted = new Set(mr.accepted.map((p) => p.id))
    const outgoing = new Set([...mr.outgoing, ...opened].map((p) => p.id).filter((id) => !accepted.has(id)))
    return { requests, chats, outgoing }
  }, [live, state, opened, answered])
  const requestOf = (id: string): 'incoming' | 'outgoing' | undefined =>
    strangers.requests.some((p) => p.id === id) ? 'incoming' : strangers.outgoing.has(id) ? 'outgoing' : undefined
  const peer: (FriendFace & { nickname?: string | null }) | undefined =
    live && selected
      ? (state.friends.find((f) => f.id === selected) ?? [...strangers.requests, ...strangers.chats].find((p) => p.id === selected))
      : undefined
  const openGroup = selected?.startsWith('g:') ? groups.find((g) => groupTarget(g.id) === selected) : undefined

  // Load groups while chat is on, and again whenever one changes.
  useEffect(() => {
    if (!chatOn || !groupsOn) return
    let alive = true
    const load = () => void chatGroups().then((g) => alive && setGroups(g)).catch(() => {})
    load()
    const stop = on('chat-group-changed', load)
    return () => {
      alive = false
      void stop.then((f) => f())
    }
  }, [chatOn, groupsOn])

  // Names for group members who are not friends (kryo.to answers only for
  // people you share a group with).
  useEffect(() => {
    if (!live || groups.length === 0) return
    const known = new Set([...state.friends.map((f) => f.id), ...Object.keys(extraPeople), myIdRef.current ?? ''])
    const missing = [...new Set(groups.flatMap((g) => g.members.map((m) => m.userId)))].filter((id) => !known.has(id))
    if (missing.length === 0) return
    void chatPeople(missing)
      .then((r) => setExtraPeople((m) => ({ ...m, ...Object.fromEntries(r.people.map((p) => [p.id, p])) })))
      .catch(() => {})
  }, [groups, live, state, extraPeople])

  const nameOf = (id: string): string => {
    const f = live ? state.friends.find((x) => x.id === id) : undefined
    if (f) return friendName(f)
    const e = extraPeople[id]
    return e ? e.displayName || e.username : 'Someone'
  }

  // The engine learns names, supporter status and mutes from the lists.
  useEffect(() => {
    if (!live) return
    const people = [
      ...state.friends.map((f) => ({ id: f.id, name: friendName(f), supporter: f.supporter, muted: f.muted })),
      ...strangers.requests.map((p) => ({ id: p.id, name: friendName(p), supporter: p.supporter, muted: false, pending: true })),
      ...strangers.chats.map((p) => ({ id: p.id, name: friendName(p), supporter: p.supporter, muted: false })),
    ]
    const listed = new Set(people.map((p) => p.id))
    for (const e of Object.values(extraPeople)) {
      if (!listed.has(e.id)) people.push({ id: e.id, name: e.displayName || e.username, supporter: e.supporter, muted: false })
    }
    void chatSetContext(people, !!account.supporter).catch(() => {})
  }, [live, state, strangers, extraPeople, account.supporter])

  const refreshLists = () => void call('store_refresh_account').catch(() => {})

  const startChat = async () => {
    const username = to.trim().replace(/^@/, '')
    if (!username || asking) return
    setAsking(true)
    setToError(null)
    try {
      const r = await chatMessageRequest(username)
      const face: FriendFace = { id: r.userId, username: r.username, displayName: r.name, avatarUrl: null, supporter: false }
      if (r.status !== 'friends') setOpened((list) => [face, ...list.filter((p) => p.id !== face.id)])
      setSelected(face.id)
      setComposing(false)
      setTo('')
      refreshLists()
    } catch (e) {
      setToError(errorText(e))
    } finally {
      setAsking(false)
    }
  }

  // A chat link: a friend opens straight away; anyone else goes through the
  // message request (which also answers whether they can be messaged).
  const handledLink = useRef(0)
  useEffect(() => {
    if (!chatWith || handledLink.current === chatWith.at || !live || !chatOn) return
    handledLink.current = chatWith.at
    const name = chatWith.username.toLowerCase()
    const known = [...state.friends, ...strangers.requests, ...strangers.chats].find((p) => p.username.toLowerCase() === name)
    if (known) {
      setSelected(known.id)
      return
    }
    setComposing(true)
    setTo(chatWith.username)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chatWith, live, chatOn])

  const respond = async (id: string, accept: boolean) => {
    await chatMessageRequestRespond(id, accept)
    setAnswered((a) => ({ ...a, [id]: accept ? 'accepted' : 'declined' }))
    if (!accept) setSelected(null)
    refreshLists()
  }

  useEffect(() => {
    if (!chatOn) return
    let alive = true
    const refresh = () =>
      void chatConversations()
        .then((list) => {
          if (!alive) return
          // People by their id; groups by their conversation id.
          setUnread(Object.fromEntries(list.map((c) => [c.peerUserId ?? c.id, c.unread])))
        })
        .catch(() => {})
    refresh()
    const stops = [on('chat-message', refresh), on('chat-updated', refresh), on('chat-read', refresh)]
    return () => {
      alive = false
      stops.forEach((p) => void p.then((f) => f()))
    }
  }, [chatOn, selected])

  const openFriend = (f: FriendFace) => {
    if (chatOn) setSelected(f.id)
    else onWeb(`/user/${encodeURIComponent(f.username)}`)
  }
  return (
    <div className="grid min-h-0 grow grid-cols-[300px_1fr]">
      <aside className="grid min-h-0 grid-rows-[auto_auto_1fr] border-r border-border bg-card/40">
        <button
          type="button"
          onClick={onProfile}
          className="kryo-square flex items-center gap-3 border-b border-border p-4 text-left transition-colors hover:bg-secondary"
        >
          {account.avatarUrl ? (
            <img src={artSrc(account.avatarUrl) ?? undefined} alt="" className="kryo-pill size-11 object-cover" />
          ) : (
            <span className="kryo-pill grid size-11 place-items-center bg-secondary text-sm font-bold">{name.slice(0, 1).toUpperCase()}</span>
          )}
          <span className="grid min-w-0">
            <b className="truncate text-sm text-foreground">{name}</b>
            <span className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-success">
              <span className="size-1.5 rounded-full bg-success" />
              Online
            </span>
          </span>
        </button>
        <div className="flex gap-2 border-b border-border p-3">
          <label className={`kryo-pill flex h-8 grow items-center gap-2 border border-border px-3 text-[11px] text-muted-foreground${live ? '' : ' opacity-60'}`}>
            <Search className="size-3" aria-hidden />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              disabled={!live}
              placeholder="Search friends"
              aria-label="Search friends"
              className="min-w-0 grow bg-transparent text-foreground outline-none placeholder:text-muted-foreground"
            />
          </label>
          {live && chatOn ? (
            <Button size="sm" title="New message" aria-label="New message" onClick={() => setComposing((v) => !v)}>
              <MessageSquarePlus className="size-3" aria-hidden />
            </Button>
          ) : null}
          <Button size="sm" disabled={!live} title={live ? 'Add a friend' : 'Coming soon'} onClick={() => onWeb('/friends')}>
            <UserPlus className="size-3" aria-hidden />
          </Button>
        </div>
        {composing && live && chatOn ? (
          <form
            className="grid gap-2 border-b border-border p-3"
            onSubmit={(e) => {
              e.preventDefault()
              void startChat()
            }}
          >
            <div className="flex items-center justify-between">
              <Caption>New message</Caption>
              <IconButton label="Close" className="size-6" onClick={() => setComposing(false)}>
                <X className="size-3" />
              </IconButton>
            </div>
            <div className="flex gap-2">
              <input
                autoFocus
                value={to}
                onChange={(e) => setTo(e.target.value)}
                placeholder="Their exact username"
                aria-label="Username"
                className={inputCls}
              />
              <Button type="submit" size="sm" variant="primary" disabled={!to.trim() || asking}>
                Open
              </Button>
            </div>
            {toError ? <p className="text-[11px] text-destructive">{toError}</p> : null}
            <p className="text-[11px] text-muted-foreground">
              Not a friend? They get a message request, if they accept messages from everyone.
            </p>
          </form>
        ) : null}
        {live ? (
          <div className="grid content-start gap-5 overflow-auto p-4">
            {state.incoming.length > 0 ? (
              <div className="grid gap-2">
                <Caption className="flex items-center justify-between">
                  <span>Requests</span>
                  <span>{state.incoming.length}</span>
                </Caption>
                {state.incoming.map((r) => (
                  <button
                    key={r.requestId}
                    type="button"
                    onClick={() => onWeb('/friends')}
                    className="kryo-radius flex items-center justify-between gap-2 border border-border px-3 py-2 text-left text-xs text-foreground transition-colors hover:bg-secondary"
                  >
                    <FriendRow f={r} label={friendName(r)} />
                    <span className="text-[10px] uppercase tracking-wider text-muted-foreground">Respond</span>
                  </button>
                ))}
              </div>
            ) : null}
            {strangers.requests.length > 0 ? (
              <PeopleGroup title="Message requests" list={strangers.requests} unread={unread} selected={selected} onOpen={openFriend} />
            ) : null}
            {favourites.length > 0 ? (
              <FriendGroup title="Favourites" list={favourites} unread={unread} selected={selected} onOpen={openFriend} />
            ) : null}
            {inGame.length > 0 ? (
              <FriendGroup title="In game" list={inGame} unread={unread} selected={selected} onOpen={openFriend} />
            ) : null}
            {online.length > 0 ? (
              <FriendGroup title="Online" list={online} unread={unread} selected={selected} onOpen={openFriend} />
            ) : null}
            <FriendGroup
              title={inGame.length + online.length > 0 ? 'Offline' : 'Friends'}
              list={offline}
              total={inGame.length + online.length > 0 ? offline.length : state.friends.length}
              empty={
                needle
                  ? others.length === 0
                    ? 'Nobody matches.'
                    : undefined
                  : state.friends.length === 0
                    ? 'No friends yet. Add someone from their profile or with your invite link.'
                    : undefined
              }
              unread={unread}
              selected={selected}
              onOpen={openFriend}
            />
            {strangers.chats.length > 0 ? (
              <PeopleGroup title="Other chats" list={strangers.chats} unread={unread} selected={selected} onOpen={openFriend} />
            ) : null}
            {account.room && chatOn ? (
              <button
                type="button"
                onClick={() => setSelected('room')}
                aria-current={selected === 'room' ? 'true' : undefined}
                className={`kryo-radius flex items-center gap-2.5 px-2 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-secondary${selected === 'room' ? ' bg-secondary' : ''}`}
              >
                <span className="kryo-pill grid size-7 shrink-0 place-items-center bg-secondary">
                  <Globe className="size-3.5 text-muted-foreground" aria-hidden />
                </span>
                <span className="grid min-w-0">
                  <span>Public room</span>
                  <span className="text-[10px] text-muted-foreground">Everyone, not encrypted</span>
                </span>
              </button>
            ) : null}
            {groupsOn && chatOn ? (
              <div className="grid gap-1">
                <Caption className="mb-1 flex items-center justify-between">
                  <span>Group chats</span>
                  <button type="button" className="flex items-center gap-1 normal-case tracking-normal hover:text-foreground" onClick={() => setCreatingGroup(true)}>
                    <Plus className="size-3" aria-hidden /> New
                  </button>
                </Caption>
                {groups.length === 0 ? (
                  <p className="kryo-radius border border-dashed border-border px-3 py-2.5 text-[11px] text-muted-foreground">
                    Start a group with your friends.
                  </p>
                ) : null}
                {groups.map((g) => {
                  const key = groupTarget(g.id)
                  const n = unread[groupConversationId(g.id)] ?? 0
                  return (
                    <button
                      key={g.id}
                      type="button"
                      onClick={() => setSelected(key)}
                      aria-current={selected === key ? 'true' : undefined}
                      className={`kryo-radius flex items-center justify-between gap-2 px-2 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-secondary${selected === key ? ' bg-secondary' : ''}`}
                    >
                      <span className="flex min-w-0 items-center gap-2.5">
                        <span className="kryo-pill grid size-7 shrink-0 place-items-center bg-secondary">
                          <Users className="size-3.5 text-muted-foreground" aria-hidden />
                        </span>
                        <span className="grid min-w-0">
                          <span className="truncate">{g.name}</span>
                          <span className="text-[10px] text-muted-foreground">{g.members.length} people</span>
                        </span>
                      </span>
                      {n ? (
                        <span className="kryo-pill bg-primary px-1.5 text-[10px] font-bold text-primary-foreground" aria-label={`${n} unread`}>
                          {n}
                        </span>
                      ) : null}
                    </button>
                  )
                })}
              </div>
            ) : (
              <Group title="Group chats" />
            )}
          </div>
        ) : (
          <div className="grid content-start gap-5 overflow-auto p-4">
            <Group title="Friends" />
            <Group title="Online" />
            <Group title="Group chats" />
          </div>
        )}
      </aside>

      {creatingGroup && live ? (
        <GroupCreateDialog
          friends={state.friends}
          onClose={() => setCreatingGroup(false)}
          onCreated={(g) => {
            setCreatingGroup(false)
            setGroups((list) => [g, ...list.filter((x) => x.id !== g.id)])
            setSelected(groupTarget(g.id))
          }}
        />
      ) : null}
      {membersOf && myIdRef.current && live ? (
        (() => {
          const g = groups.find((x) => x.id === membersOf)
          return g ? (
            <GroupMembersDialog
              group={g}
              myId={myIdRef.current}
              friends={state.friends}
              nameOf={nameOf}
              onClose={() => setMembersOf(null)}
              onLeft={() => {
                setMembersOf(null)
                setSelected(null)
                setGroups((list) => list.filter((x) => x.id !== g.id))
              }}
            />
          ) : null
        })()
      ) : null}

      {selected === 'room' && chatOn && account.room ? (
        <RoomPane myUsername={account.username} onProfile={(u) => onWeb(`/user/${encodeURIComponent(u)}`)} />
      ) : openGroup && chatOn && myIdRef.current ? (
        <ChatPane
          key={selected ?? ''}
          peer={{ id: groupTarget(openGroup.id), name: openGroup.name, supporter: false }}
          myId={myIdRef.current}
          meSupporter={!!account.supporter}
          onProfile={() => {}}
          games={games}
          onJoinInvite={onJoinInvite}
          group={openGroup}
          nameOf={nameOf}
          onMembers={() => setMembersOf(openGroup.id)}
        />
      ) : peer && chatOn && myIdRef.current ? (
        <ChatPane
          key={peer.id}
          peer={{ id: peer.id, name: friendName(peer), supporter: peer.supporter }}
          myId={myIdRef.current}
          meSupporter={!!account.supporter}
          onProfile={() => onWeb(`/user/${encodeURIComponent(peer.username)}`)}
          request={requestOf(peer.id)}
          onRespond={(accept) => respond(peer.id, accept)}
          canCall={!!account.voice}
          games={games}
          onJoinInvite={onJoinInvite}
        />
      ) : (
      <section className="grid min-h-0 place-content-center justify-items-center gap-6 overflow-auto p-10 text-center">
        <AsciiArt lines={CHAT} mode="reveal" revealMs={800} className="h-24 text-muted-foreground" />
        <div className="grid gap-2">
          <Label>Friends &amp; chat</Label>
          <p className="text-2xl font-bold text-foreground">
            {chatOn ? 'Pick someone to talk to' : live ? 'Your friends, in one place' : 'Coming soon'}
          </p>
        </div>
        <ChatSetup available={!!account.chat} />
        {chatOn ? null : (
          <>
            <ul className="grid w-full max-w-md gap-2 text-left">
              <Plan icon={<Users />} title="Friends" body="Add people from their kryo.to profile and see who is online." />
              <Plan icon={<MessageSquare />} title="Chat" body="Direct messages, end-to-end encrypted." />
              <Plan icon={<Gamepad2 />} title="Game invites" body="See what friends are playing and invite them to yours." />
              <Plan icon={<ShieldCheck />} title="Safety" body="Block anyone, and report a message straight to staff." />
            </ul>
            <Button onClick={onDiscord}>Open Discord</Button>
          </>
        )}
      </section>
      )}
    </div>
  )
}

function FriendRow({ f, label }: { f: FriendFace & Partial<Pick<Friend, 'online' | 'playing'>>; label: string }) {
  const dot = f.playing || f.online
  return (
    <span className="flex min-w-0 items-center gap-2.5">
      <span className="relative shrink-0">
        {f.avatarUrl ? (
          <img src={artSrc(f.avatarUrl) ?? undefined} alt="" className="kryo-pill size-7 object-cover" />
        ) : (
          <span className="kryo-pill grid size-7 place-items-center bg-secondary text-[10px] font-bold">
            {label.slice(0, 1).toUpperCase()}
          </span>
        )}
        {dot ? (
          <span
            className={`absolute -bottom-0.5 -right-0.5 size-2.5 rounded-full border-2 border-card ${f.playing ? 'bg-primary' : 'bg-success'}`}
            aria-hidden
          />
        ) : null}
      </span>
      <span className="grid min-w-0">
        <span className="truncate">{label}</span>
        {f.playing ? (
          <span className="truncate text-[10px] text-muted-foreground">Playing {f.playing.title}</span>
        ) : null}
      </span>
    </span>
  )
}

function FriendGroup({
  title,
  list,
  total,
  empty,
  unread,
  selected,
  onOpen,
}: {
  title: string
  list: Friend[]
  total?: number
  empty?: string
  unread: Record<string, number>
  selected: string | null
  onOpen: (f: FriendFace) => void
}) {
  return (
    <div className="grid gap-1">
      <Caption className="mb-1 flex items-center justify-between">
        <span>{title}</span>
        <span>{total ?? list.length}</span>
      </Caption>
      {list.length === 0 && empty ? (
        <p className="kryo-radius border border-dashed border-border px-3 py-2.5 text-[11px] text-muted-foreground">{empty}</p>
      ) : null}
      {list.map((f) => (
        <button
          key={f.id}
          type="button"
          onClick={() => onOpen(f)}
          aria-current={selected === f.id ? 'true' : undefined}
          title={f.nickname ? `${f.displayName || f.username} (your nickname: ${f.nickname})` : undefined}
          className={`kryo-radius flex items-center justify-between gap-2 px-2 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-secondary${selected === f.id ? ' bg-secondary' : ''}`}
        >
          <FriendRow f={f} label={friendName(f)} />
          <span className="flex shrink-0 items-center gap-1 text-muted-foreground">
            {unread[f.id] ? (
              <span className="kryo-pill bg-primary px-1.5 text-[10px] font-bold text-primary-foreground" aria-label={`${unread[f.id]} unread`}>
                {unread[f.id]}
              </span>
            ) : null}
            {f.muted ? <BellOff className="size-3" aria-label="Muted" /> : null}
            {f.favourite ? <Star className="size-3 fill-current" aria-label="Favourite" /> : null}
          </span>
        </button>
      ))}
    </div>
  )
}

/** Non-friends you can chat with: rows like friends, without favourites or mutes. */
function PeopleGroup({
  title,
  list,
  unread,
  selected,
  onOpen,
}: {
  title: string
  list: FriendFace[]
  unread: Record<string, number>
  selected: string | null
  onOpen: (f: FriendFace) => void
}) {
  return (
    <div className="grid gap-1">
      <Caption className="mb-1 flex items-center justify-between">
        <span>{title}</span>
        <span>{list.length}</span>
      </Caption>
      {list.map((p) => (
        <button
          key={p.id}
          type="button"
          onClick={() => onOpen(p)}
          aria-current={selected === p.id ? 'true' : undefined}
          className={`kryo-radius flex items-center justify-between gap-2 px-2 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-secondary${selected === p.id ? ' bg-secondary' : ''}`}
        >
          <FriendRow f={p} label={friendName(p)} />
          {unread[p.id] ? (
            <span className="kryo-pill bg-primary px-1.5 text-[10px] font-bold text-primary-foreground" aria-label={`${unread[p.id]} unread`}>
              {unread[p.id]}
            </span>
          ) : null}
        </button>
      ))}
    </div>
  )
}

function Group({ title }: { title: string }) {
  return (
    <div className="grid gap-2">
      <Caption className="flex items-center justify-between">
        <span>{title}</span>
        <span>0</span>
      </Caption>
      <p className="kryo-radius border border-dashed border-border px-3 py-2.5 text-[11px] text-muted-foreground">Nobody here yet</p>
    </div>
  )
}

function Plan({ icon, title, body }: { icon: React.ReactNode; title: string; body: string }) {
  return (
    <li className="kryo-radius flex items-start gap-3 border border-border bg-card p-3">
      <span className="mt-0.5 text-muted-foreground [&>svg]:size-4">{icon}</span>
      <span className="grid gap-0.5">
        <b className="text-xs text-foreground">{title}</b>
        <span className="text-xs text-muted-foreground">{body}</span>
      </span>
    </li>
  )
}
