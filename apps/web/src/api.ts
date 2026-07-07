// The web SSR shell holds NO database — it calls the standing Axum backend (vegify-server, the source
// of truth — [[server-source-of-truth]]) over HTTP for ALL auth + content. The browser holds an
// httpOnly cookie carrying the opaque session token Axum issued; on each SSR request the shell reads
// that cookie and forwards the token as `Authorization: Bearer` to Axum, which scopes every read/write
// to the session user. This module is the ONE place the web talks to the backend.
//
// Server-only: `getCookie` is server-only, so importing this module pins it to the server bundle —
// never import a VALUE from here into a client component (types are fine; they erase).

import { apiUrl } from "@vegify/config";

export const SESSION_COOKIE = "vegify_session";

/** The standing backend's base URL (VEGIFY_API_URL; dev default = a local vegify-server). */
export { apiUrl };

/** The current request's opaque session token (from the httpOnly cookie), or null. Server-only — the
 *  dynamic import keeps @tanstack/react-start/server out of the client module graph (api.ts is reachable
 *  from the client via auth.ts), matching how the route handlers gate their server-only imports. */
async function sessionToken(): Promise<string | null> {
  const { getCookie } = await import("@tanstack/react-start/server");
  return getCookie(SESSION_COOKIE) ?? null;
}

/** A backend error carrying the HTTP status, so callers can treat 401 (no/expired session) specially. */
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

type ApiInit = Omit<RequestInit, "body"> & { body?: unknown; auth?: boolean };

/** Call the Axum backend. Attaches the session Bearer (unless `auth: false`), JSON-encodes an object
 *  body, and on a non-2xx surfaces the server's `{error}` message as an ApiError(status). A 2xx with a
 *  JSON `null` body (a forbidden/missing detail row — Axum returns `Option`) resolves to null. */
export async function api<T>(path: string, init: ApiInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.auth !== false) {
    const token = await sessionToken();
    if (token) headers.set("authorization", `Bearer ${token}`);
  }
  let body: BodyInit | undefined;
  if (init.body !== undefined) {
    body =
      typeof init.body === "string" ? init.body : JSON.stringify(init.body);
    if (!headers.has("content-type"))
      headers.set("content-type", "application/json");
  }
  const res = await fetch(`${apiUrl()}${path}`, { ...init, headers, body });
  if (!res.ok) {
    let message = `Request failed (${res.status}).`;
    try {
      const j = (await res.json()) as { error?: string };
      if (j?.error) message = j.error;
    } catch {
      // non-JSON error body — keep the status message
    }
    // Surface genuine backend faults (5xx) to the SSR Lambda's CloudWatch. 4xx is normal flow here —
    // 401 is the auth gate (logged-out), 404 a missing/forbidden detail row — so those stay quiet to
    // keep the logs signal-rich. The browser's own errors ship separately (client-log.ts).
    if (res.status >= 500) {
      console.error(
        `[api] ${init.method ?? "GET"} ${path} -> ${res.status}: ${message}`,
      );
    }
    throw new ApiError(res.status, message);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}
