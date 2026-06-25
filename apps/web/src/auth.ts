import { createServerFn } from '@tanstack/react-start'
import { api, ApiError, SESSION_COOKIE } from './api'

// Web auth surface: an opaque session token (minted by the standing Axum backend) in an httpOnly
// cookie. The handlers proxy to vegify-server's /api/auth/* — the web holds no users/sessions store
// of its own; the backend is the source of truth. See [[server-source-of-truth]].

export { SESSION_COOKIE }

export type AuthUser = { id: string; name: string; email: string }

// Matches the server's session TTL (auth.rs SESSION_TTL_MS). The cookie may outlive the server session
// by milliseconds of clock skew — harmless: the backend 401s and the auth gate redirects to /login.
const SESSION_TTL_MS = 1000 * 60 * 60 * 24 * 30 // 30 days

async function setSessionCookie(token: string) {
  const { setCookie, getRequestProtocol } = await import('@tanstack/react-start/server')
  setCookie(SESSION_COOKIE, token, {
    httpOnly: true,
    // http on localhost, https behind CloudFront (x-forwarded-proto) — a Secure cookie would be
    // dropped by the browser over plain http in dev.
    secure: getRequestProtocol() === 'https',
    sameSite: 'lax',
    path: '/',
    expires: new Date(Date.now() + SESSION_TTL_MS),
  })
}

/** Server-side: the current user via the backend's whoami (Bearer = the session cookie), or null. */
export const fetchUser = createServerFn({ method: 'GET' }).handler(async (): Promise<AuthUser | null> => {
  try {
    return await api<AuthUser>('/api/auth/session')
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) return null // no/expired session
    throw e
  }
})

export const loginFn = createServerFn({ method: 'POST' })
  .validator((d: { email: string; password: string }) => d)
  .handler(async ({ data }): Promise<{ ok: boolean; error?: string }> => {
    try {
      const { token } = await api<{ token: string }>('/api/auth/login', {
        method: 'POST',
        auth: false,
        body: { email: data.email, password: data.password },
      })
      await setSessionCookie(token)
      return { ok: true }
    } catch (e) {
      // The backend returns 401 "Invalid email or password." / 400 for missing fields.
      return { ok: false, error: e instanceof Error ? e.message : 'Sign-in failed.' }
    }
  })

export const signupFn = createServerFn({ method: 'POST' })
  .validator((d: { name: string; email: string; password: string }) => d)
  .handler(async ({ data }): Promise<{ ok: boolean; error?: string }> => {
    const name = data.name.trim()
    const email = data.email.trim()
    if (!name || !email) return { ok: false, error: 'Name and email are required.' }
    if (data.password.length < 8) return { ok: false, error: 'Password must be at least 8 characters.' }
    try {
      const { token } = await api<{ token: string }>('/api/auth/signup', {
        method: 'POST',
        auth: false,
        body: { name, email, password: data.password },
      })
      await setSessionCookie(token)
      return { ok: true }
    } catch (e) {
      // The backend returns 409 on a duplicate email.
      return { ok: false, error: e instanceof Error ? e.message : 'Sign-up failed.' }
    }
  })

export const logoutFn = createServerFn({ method: 'POST' }).handler(async () => {
  const { getCookie, deleteCookie } = await import('@tanstack/react-start/server')
  const token = getCookie(SESSION_COOKIE)
  if (token) {
    // Best-effort server-side invalidation (forwards the cookie token as Bearer); clear the cookie
    // regardless so the browser is logged out even if the backend call fails.
    try {
      await api('/api/auth/logout', { method: 'POST' })
    } catch {
      // ignore — the cookie delete below is what logs the browser out
    }
  }
  deleteCookie(SESSION_COOKIE, { path: '/' })
  return { ok: true }
})
