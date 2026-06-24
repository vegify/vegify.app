import { createServerFn } from '@tanstack/react-start'

// Web auth surface: opaque server-side sessions (a token in an httpOnly cookie). The handlers call
// the shared @vegify/db auth core. The desktop (A2) will reuse the same core over JSON /api routes.

export const SESSION_COOKIE = 'vegify_session'

export type AuthUser = { id: string; name: string; email: string }

async function setSessionCookie(token: string, expires: Date) {
  const { setCookie, getRequestProtocol } = await import('@tanstack/react-start/server')
  setCookie(SESSION_COOKIE, token, {
    httpOnly: true,
    // http on localhost, https behind CloudFront (x-forwarded-proto) — a Secure cookie would be
    // dropped by the browser over plain http in dev.
    secure: getRequestProtocol() === 'https',
    sameSite: 'lax',
    path: '/',
    expires,
  })
}

/** Server-side: the current user's id from the session cookie, or null. Used to stamp writes. */
export async function currentUserId(): Promise<string | null> {
  const { getCookie } = await import('@tanstack/react-start/server')
  const token = getCookie(SESSION_COOKIE)
  if (!token) return null
  const { validateSession } = await import('@vegify/db')
  const user = await validateSession(token)
  return user?.id ?? null
}

export const fetchUser = createServerFn({ method: 'GET' }).handler(
  async (): Promise<AuthUser | null> => {
    const { getCookie } = await import('@tanstack/react-start/server')
    const token = getCookie(SESSION_COOKIE)
    if (!token) return null
    const { validateSession } = await import('@vegify/db')
    const user = await validateSession(token)
    return user ? { id: user.id, name: user.name, email: user.email } : null
  },
)

export const loginFn = createServerFn({ method: 'POST' })
  .validator((d: { email: string; password: string }) => d)
  .handler(async ({ data }): Promise<{ ok: boolean; error?: string }> => {
    const { authenticate, createSession } = await import('@vegify/db')
    const user = await authenticate(data.email, data.password)
    if (!user) return { ok: false, error: 'Invalid email or password.' }
    const { token, expiresAt } = await createSession(user.id)
    await setSessionCookie(token, expiresAt)
    return { ok: true }
  })

export const signupFn = createServerFn({ method: 'POST' })
  .validator((d: { name: string; email: string; password: string }) => d)
  .handler(async ({ data }): Promise<{ ok: boolean; error?: string }> => {
    const name = data.name.trim()
    const email = data.email.trim()
    if (!name || !email) return { ok: false, error: 'Name and email are required.' }
    if (data.password.length < 8)
      return { ok: false, error: 'Password must be at least 8 characters.' }
    const { createUser, createSession } = await import('@vegify/db')
    let userId: string
    try {
      const user = await createUser({ name, email, password: data.password })
      userId = user.id
    } catch {
      // users.email is unique — the realistic failure here is a duplicate.
      return { ok: false, error: 'An account with that email already exists.' }
    }
    const { token, expiresAt } = await createSession(userId)
    await setSessionCookie(token, expiresAt)
    return { ok: true }
  })

export const logoutFn = createServerFn({ method: 'POST' }).handler(async () => {
  const { getCookie, deleteCookie } = await import('@tanstack/react-start/server')
  const token = getCookie(SESSION_COOKIE)
  if (token) {
    const { invalidateSession } = await import('@vegify/db')
    await invalidateSession(token)
  }
  deleteCookie(SESSION_COOKIE, { path: '/' })
  return { ok: true }
})
