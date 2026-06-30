import { describe, expect, it } from 'vitest'
import { readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

import { PUBLIC_PATHS, STATIC_TOP_LEVEL, isPublicPath } from './auth-gate'

describe('isPublicPath', () => {
  it('serves the landing and the auth/token pages to logged-out visitors', () => {
    for (const p of ['/', '/login', '/signup', '/forgot', '/reset', '/verify']) {
      expect(isPublicPath(p), `${p} should be public`).toBe(true)
    }
  })

  it('treats a "/<username>" handle as a public, shareable profile', () => {
    for (const p of ['/simone', '/best-cook', '/x9', '/the-biga-guy', '/vegan123']) {
      expect(isPublicPath(p), `${p} should be a public profile`).toBe(true)
    }
  })

  it('gates the authed top-level sections', () => {
    for (const p of ['/settings', '/recipes', '/ingredients']) {
      expect(isPublicPath(p), `${p} should be gated`).toBe(false)
    }
  })

  it('gates nested authed paths (only a single-segment handle is a profile)', () => {
    for (const p of ['/recipes/new', '/recipes/abc123', '/recipes/abc123/edit', '/ingredients/new']) {
      expect(isPublicPath(p), `${p} should be gated`).toBe(false)
    }
  })

  // The fail-closed regression test. EVERY static top-level route — re-derived here straight off the
  // filesystem, a different mechanism than the module's import.meta.glob — must be either an explicit
  // public page or gated, NEVER served as a "/<username>" profile. Add routes/dashboard.tsx and it is
  // covered automatically; if the module's derivation ever silently empties (e.g. a broken glob), these
  // routes fall through to "public" and this test goes red.
  it('never serves a static top-level route as a public profile', () => {
    const routesDir = join(dirname(fileURLToPath(import.meta.url)), 'routes')
    const segments = readdirSync(routesDir)
      .filter((f) => /\.(ts|tsx)$/.test(f))
      .map((f) => f.replace(/\.(ts|tsx)$/, '').split('.')[0])
      .filter((s) => s !== '__root' && s !== 'index' && !s.startsWith('$') && !s.startsWith('_'))

    expect(segments.length, 'expected to find route files on disk').toBeGreaterThan(0)
    for (const seg of segments) {
      const path = `/${seg}`
      expect(STATIC_TOP_LEVEL.has(path), `${path} should be a known static route`).toBe(true)
      if (!PUBLIC_PATHS.has(path)) {
        expect(isPublicPath(path), `${path} is a static route and must be gated`).toBe(false)
      }
    }
  })
})
