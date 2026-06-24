# infra — AWS CDK

All vegify hosting is **AWS, defined here in AWS CDK**. No Vercel, no Cloudflare.

## Hosting decision (2026-06-23): free tier first, Fargate when there's revenue

**Principle: a zero-revenue app deploys to the free tier / scale-to-zero. No standing costs.** This supersedes the 2026-06-18 "Bun on Fargate" decision below — that choice buys a real but unneeded performance win at a standing monthly cost, which is the wrong trade until the app earns.

**web-start ships on the free tier:** TanStack Start's WinterCG handler on a scale-to-zero **Lambda** (Function URL) behind **CloudFront**, with built client assets on **S3** and the libSQL DB as a **file on EFS**. At zero/low traffic this is ~$0/mo — Lambda + CloudFront + S3 sit in the always-free tier, and EFS is 5 GB free for 12 months (a sub-1 GB DB ≈ pennies after). The stack is `lib/web-start-stack.ts`.

**Why EFS for the DB.** A Lambda is stateless, so the SQLite file needs to live somewhere persistent and writable. The options weighed:
- **Read-only (baked SQLite in the Lambda)** — truly $0, but web writes don't persist. **Rejected:** the web shell must be writable, not a read showcase.
- **Turso free tier (managed libSQL)** — writable + free, but a managed third-party dependency and not self-hosted. **Rejected:** not desirable.
- **EFS + Lambda** — writable, persists, free-forever, keeps the libSQL/Drizzle stack (no rewrite). **Chosen.** Two EFS gotchas are handled in code: the Lambda is pinned to `reservedConcurrentExecutions: 1` (single writer — SQLite over NFS is lock-finicky), and the DB runs in **rollback-journal (DELETE) mode, not WAL** (WAL needs shared memory NFS can't provide). The seed DB is baked in DELETE mode (`apps/web-start/aws/assemble-bundle.mjs`) and copied onto EFS on first cold start (`apps/web-start/aws/lambda-handler.mjs`).
- **Free-tier EC2 micro** (libSQL in-process on local disk) was the runner-up — more robust writes, but a VM to manage and free only for 12 months. EFS+Lambda wins on free-forever + no-VM.

**The Bun-on-Fargate path is kept ready, not deployed.** It's the measured bake-off winner (Bun: ~+10% read throughput, ~10× tighter p99.9 tail vs Node — `docs/benchmark.md`), but a long-running task + ALB is a standing ~$25–30/mo. It lives complete in `lib/web-start-fargate-stack.ts` (+ `apps/web-start/Dockerfile`, `docker-entrypoint.sh`). **To flip when revenue justifies the tail win:** swap `WebStartStack` for `WebStartFargateStack` in `bin/vegify.ts`, `pnpm --filter web-start build:aws`, `cdk deploy VegifyVpc VegifyWebStart`. That path uses option (ii) "collapse" — libSQL embedded in-process on a mounted EBS volume (WAL works on EBS), one task, no separate DB service.

**web-next is dropped** (Next.js lost the bake-off) and the separate **sqld-on-Fargate `DbStack` is gone** (the DB is now the Lambda's EFS file / the Fargate task's EBS file). The old OpenNext/sqld scaffold notes below are retained only for historical context.

## Architecture (deployed)

```
            ┌──────────────── CloudFront ────────────────┐
 browser ─► │  default → web-start Lambda (Function URL)  │
            │  /assets/* → S3 (OAC, cached)               │
            └─────────────────────────────────────────────┘
                          │  (Lambda in VPC, private-isolated subnets, reserved concurrency 1)
                          ▼
              EFS access point /vegify  →  file:/mnt/data/vegify.db  (libSQL, rollback journal)
 No NAT. No paid interface endpoints. EFS reached via in-subnet mount targets.

 Desktop sync (separate, independent): S3 changeset bucket + least-privilege IAM client (VegifySync).
```

Stacks (`lib/`): `VegifyVpc` (VPC, no NAT/endpoints) → `VegifyWebStart` (Lambda + EFS + CloudFront + S3). `VegifySync` (desktop local-first S3 changeset store) is independent. `web-start-fargate-stack.ts` is the ready-but-unwired performance path.

## Prerequisites

- AWS account + credentials (`aws configure` / SSO). Region defaults to `us-east-1`.
- Node 22, pnpm. **Docker** (the CDK bundles `@libsql/client` for the Lambda arch in a container).
- One-time per account/region: `pnpm --filter @vegify/infra bootstrap`.

## Deploy runbook (free-tier web)

```sh
pnpm --filter web-start build:aws     # → apps/web-start/dist/ + apps/web-start/.aws-lambda/
                                      #   (.aws-lambda = handler + server + package.json + DELETE-mode seed)
cd infra && pnpm install
pnpm synth                            # sanity-check (also runs the Docker bundling)
cdk deploy VegifyVpc VegifyWebStart   # ~10 min (EFS mount targets + CloudFront)
```

The CloudFront URL is the `VegifyWebStart.Url` output.

## Gotchas / TODO

- **Native `@libsql/client`.** Kept external from the SSR bundle (`vite ssr.external`); the CDK installs the matching binding at deploy via Docker bundling (x86_64 Lambda → x86 binding). `HOME=/tmp` in the bundling command so npm's cache is writable (the container runs as the host uid).
- **SQLite on EFS.** Reserved concurrency 1 + rollback journal (above). This caps write throughput at one-at-a-time — fine at low traffic; heavy write concurrency is the signal to switch to the Fargate/EBS path.
- **Function URL exposure.** `authType: NONE` so CloudFront can reach it — also directly reachable. Harden with **OAC** (`FunctionUrlOrigin.withOriginAccessControl` + IAM) before any real launch.
- **EFS removal policy.** Currently `DESTROY` (dev teardown re-seeds from the baked copy). Set to `RETAIN` before storing data you can't lose.
- **Cost.** No NAT (~$32/mo saved), no interface endpoints (~$7/mo each saved). The web stack is ~$0/mo idle. The desktop `VegifySync` bucket is scale-to-zero (~$0 + ~$0.40/mo for its Secrets Manager secret).

---

### Historical: 2026-06-18 scaffold notes (superseded)

The original scaffold targeted OpenNext (web-next) + a TanStack Start Lambda adapter, both reaching a self-hosted `sqld` on Fargate+EBS over the VPC, with Secrets/Logs interface endpoints. That shape is retired (see the hosting decision above): web-next is gone, the DB is no longer a separate service, and the paid endpoints were trimmed. The Bun-on-Fargate reasoning from that date survives in `web-start-fargate-stack.ts`.
