# infra — AWS CDK

All vegify hosting is **AWS, defined here in AWS CDK**. No Vercel, no Cloudflare.

> **Status: scaffold.** The stacks compile (`pnpm --filter @vegify/infra build`) and the two
> app→AWS build paths are verified locally (OpenNext builds web-next; the Lambda adapter serves
> web-start). It has **not** been `cdk deploy`-ed — that needs your AWS account + the deploy-prep
> steps below, and CDK infra always wants a round of `cdk diff`/iteration before it's real.

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

Stacks (`lib/`): `VegifyVpc` → `VegifyDb` → `VegifyWebNext`, `VegifyWebStart`.

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
