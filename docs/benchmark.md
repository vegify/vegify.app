# Benchmark: Next.js vs TanStack Start

vegify deliberately ships **two app shells over identical shared packages** (`@vegify/db`,
`@vegify/ui`, `@vegify/tokens`). Both render the same JSX through the same components — the framework
is the only variable. This doc records how they compare on a real slice (the recipes + ingredients
CRUD with the computed Nutrition Facts panel).

## Environment

- Apple M2 Max, 12 cores · macOS · Node 24.16
- Next.js **16.2.9** (Turbopack, App Router, RSC) · React 19.2.4
- TanStack Start **1.168.25** (Vite **8.0.16**, file-based router) · React 19.2.4
- Turborepo 2.9.18 · pnpm workspace
- Measured 2026-06-16, local only (no network latency). Same machine, same seeded SQLite db.

## Results at a glance

| Metric | web-next (Next 16) | web-start (TanStack Start) | Winner |
|---|---:|---:|:--|
| Cold build (wall) | **4.5 s** | **1.4 s** | start (3.2×) |
| Warm build (wall) | 4.5 s | 1.4 s | start |
| Total client JS emitted | 939 KB / **281 KB** gz | 456 KB / **145 KB** gz | start |
| First-load JS per route (uncompressed) | **~248 KB** | ~449 KB | next |
| First-load JS (gzip-equiv estimate) | ~75 KB | ~145 KB | next |
| SSR HTML for `/recipes` | 36.5 KB | 30.3 KB | start |
| TTFB `/recipes` (local prod, median) | **4.2 ms** | 6.8 ms* | next |
| Dev server boot | 243 ms | 416 ms | next |
| Dev launch → first page rendered | 2.7 s | **1.5 s** | start |

\* web-start prod is a WinterCG fetch handler served here via a small node `http`→`fetch` bridge
(`tools`-less, see Methodology); it adds a little overhead vs a native runtime, so treat its TTFB as
an upper bound.

## The interesting nuance (total bundle vs first-load)

The two flip depending on what you measure:

- **web-next** emits a *larger total* bundle (939 KB) but a *smaller per-route first-load* (~248 KB,
  and essentially flat across `/`, `/recipes`, `/recipes/new`). React Server Components keep route
  content off the client; what ships is mostly the framework runtime + the one client boundary in the
  layout (`AppShell`). Read-heavy pages cost almost nothing extra on the client.
- **web-start** emits a *smaller total* bundle (456 KB) but each route pulls *~all of it* (~449 KB).
  It hydrates the whole app SPA-style, so the first content page downloads nearly the entire client
  bundle regardless of route.

For vegify — content-heavy, read-mostly recipe/ingredient pages — Next's lower per-page JS is the
better story for mobile LCP. TanStack Start's build speed and smaller total are the better story for
DX and simplicity.

## Methodology

**Builds** — deps prebuilt; `/usr/bin/time -p pnpm --filter <app> build` (direct, no Turbo
orchestration). Cold = after deleting `.next` / `dist`+`.vite`. Warm = immediately again.

**Client JS** — sum of all `*.js` under the client output dir (`.next/static` / `dist/client`), raw
and gzipped (`gzip -c | wc -c`).

**First-load JS** — Chrome DevTools Protocol `Network.loadingFinished.encodedDataLength` summed over
`Script` responses for a single navigation, fresh profile per run (`/tmp/network-bench.mjs`). Values
are **uncompressed** (neither local server applies gzip/brotli); a real CDN would cut these ~⅔.
Caveat: `next start` sets immutable cache headers on `/_next/static`, so reusing a Chrome profile
zeroes its JS on the second hit — always use a cold profile.

**TTFB** — `curl -w %{time_starttransfer}` ×8, median, after warming. Local compute only; no network.

**Prod servers** — web-next: `next start`. web-start: the built `dist/server/server.js` fetch handler
wrapped in a node `http.createServer` bridge (`/tmp/start-prod.mjs`) that also serves `dist/client/*`
statically (a CDN's job in real prod). Both serve JS uncompressed.

**Dev startup** — launch → first successful `GET /` (includes the first on-demand route compile);
plus the framework's self-reported "ready" time.

## Caveats / honesty

- Small app, sparse seed data — absolute numbers are tiny; treat as *ratios and shape*, not gospel.
- Local, no network — TTFB reflects server compute only; real-world is dominated by latency + CDN.
- web-start's prod numbers come through a bridge, not its real target (**AWS Lambda** via a small
  adapter over its WinterCG handler + CloudFront — all deploys are AWS/CDK). The local bridge that
  produced these numbers is essentially that adapter; re-measure against the real deploy in Item 4.
- First-load JS is uncompressed; gzip-equiv estimates use each app's measured total-bundle ratio
  (next 0.30, start 0.32).
