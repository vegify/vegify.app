# vegify.app

## Design

The living design lives in Figma, binary archives live in Dropbox, and this repo holds the diffable design-to-code contract.

- **Figma file:** [Vegify](https://www.figma.com/design/FIGMA_FILE_KEY/Vegify) (file key `FIGMA_FILE_KEY`)
- **Binary archives:** `~/Vegify Dropbox/vegifyapp/brand/` — `.sketch`/`.fig`/`.psd` are gitignored by policy
- **Design tokens:** [`design/tokens/`](design/tokens/) — `color.json` and `typography.json` in W3C DTCG format, extracted from the Sketch shared styles; seed for the eventual Tailwind config
- **Figma import runbook:** [`design/figma-import/PREFLIGHT.md`](design/figma-import/PREFLIGHT.md)
- **Text-override reference:** [`design/figma-import/text-overrides.md`](design/figma-import/text-overrides.md) — the real text content of every symbol instance; the Figma import dropped these overrides, so affected instances show master defaults until fixed
- **Personas:** [`design/personas/`](design/personas/)
- **Site map:** [`design/site-map.md`](design/site-map.md) — mermaid diagram reconstructed from the Sketch `Site Map` page, numbering preserved

## Development

pnpm + Turborepo. Two app shells implement the same recipes slice against shared packages —
a deliberate Next.js vs TanStack Start benchmark; everything except the framework is shared.

```
apps/web-next    Next.js 16 (App Router, RSC)   pnpm --filter web-next dev   → :3000
apps/web-start   TanStack Start (Vite)         pnpm --filter web-start dev  → :3001
apps/web-fast    Rust + Axum + rusqlite (SSR)  cargo run — speed-ceiling spike (read-only)
apps/web-leptos  Rust + Leptos (SSR components) cargo run — React-like DX in Rust (read-only)
packages/db      Drizzle + libSQL/SQLite — schema ported from vegify-laravel
packages/ui      shared components — shadcn on Base UI (cva + Tailwind v4)
packages/tokens  design/tokens → Tailwind v4 @theme (build: pnpm --filter @vegify/tokens build)
```

First run:

```sh
pnpm install
pnpm --filter @vegify/tokens build   # design tokens → theme.css
pnpm db:push                         # create .data/vegify.db from the Drizzle schema
pnpm db:seed                         # Biga + Neapolitan pizza dough sample data
pnpm dev                             # or build: pnpm build
```

Database: local dev uses a SQLite file at `.data/vegify.db` (no daemon). Prod is libSQL —
set `DATABASE_URL` (+ `DATABASE_AUTH_TOKEN`); same client. The schema stays portable to Postgres
if the site ever outgrows SQLite-class infra.

Hosting: **AWS only, via AWS CDK** — web-next through OpenNext (Lambda + CloudFront + S3),
web-start through a small Lambda adapter over its WinterCG fetch handler (Lambda + CloudFront + S3).
Self-hosted libSQL (sqld) on ECS Fargate + EBS. No Vercel, no Cloudflare. See `infra/`.

## Benchmark

The two JS shells render **identical JSX over the shared packages** — the framework is the only
variable. Two Rust SSR spikes (`web-fast`, `web-leptos`) were added to find vegify's speed ceiling on
the read path. Full methodology, raw numbers, and caveats: [`docs/benchmark.md`](docs/benchmark.md).
Measured on an M2 Max, local prod builds, same seeded SQLite db (2026-06-16) — read as ratios and
shape, not absolutes.

**Read throughput** (oha, 50 concurrent; simple = 4-ingredient recipe, complex = 20-ingredient):

| impl | simple req/s · p50 | complex req/s · p50 |
|---|--:|--:|
| web-next (Next 16 / RSC) | 137 · 364 ms | 67 · 755 ms |
| web-start (TanStack Start) | 150 · 223 ms | 74 · 343 ms |
| **web-leptos** (Rust + Leptos) | **1819 · 12 ms** | **1806 · 11 ms** |
| **web-fast** (Rust + Axum) | **1854 · 10 ms** | **1791 · 11 ms** |

Rust serves reads ~12–25× faster and stays flat as recipes get complex — the JS apps' N+1 nutrition
recursion degrades from 4→20 ingredients where the Rust apps run one recursive CTE. The component
model (Leptos) costs nothing vs raw Axum, and web-fast's data-only JSON ≈ its full HTML, so HTML
render is nearly free — the CTE compute (~0.5 ms) is the whole cost.

**Operation latency** (headless Chrome / CDP, median). Client nav, save, and reactivity are
Next-vs-Start only — the Rust spikes are read-only SSR (no client router, no hydration):

| operation | web-next | web-start | web-fast | web-leptos |
|---|--:|--:|--:|--:|
| full page load → detail | 91 ms | 112 ms | **52 ms** | **55 ms** |
| in-app client nav (list → detail) | 21 ms | **3 ms** | — full reload | — full reload |
| recipe save round-trip | 28 ms | 62 ms | n/a | n/a |
| reactive rescale (client re-render) | 1 ms | 1 ms | n/a | n/a |

The throughput win is real **for the read path**, but it doesn't transfer to the whole app: a real
vegify needs the writes and interactivity the SSR-only Rust spikes skip (instant client nav, 1 ms
reactive panels, real saving). Rust owns raw read-serving and cold load; React owns interactivity —
the argument for backporting the recursive CTE into `packages/db` rather than rewriting in Rust.

**Build & bundle:** web-start builds ~3.2× faster (1.4 s vs 4.5 s) with a smaller total bundle;
web-next ships ~half the per-route first-load JS (RSC keeps read pages off the client).
