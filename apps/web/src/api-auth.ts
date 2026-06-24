// Bearer-token session auth for the JSON content API (the Tauri desktop client, P1 — server source
// of truth). Mirrors auth.ts's currentUserId(), but reads the `Authorization: Bearer <token>` header
// instead of the browser cookie. The desktop already sends this header on logout; the content API
// reuses it for every request. Returns the session user's id, or null if missing/invalid.
export async function userIdFromRequest(request: Request): Promise<string | null> {
  const token = request.headers.get('authorization')?.replace(/^Bearer\s+/i, '').trim()
  if (!token) return null
  const { validateSession } = await import('@vegify/db')
  const user = await validateSession(token)
  return user?.id ?? null
}

// Wrap a content-API handler: require a valid session (401 if not), run it with the user id, and
// JSON-encode the result. Thrown errors (owner guards, validation) surface as 400 + message — the
// desktop turns those into its DataError. Keeps each route handler to a couple of lines.
export async function withUser(
  request: Request,
  fn: (me: string) => Promise<unknown>,
): Promise<Response> {
  const me = await userIdFromRequest(request)
  if (!me) return Response.json({ error: 'Unauthorized.' }, { status: 401 })
  try {
    return Response.json(await fn(me))
  } catch (e) {
    return Response.json({ error: e instanceof Error ? e.message : 'Request failed.' }, { status: 400 })
  }
}
