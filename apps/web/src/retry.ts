import { isNotFound, isRedirect } from '@tanstack/react-router'

// Retry a server-fn call through a transient failure — most often a 429 from the prod web Lambda's
// reserved-concurrency-1 throttle, when the desktop's background sync and a page load collide on the
// one slot. Short jittered exponential backoff (~120 / 320 / 640ms). Wrap idempotent reads; our saves
// are upserts-by-id, so they're safe too. Server-side (SSR, in-process) the first call succeeds, so
// this is a no-op there — it only does work on the client (HTTP) path where throttling happens.
//
// Control-flow throws (notFound, redirect — TanStack's signal for a 404 or a gated route) are NOT
// transient: they propagate immediately so a real 404 doesn't spin through four retries.
export async function withRetry<T>(fn: () => Promise<T>, tries = 4): Promise<T> {
  let lastErr: unknown
  for (let attempt = 0; attempt < tries; attempt++) {
    try {
      return await fn()
    } catch (err) {
      if (isNotFound(err) || isRedirect(err)) throw err
      lastErr = err
      if (attempt === tries - 1) break
      const delay = 120 * 2 ** attempt + Math.floor(Math.random() * 80)
      await new Promise((resolve) => setTimeout(resolve, delay))
    }
  }
  throw lastErr
}
