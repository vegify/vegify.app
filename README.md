# vegify.app

A micronutrition tracker for plant-based cooking. Recipes nest (a recipe *is* an ingredient), and per-100g nutrients roll up through the graph, so you see the micronutrition of a dish, not just its calories. Recipes are user-generated and public by default; ownership gates editing, not reading.

## Architecture

Two clients over one backend, rendering one shared set of screens: each client is a thin data/routing adapter, never a re-implementation.

- **`apps/web`**: a stateless TanStack Start (React) SSR shell. Holds no database: it calls `vegify-server` over HTTP for all auth and content. Deploys as a scale-to-zero Lambda behind CloudFront with client assets on S3.
- **`apps/desktop`**: a local-first Tauri 2 (Rust) app. Full offline CRUD against a local libSQL cache through the shared `vegify-core` DAL, syncing to the server when online. The local store is a cache; the server is the authority.
- **`crates/vegify-server`**: an Axum service that is the standing source of truth (libSQL on EBS). Owns authentication (self-hosted opaque sessions) and the content API.

vegify began as a framework bake-off (Next.js vs TanStack Start, plus two Rust SSR spikes). TanStack Start on Bun won the web slot; the retired shells live only in history. Full methodology and numbers: [`docs/benchmark.md`](docs/benchmark.md).

## Repo layout

| Path | What |
|------|------|
| `apps/web` | TanStack Start SSR shell (Vite). Stateless; calls the backend over HTTP. |
| `apps/desktop` | Tauri 2 desktop. Local-first; offline CRUD via `vegify-core`, syncs to the server. |
| `crates/vegify-server` | Axum backend, the source of truth. Auth + content API over libSQL. |
| `crates/vegify-core` | Shared Rust DAL, consumed by both the desktop and the server. |
| `packages/db` | Drizzle schema + seed (libSQL/SQLite). Schema stays portable to Postgres. |
| `packages/ui` | Shared components and the one set of screens both clients render. |
| `packages/tokens` | Design tokens → Tailwind v4 `@theme` CSS. |
| `infra` | AWS CDK. AWS only: no Vercel, no Cloudflare. |
| `design/` | Diffable design source: tokens, personas, site map, Figma import reference. |

## Quickstart

Prerequisites: Node 22 + [pnpm](https://pnpm.io), a Rust toolchain (for the desktop and server), and [Bun](https://bun.sh) (the web's production runtime).

```sh
pnpm install
pnpm --filter @vegify/tokens build   # design tokens → theme.css (run once / after token changes)
pnpm db:push                         # create .data/vegify.db from the Drizzle schema
pnpm db:seed                         # sample data (Biga + Neapolitan pizza dough)
```

Run the pieces:

```sh
cargo run -p vegify-server           # the backend (auth + content API)
pnpm dev                             # web (:3001) + desktop, against that backend
```

The web is stateless and reaches the backend at `VEGIFY_API_URL` (default `http://localhost:8787`). Build and serve it on Bun the way production does:

```sh
pnpm --filter web build:aws          # build + assemble the Lambda bundle
pnpm --filter web start:bun          # serve the build on Bun (PORT, VEGIFY_API_URL via env)
```

The desktop bakes its backend URL at build time from `VEGIFY_API_URL`; a runtime `VEGIFY_AUTH_URL` overrides it for local development.

## Database

Local dev uses a SQLite file at `.data/vegify.db`; no daemon. Production is self-hosted libSQL (sqld); set `DATABASE_URL` (and `DATABASE_AUTH_TOKEN` if used), same client. The schema stays portable to Postgres if the project ever outgrows SQLite-class infra.

A recipe carries an `as_ingredient_id` row (name, creator, serving/batch amounts) so recipes can nest: the seed's pizza dough consumes its Biga. `ingredient_nutrient` holds the per-100g values that make micronutrition the core.

## Deploy

Production ships only through CI, AWS-only via CDK: **merging to main is what ships.** The deploy workflow cuts a `vX.Y.Z` tag + GitHub Release in-job (patch per shipping merge; `[skip release]` in the PR title suppresses it; `just release minor|major` for bigger bumps), then runs the ordered cascade — server (path-gated, `/health`-gated) → web ∥ desktop. No release PRs, no bots merging anything; versions are git-tag-derived and no committed version field is authoritative. The tree carries no account-specific values and there are no env files: deploy decisions live in **your account's** SSM Parameter Store (`just init <domains>` records them; `just config` shows them), and everything else is derived (zone lookup, DNS-validated cert), wired cross-stack (backend + ingest origins), or generated in-account (the origin-verify secret). See `docs/self-host.md` for the full path.

GitHub-side config shrinks to one repository **variable** — `AWS_ACCOUNT_ID` (the workflows construct the fixed-name OIDC role ARNs from it) — plus `VEGIFY_APPLE_TEAM_ID` (variable) and the `VEGIFY_PROVISION_PROFILE_B64` secret only if you publish the desktop app.

CDK stacks: `VegifyVpc`, `VegifyServer`, `VegifyWebStart`, `VegifyClientLogs`, `VegifyCi`. With nothing configured at all, `cdk synth` still succeeds against inert placeholders.

## Design

The living design is a private Figma file; binary archives stay out of the repo (`.sketch`/`.fig`/`.psd` are gitignored by policy). What's diffable lives here:

- **Design tokens:** [`design/tokens/`](design/tokens/), W3C DTCG JSON, the seed for the Tailwind theme.
- **Personas:** [`design/personas/`](design/personas/).
- **Site map:** [`design/site-map.md`](design/site-map.md), the app's information architecture.
- **Figma import reference:** [`design/figma-import/`](design/figma-import/), text-override inventory from the Sketch → Figma migration.

## License

vegify.app is **source-available, not open source**. It is released under the [PolyForm Noncommercial License 1.0.0](LICENSE): use, modify, and share it freely for any **noncommercial** purpose; commercial use is reserved to the copyright holder. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for how contributions are licensed back to the project.
