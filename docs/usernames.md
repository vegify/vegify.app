# Usernames and public URLs

## The decision (locked 2026-06-28)

Public profiles and recipes live at **apex-namespaced URLs**: `vegify.app/<username>/<recipe-slug>` (and `vegify.app/<username>` for a profile). Everything stays on the apex `vegify.app`. There is no `app.vegify.app` / `run.vegify.app` split.

Why apex, not a subdomain: on a user-generated recipe site the SEO and GEO value lives in the long tail of public recipe pages, not the marketing homepage. Those pages have to sit on the apex so all link equity pools into one domain and the canonical, shareable, citable URL is the strong one. A subdomain would hand the crawlable content the weaker address. The split that matters is **public vs gated at the route level** (public pages are SSR'd, crawlable, cacheable; create/edit/dashboard are gated behind `/login`), not marketing vs app at the domain level. See `.claude/state.md` UPDATE 21 and the `PUBLIC_PATHS` gate in `apps/web/src/routes/__root.tsx`.

## Reserved handles

Because `<username>` is the first path segment, it competes with every app route (`/login`, `/recipes`, `/api`, ...), and a handle once claimed can never be safely reclaimed. So the reserved set is locked **before** usernames launch.

- Authoritative list + validator: **`crates/vegify-server/src/handles.rs`** (`RESERVED`, `validate_username`, `is_reserved`). Server-side because that is where signup enforces it.
- The web keeps **no copy**. A client-side "is this handle free?" check must reach the server anyway (uniqueness is server-only), so availability will go through a server `check-handle` endpoint. Nothing to mirror, nothing to drift.
- A unit test (`every_current_route_segment_is_reserved`) fails CI if a current top-level route stops being reserved. **When you add a new top-level route, add its segment to `RESERVED` and to that test.**
- The list covers: current route segments, planned route segments (meal plans, social, settings from the site map), system/security/impersonation words, brand/infra/email words, and generic placeholders. It deliberately does **not** include a profanity/abuse blocklist; that is a separate content-moderation concern.

## Username format

Validated and normalized by `handles::validate_username`, which returns the canonical lowercased handle or a reason:

- 2 to 30 characters.
- Lowercase `a-z`, digits `0-9`, and hyphens only. Input is lowercased, so handles are case-insensitive (canonical form is lowercase).
- No leading, trailing, or doubled hyphens.
- Not all digits (avoids ambiguity with any future numeric-id routes).
- Not a reserved word.

These are sensible GitHub-style defaults and are adjustable; widening the lower length bound later is safe, narrowing it is not.

## Deferred: the username + public-recipe launch

The reservation is set up; turning on usernames and public recipe pages is a deliberate later step:

- [ ] Add a `username` column to `users` (Drizzle `packages/db/src/schema.ts`, unique, case-insensitive), propagate to the server schema and the desktop `schema.sql` bootstrap, migrate the standing prod DB.
- [ ] Call `handles::validate_username` in `signup` (and a future rename endpoint), then check DB uniqueness on the normalized handle. Collect the username in the signup form (`packages/ui/src/auth-form.tsx`) once signups open (`SIGNUPS_ENABLED` / server `VEGIFY_SIGNUPS_OPEN`).
- [ ] Add a `check-handle` endpoint for live availability UX.
- [ ] Add recipe slugs (unique per user) and the public `/<username>/<recipe-slug>` route: render read-only for logged-out visitors (un-gated, per the public-by-default model), with a "sign in to save or fork" CTA. This is the real SEO/GEO engine.