- Revisit as the app grows: web-next's per-route JS rises as client components are added; web-start's
  gap could shrink with route-level code-splitting/lazy loading.

## Raw numbers

```
build (wall, /usr/bin/time -p):   web-next cold 4.52s / warm 4.50s   web-start cold 1.39s / warm 1.36s
client JS emitted:                web-next 939 KB raw, 281 KB gz, 13 files
                                  web-start 456 KB raw, 145 KB gz, 12 files
first-load JS (uncompressed, CDP, cold profile):
  /            web-next 248 KB (8 reqs)   web-start 449 KB (5 reqs)
  /recipes     web-next 248 KB (8 reqs)   web-start 450 KB (5 reqs)
  /recipes/new web-next 248 KB (8 reqs)   web-start 449 KB (5 reqs)
SSR HTML /recipes:                web-next 36.5 KB   web-start 30.3 KB
TTFB /recipes (median of 8):      web-next 4.2 ms    web-start 6.8 ms (via bridge)
dev boot / first page:            web-next 243 ms / 2698 ms   web-start 416 ms / 1474 ms
```

## Throughput across all four implementations (oha, 2026-06-16)

Two more implementations were added to find vegify's speed ceiling (TechEmpower R23: Rust tops the
DB-backed tests, Node/Next near the bottom): **web-leptos** (Rust + Leptos, React-like components,
SSR) and **web-fast** (Rust + Axum + rusqlite, templated). Both read the same `.data/vegify.db` and
compute nutrition with **one recursive CTE** (vs the JS apps' N+1 recursion). Load: `oha --no-tui
-z 5s -c 50` (via `127.0.0.1` — `localhost` resolves to IPv6 first and the servers bind IPv4) on a
simple recipe (4 ingredients, nested biga) and a complex one (20 ingredients, full panel). 100% success.

| impl | simple req/s | simple p50 | complex req/s | complex p50 |
|---|--:|--:|--:|--:|
| web-next (Next 16 / RSC) | 139 | 352 ms | 66 | 769 ms |
| web-start (TanStack Start) | 146 | 228 ms | 73 | 342 ms |
| **web-leptos** (Rust + Leptos SSR) | **1742** | **14 ms** | **1523** | **12 ms** |
| **web-fast** (Rust + Axum) | **1703** | **12 ms** | **1472** | **11 ms** |

- **Rust ≈ 12–25× the JS shells' throughput**, ~25–60× lower p50.
- **Leptos ≈ raw Rust** — the React-like component model (`#[component]` + `view!`) has no meaningful
  perf cost. "Good DX" and "fastest" coexist in Rust.
- **Complexity exposes the algorithm:** web-next 139→66 req/s (4→20 ingredients) because nutrition is
  N+1 (~40 round-trips); the Rust apps stay ~flat (1742→1523) — the single recursive CTE costs about
  the same for 20 ingredients as for 4. **Query pattern matters as much as language** — backporting
  the CTE to `packages/db` would materially speed the JS apps (likely narrowing the gap to ~5–10×).

Caveats: local macOS, oha shares the CPU, single run — indicative not authoritative. JS apps use N+1
+ libSQL client; Rust apps use the CTE + direct rusqlite (gap is part-algorithm, part-runtime).
`force-dynamic` (no caching) is the fair "compute per request" setting for the JS apps.

### IDE / tooling for the Rust + Leptos path
rust-analyzer (the LSP all major IDEs use) expands the `view!` proc-macro → type errors, hover,
go-to-def, and typed-prop autocomplete work inside it; `clippy` lints the macro code cleanly;
`leptosfmt` formats inside `view!` (rustfmt can't). Thinner than TS/JSX for HTML-attribute
autocomplete and occasionally cryptic macro errors — but fully lintable/navigable, not "unlintable."

## Reproduce

```sh
pnpm install && pnpm --filter @vegify/tokens build && pnpm db:push && pnpm db:seed
# builds
rm -rf apps/web-next/.next && /usr/bin/time -p pnpm --filter web-next build
rm -rf apps/web-start/dist node_modules/.vite && /usr/bin/time -p pnpm --filter web-start build
# bundle sizes
find apps/web-next/.next/static -name '*.js' -print0 | xargs -0 cat | gzip -c | wc -c
find apps/web-start/dist/client -name '*.js' -print0 | xargs -0 cat | gzip -c | wc -c
# prod TTFB / first-load JS: start both servers on high ports (not 3000/3001 — other ~/coding apps),
#   web-next: PORT=39000 pnpm --filter web-next start
#   web-start: DATABASE_URL="file:$(pwd)/.data/vegify.db" node <bridge> apps/web-start/dist/server/server.js 39002
#   then curl -w '%{time_starttransfer}' and a CDP Network capture per route.
```
