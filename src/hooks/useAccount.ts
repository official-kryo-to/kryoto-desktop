import { useEffect, useState } from 'react'
import { call, isTauri, on } from '@/lib/bridge'
import { isOnline } from '@/lib/online'
import type { AccountAppearance } from '@/lib/settings'

export type Account = {
  username: string
  displayName: string | null
  avatarUrl: string | null
  appearance?: AccountAppearance | null
  /** A kryo.to supporter, or someone who bought "no ads": never asked to donate. */
  supporter?: boolean
  /** Chat is rolled out to this account (kryo.to feature flag). */
  chat?: boolean
  /** Friends are rolled out to this account (kryo.to feature flag). */
  friends?: boolean
  /** Group chats are rolled out to this account (kryo.to feature flag). */
  groups?: boolean
  /** The public room is rolled out to this account (kryo.to feature flag). */
  room?: boolean
  /** Voice calls are rolled out to this account (kryo.to feature flag). */
  voice?: boolean
  /** Controller support is rolled out to this account (kryo.to feature flag). */
  controller?: boolean
  /**
   * Their Kryos, the coin of kryo.to's Hatchery, when they have a wallet.
   * Shown beside the account; a click opens the Hatchery (18+) in the Store.
   */
  kryos?: number | null
  /** Using the client without an account (see App). */
  guest?: boolean
}

/** Stands in for an account while using the client as a guest. */
export const GUEST: Account = { username: '', displayName: 'Guest', avatarUrl: null, appearance: null, guest: true }

/** The site's actual theme, also available when signed out. */
export function useCatalogTheme() {
  const [theme, setTheme] = useState<'light' | 'dark' | null>(null)
  useEffect(() => {
    let stop: (() => void) | undefined
    let cancelled = false
    void on<'light' | 'dark'>('catalog-theme', value => { if (!cancelled) setTheme(value) }).then(fn => {
      if (cancelled) return fn()
      stop = fn
      if (isTauri()) void call('store_refresh_account').catch(() => {})
    }).catch(() => {})
    return () => { cancelled = true; stop?.() }
  }, [])
  return theme
}

/**
 * Who is signed in to kryo.to - read from the Store web view, which holds the
 * real session. `undefined` until the Store has loaded and said; `null` when
 * signed out. One sign-in, shared by the Store and the client.
 */
/**
 * The last account kryo.to reported, kept so a start with no connection opens
 * the client as that account (the Store cannot say who it is offline).
 */
const REMEMBER = 'kryoto.account'
function remembered(): Account | undefined {
  try {
    const a = JSON.parse(localStorage.getItem(REMEMBER) ?? 'null') as Account | null
    return a?.username ? a : undefined
  } catch {
    return undefined
  }
}
function remember(a: Account | null) {
  try {
    if (a) localStorage.setItem(REMEMBER, JSON.stringify(a))
    else localStorage.removeItem(REMEMBER)
  } catch {
    /* not important */
  }
}

export function useAccount(): Account | null | undefined {
  const [account, setAccount] = useState<Account | null | undefined>(() =>
    isTauri()
      ? isOnline()
        ? undefined
        : remembered()
      : { username: 'mira', displayName: 'Mira', avatarUrl: null, appearance: null, kryos: 12480, chat: true, friends: true, groups: true, room: true, controller: true },
  )
  useEffect(() => {
    let stop: (() => void) | undefined
    let cancelled = false
    void on<Account | null>('account-state', (a) => {
      remember(a)
      setAccount(a)
    }).then((fn) => {
      if (cancelled) fn()
      else stop = fn
    })
    return () => {
      cancelled = true
      stop?.()
    }
  }, [])
  return account
}
