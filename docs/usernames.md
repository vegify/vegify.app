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

## Handle tiers

Not every reserved name is reserved for the same reason. Three tiers:

- **System-permanent** — route segments and system words (`login`, `api`, `admin`, `settings`). Never anyone's, ever. This is the `RESERVED` constant in `handles.rs` today (tier-1, code).
- **Held** — a real external entity's name, held so no one can squat or impersonate it, and claimable *only* by that verified entity (`usda`, `gordon-ramsay`, a food brand). This is **data, not code**: a `held_handles` table (handle, reason, optional managing entity, claim status). The set of notable entities is open-ended and changes, so it does not belong in a compiled constant.
- **Open** — everything else; first verified signup wins.

`validate_username` enforces tier-1 today. Tiers 2 and 3 are checked at claim time against the DB (the `held_handles` table plus handle uniqueness), alongside the existing reserved-word check.

## Entity types

A handle's owner is not always a person:

- **`user`** — a person with a login who owns and edits their own recipes.
- **`source` / `org`** — a data source or organization (USDA, a food brand). May be **system-managed** (we run the page) and/or **verified-claimable** (the real org can take it over). Its content carries explicit provenance.
- **`system`** — reserved by the platform, no public page.

The type drives provenance and claimability, not just a badge.

## Onboarding known entities (USDA, celebrities, brands)

A data source and a creator persona both want a good handle, but they are handled oppositely:

- **Data sources (USDA, public reference data)** — model as a **`source`**, never a fabricated user. Imported foods are communal-catalog ingredients carrying a provenance field (`source = "USDA FoodData Central (public domain)"`). Surface a **system-managed `/usda` page** for SEO and trust; impersonation risk is ~zero because the data is clearly labeled public-domain and factual, not recipes attributed to them. `usda` goes in the **held** tier; a verified-USDA claim is a cheap future option but low priority (a federal agency will not manage a Vegify profile). Do not seed it as a user persona.
- **Creator personas (celebrity chefs, cookbook authors, brands with original recipes)** — the opposite. **Reserve the handle (held tier) but do not pre-seed content.** An unclaimed "Gordon Ramsay" page carrying recipes reads as a false endorsement (right-of-publicity, trademark, and copyright risk). The pull is "we saved your name, come claim it," never fabricated content. Onboard via a **verified claim**: prove identity (domain email, social cross-post, or manual review), then grant the handle plus a **verified badge**.

Claim flow (both cases): a verified party requests a held handle, an identity check runs, the handle transfers from held/system ownership to the entity, and content provenance is preserved through the transfer.

## Deferred: the username + public-recipe launch

The reservation is set up; turning on usernames and public recipe pages is a deliberate later step:

- [ ] Add a `username` column to `users` (Drizzle `packages/db/src/schema.ts`, unique, case-insensitive), propagate to the server schema and the desktop `schema.sql` bootstrap, migrate the standing prod DB.
- [ ] Call `handles::validate_username` in `signup` (and a future rename endpoint), then check DB uniqueness on the normalized handle. Collect the username in the signup form (`packages/ui/src/auth-form.tsx`) once signups open (`SIGNUPS_ENABLED` / server `VEGIFY_SIGNUPS_OPEN`).
- [ ] Add a `check-handle` endpoint for live availability UX.
- [ ] Add recipe slugs (unique per user) and the public `/<username>/<recipe-slug>` route: render read-only for logged-out visitors (un-gated, per the public-by-default model), with a "sign in to save or fork" CTA. This is the real SEO/GEO engine.
- [ ] Build tier-2: a `held_handles` table, an entity `type` (user / source / system) on accounts, the claim/verification flow, and a verified badge. Required before onboarding any known entity or importing a named source.
- [ ] USDA import: imported foods carry a `source` provenance field; `usda` is held and system-managed (a `/usda` source page), never a fabricated persona.
