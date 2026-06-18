# infra — AWS CDK

All vegify hosting is **AWS, defined here in AWS CDK**. No Vercel, no Cloudflare.

> **Status: scaffold.** The stacks compile (`pnpm --filter @vegify/infra build`) and the two
> app→AWS build paths are verified locally (OpenNext builds web-next; the Lambda adapter serves
> web-start). It has **not** been `cdk deploy`-ed — that needs your AWS account + the deploy-prep
> steps below, and CDK infra always wants a round of `cdk diff`/iteration before it's real.

## Decision (2026-06-18): runtime + hosting shape

The framework bake-off resolved (see `docs/benchmark.md`): **TanStack Start (React) on the Bun runtime is the winner**; Next.js is eliminated, and the Rust spikes (web-fast/web-leptos) stay as perf references, not deploy targets. This supersedes parts of the scaffold below:

- **web-start → a long-running Bun server on Fargate/ECS (NOT Lambda).** Bun's measured win (~+10% throughput, ~10× tighter p99.9 tail) is a *sustained-concurrency-on-one-process* property; Lambda isolates requests one-per-instance, so it cannot capture that tail win — and Lambda has no first-class Bun runtime anyway (it would mean a container image plus cold starts). A long-running Bun task realizes the win and fits the VPC where the DB already lives. Entry point: `apps/web-start/serve-bun.mjs` (`Bun.serve` over the WinterCG build), containerized from the `oven/bun` image; `@libsql/client` resolves natively (verified).
- **`VegifyWebNext` is dead** — Next.js was eliminated; drop the stack (and OpenNext) when the CDK is reworked.
- **DB shape, two options:** (i) keep the separate `sqld` Fargate task and have web-start reach it in-VPC (one in-cluster hop), or (ii) **collapse** — run libSQL embedded in-process inside the web-start Fargate task against a mounted EBS volume (no separate DB service, no socket hop). (ii) is fastest + simplest + cheapest (one task, not two; mirrors the proven web-fast in-process model) and single-writer is fine at this scale; (i) is the lower-risk increment from today's scaffold. **Target (ii).**

**Implementation status:** decided + documented; the CDK below still reflects the old Lambda plan (`WebStartStack` packages `aws/lambda-handler.mjs`). Reworking `WebStartStack` into a Bun Fargate service (and folding in EBS for option ii) is the next infra task — gated on the same AWS creds as any deploy.

## Architecture

```
                         ┌─────────────── CloudFront ───────────────┐
  browser ──► CloudFront │  default → app Lambda (Function URL)      │
                         │  /_next/static, /assets → S3 (OAC)        │
                         │  /_next/image → image Lambda (web-next)   │
                         └───────────────────────────────────────────┘
                                        │ (app Lambdas in VPC, private-isolated subnets)
                                        ▼
                         libSQL (sqld) on ECS Fargate + EBS
                         discoverable at http://sqld.vegify.internal:8080
                         (public subnet for image-pull egress; SG locks 8080 to app Lambdas)
  No NAT gateway. S3 via gateway endpoint; Secrets/Logs via interface endpoints.
```

Stacks (`lib/`): `VegifyVpc` → `VegifyDb` → `VegifyWebNext`, `VegifyWebStart`. (Per the 2026-06-18 decision above, `VegifyWebNext` is to be dropped and `VegifyWebStart` reworked from Lambda to a long-running Bun Fargate service — the diagram below still shows the old Lambda shape.)

## Prerequisites

- AWS account + credentials (`aws configure` / SSO). Pick a region (default `us-east-1`).
- Node 22, pnpm. Docker (for building the web-start Lambda bundle on the Lambda arch — see below).
- One-time per account/region: `pnpm --filter @vegify/infra bootstrap`.

## Deploy runbook

```sh
# 1. Build the app AWS artifacts
pnpm --filter web-next build:aws          # → apps/web-next/.open-next/
pnpm --filter web-start build:aws         # → apps/web-start/dist/  (then assemble the bundle, below)

# 2. Assemble the web-start Lambda bundle → apps/web-start/.aws-lambda/
#    { handler.mjs (from aws/lambda-handler.mjs), server/ (from dist/server),
#      node_modules with @libsql/client built for the Lambda arch }
#    Do this on the target arch — easiest is Docker bundling (see "Native modules").

# 3. Deploy
cd infra
pnpm install
pnpm synth                                # sanity-check the CloudFormation
pnpm deploy                               # cdk deploy --all
```

## Gotchas / TODO before this is production-real

- **Native modules (`@libsql/client`).** Both app Lambdas import it; its native binary must match
  the Lambda arch (we target `arm64`). Building the bundle on macOS yields a darwin binary that
  won't run on Lambda. Fix at deploy: build inside a Lambda-arch container (CDK `Code.fromAsset(...,
  { bundling: { image: lambda.Runtime.NODEJS_22_X.bundlingImage } })`) or install the linux-arm64
  prebuilt. OpenNext traces this for web-next; the web-start bundle assembly must handle it too.
- **Function URL exposure.** Function URLs are `authType: NONE` so CloudFront can reach them — that
  also makes them publicly reachable directly. Harden with **OAC** (`FunctionUrlOrigin.withOriginAccessControl`
  + IAM auth) before prod.
- **sqld auth.** Today access is network-only (SG limits port 8080 to the app Lambdas). Add JWT auth
  (`SQLD_AUTH_JWT_KEY`) + a Secrets Manager `DATABASE_AUTH_TOKEN` (wire into Lambda env / runtime
  fetch) for defense-in-depth. Pin the `libsql-server` image to a digest.
- **Single writer.** sqld is one Fargate task (SQLite is single-writer) — `desiredCount: 1`,
  `maxHealthyPercent: 100`. Don't scale it horizontally. Back up the EBS volume (snapshots).
- **Cost.** Designed to be cheap: no NAT (~$32/mo saved). Standing cost ≈ 1 small Fargate task +
  EBS + a couple of interface endpoints (~$7/mo each — trim unused). Lambdas + CloudFront + S3 scale
  to ~zero. Re-measure `docs/benchmark.md` against the real deploy once live.
- **CloudFront behaviors (web-next).** The scaffold wires the common ones; reconcile the full set
  against `apps/web-next/.open-next/open-next.output.json`.
```
