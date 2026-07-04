# infra — AWS CDK

All vegify hosting is **AWS, defined here in AWS CDK**. No Vercel, no Cloudflare.

## Hosting model: scale-to-zero web, one small standing backend

**Principle: lowest absolute cost.** The web tier is scale-to-zero (~$0/mo idle); the only deliberate standing cost is a single small backend box (~$3/mo). This account predates the AWS 12-month free tier, so infra is priced on *absolute* cost, not free-tier eligibility.

- **web (`apps/web`, TanStack Start) — stateless, scale-to-zero.** The WinterCG SSR handler runs on a **Lambda** (Function URL) behind **CloudFront**; built client assets and the Apple App Site Association serve from **S3**. It holds **no database** — every render fetches auth + content from the standing backend over HTTP (the P4 "web-SSR-calls-Axum" cutover). Lambda + CloudFront + S3 are ~$0/mo idle. Stack: `lib/web-start-stack.ts`.
- **backend (`services/api`, Axum) — the source of truth.** A single **t4g.nano** (ARM/Graviton, ~$3/mo — the cheapest standing box) in the VPC's public subnet, running vegify-server over **SQLite in WAL mode on an EBS volume**, with **Litestream** streaming the WAL to S3 for durability, fronted by **its own CloudFront**. Stack: `lib/server-stack.ts`. This standing box dissolved the web tier's former single-writer 429 ceiling.
- **Bun-on-Fargate is kept ready, not deployed.** It's the measured bake-off winner (Bun: ~+10% read throughput, ~10× tighter p99.9 tail vs Node — `docs/benchmark.md`), but a long-running task + ALB is a standing ~$25–30/mo. It lives complete in `lib/web-start-fargate-stack.ts` for when revenue justifies the tail win.

web-next (Next.js) lost the bake-off and is gone. The **desktop app is local-first** and is not part of this infra — it's a Tauri build that syncs to the backend over the content API (pull/push + a `/ws` push channel), so there's no desktop infra stack.

## Architecture (deployed)

```
        ┌──────────── CloudFront (web) ─────────────┐
browser ►│ default  → web SSR Lambda (Function URL)  │── HTTP: auth + content ──┐
        │ /assets/*, /.well-known/* → S3 (OAC)       │                          │
        │ /__ingest → client-logs Lambda             │                          ▼
        └────────────────────────────────────────────┘             ┌──── CloudFront (api) ────┐
         the web Lambda runs OUTSIDE the VPC (egress only)          │ → t4g.nano Axum backend  │
                                                                    │   SQLite-WAL on EBS      │
 An origin-verify secret header restricts both Function URLs        │   + Litestream → S3      │
 to CloudFront; the backend's origin port is locked to CloudFront's └──────────────────────────┘
 managed prefix list, so neither is directly hittable.               VPC public subnet, no NAT
```

Stacks (`lib/`):
- `VegifyVpc` — one VPC, no NAT, no paid endpoints. The backend uses a public subnet; the web Lambda runs outside the VPC.
- `VegifyWebStart` — the stateless web SSR shell (Lambda + CloudFront + S3).
- `VegifyServer` — the standing t4g.nano Axum backend (EBS + Litestream + its own CloudFront).
- `VegifyClientLogs` — a scale-to-zero Lambda that writes browser logs to CloudWatch `/vegify/web-client` (fronted first-party at `/__ingest` so ad/tracking blockers don't drop the beacon).
- `VegifyCi` — the GitHub Actions OIDC deploy role (one-time `cdk deploy VegifyCi`; the release workflow assumes it, so there are no long-lived AWS keys).
- `web-start-fargate-stack.ts` — the ready-but-unwired Bun-on-Fargate perf path.

Retired 2026-06-25: `VegifySync`, the desktop's old S3 changeset-blob store — the desktop now syncs through the backend's content API instead.

## Deploy

**Deploys run in GitHub Actions, not by hand.** Push to `main` runs build + test (`ci.yml`); a **release tag** (cut by release-please from conventional commits) runs the ordered deploy in `release.yml`: build → deploy `VegifyServer` → poll `/health` until 200 → publish `VegifyWebStart` + `VegifyClientLogs` in parallel with the notarized desktop build. The workflow assumes the `VegifyCi` OIDC role. Don't hand-run `cdk deploy` for the app stacks — push a conventional commit and let the release pipeline deploy.

One-time per account: `pnpm --filter @vegify/infra bootstrap`, then `cdk deploy VegifyCi` to create the OIDC role.

## Prerequisites (local synth / the one-time bootstrap)

- AWS account + credentials (`aws configure` / SSO). Region defaults to `us-east-1`.
- Node 22, pnpm.
- The backend binary cross-compiles to `aarch64-unknown-linux-musl` (the Graviton box) with `cross`; the x86 CI runner can't execute it, so server build + deploy are CI-only.

## Gotchas

- **Function URL exposure → origin-verify secret header.** CloudFront injects an `x-vegify-origin` header (value = the `ORIGIN_VERIFY_SECRET` GitHub Actions secret) on the Function URL origins, and the Lambdas reject any request lacking it — restricting the URLs to CloudFront. This **replaced an OAC attempt**: CloudFront OAC can't SigV4-sign POST bodies to a Lambda Function URL (`InvalidSignatureException`), so OAC broke server-fn POSTs and was reverted. An empty/unset secret = hardening off (fail-open, so a missing secret never breaks the app).
- **Backend origin lock.** The instance's app port is open only to CloudFront's managed origin-facing prefix list (region-pinned, us-east-1) — the box isn't directly reachable.
- **EBS self-attach.** The backend instance finds and attaches its data volume by tag (`vegify:role=data`) in user-data, force-detaching from any prior holder first — so an instance replacement reattaches the same SQLite DB, without a CFN `VolumeAttachment` (which would deadlock the replace on a single volume).
- **AASA content type.** `/.well-known/apple-app-site-association` is served from S3 (binary by default) but rewritten to `application/json` by a CloudFront response-headers policy — Apple's `swcd` requires that type. See `docs/desktop-autofill.md`.
