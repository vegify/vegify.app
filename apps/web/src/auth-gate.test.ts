import { readdirSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { describe, expect, it } from "vitest"

import {
  isBarePath,
  isPublicPath,
  PUBLIC_PATHS,
  PUBLIC_SECTIONS,
  STATIC_TOP_LEVEL
} from "./auth-gate"

describe("isPublicPath", () => {
  it("serves the landing and the auth/token pages to logged-out visitors", () => {
    for (const p of [
      "/",
      "/login",
      "/signup",
      "/forgot",
      "/reset",
      "/verify"
    ]) {
      expect(isPublicPath(p), `${p} should be public`).toBe(true)
    }
  })

  it('treats a "/<username>" handle as a public, shareable profile', () => {
    for (const p of [
      "/simone",
      "/best-cook",
      "/x9",
      "/the-biga-guy",
      "/vegan123"
    ]) {
      expect(isPublicPath(p), `${p} should be a public profile`).toBe(true)
    }
  })

  it('treats "/<username>/<recipe-slug>" as a public, shareable recipe', () => {
    for (const p of [
      "/simone/complete-shake",
      "/dev-user/biga",
      "/x9/pizza-dough-2"
    ]) {
      expect(isPublicPath(p), `${p} should be a public recipe`).toBe(true)
    }
    // …but a static section's second segment is NOT a profile-recipe (it routes elsewhere / gates).
    expect(
      isPublicPath("/settings/foo"),
      "a static section is not a profile-recipe"
    ).toBe(false)
  })

  it("serves the public catalog sections and their detail pages logged-out", () => {
    for (const p of [
      "/recipes",
      "/ingredients",
      "/recipes/abc123",
      "/ingredients/abc123"
    ]) {
      expect(isPublicPath(p), `${p} should be public`).toBe(true)
    }
  })

  it("serves the blog (index + posts) logged-out — pure public content", () => {
    for (const p of ["/blog", "/blog/vegan-honestly"]) {
      expect(isPublicPath(p), `${p} should be public`).toBe(true)
    }
  })

  it("renders the blog bare (own chrome, no app shell) but keeps the app pages shelled", () => {
    for (const p of ["/blog", "/blog/vegan-honestly", "/login", "/reset"]) {
      expect(isBarePath(p), `${p} should render bare`).toBe(true)
    }
    for (const p of [
      "/",
      "/recipes",
      "/recipes/abc123",
      "/settings",
      "/simone"
    ]) {
      expect(isBarePath(p), `${p} should keep the shell`).toBe(false)
    }
  })

  it("gates settings and the create/edit leaves even under a public section", () => {
    for (const p of [
      "/settings",
      "/recipes/new",
      "/recipes/abc123/edit",
      "/ingredients/new",
      "/ingredients/abc123/edit"
    ]) {
      expect(isPublicPath(p), `${p} should be gated`).toBe(false)
    }
  })

  // The fail-closed regression test. EVERY static top-level route — re-derived here straight off the
  // filesystem, a different mechanism than the module's import.meta.glob — must be either an explicit
  // public page or gated, NEVER served as a "/<username>" profile. Add routes/dashboard.tsx and it is
  // covered automatically; if the module's derivation ever silently empties (e.g. a broken glob), these
  // routes fall through to "public" and this test goes red.
  it("never serves a static top-level route as a public profile", () => {
    const routesDir = join(dirname(fileURLToPath(import.meta.url)), "routes")
    const segments = readdirSync(routesDir)
      .filter((f) => /\.(ts|tsx)$/.test(f))
      .map((f) => {
        const cleaned = f.replace(/\.(ts|tsx)$/, "")
        return cleaned.split(".")[0] ?? cleaned
      })
      .filter(
        (s) =>
          s !== "__root" &&
          s !== "index" &&
          !s.startsWith("$") &&
          !s.startsWith("_")
      )

    expect(
      segments.length,
      "expected to find route files on disk"
    ).toBeGreaterThan(0)
    for (const seg of segments) {
      const path = `/${seg}`
      expect(
        STATIC_TOP_LEVEL.has(path),
        `${path} should be a known static route`
      ).toBe(true)
      // Gated unless explicitly opted into public (PUBLIC_PATHS) or a public catalog section. THIS test's
      // narrower job — a static route must never be reachable as a "/<username>" profile — is already
      // guaranteed by its STATIC_TOP_LEVEL membership asserted just above.
      if (!PUBLIC_PATHS.has(path) && !PUBLIC_SECTIONS.includes(path)) {
        expect(
          isPublicPath(path),
          `${path} is a static route and must be gated`
        ).toBe(false)
      }
    }
  })
})
