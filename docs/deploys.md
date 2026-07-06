# Deploys and zero-downtime

## Trigger + release model (2026-07-04 — the stormdeck cutover)

**Merging to main is what ships.** The single `deploy.yml` workflow (`concurrency: deploy`, queued not cancelled) first cuts the release in-job — a `vX.Y.Z` tag (patch by default; `just release minor|major` dispatches bigger bumps) pushed with `GITHUB_TOKEN` so it triggers nothing, plus a GitHub Release whose notes are generated from PR titles (`.github/release.yml`) — then runs the ordered cascade: build gate (server + web + desktop + iOS, all-or-nothing) → `deploy-server` (path-gated, `/health`-gated) → `publish-web` (path-gated) ∥ `publish-desktop` (only when a release was cut; the signed `.dmg` attaches to it, with the bundle version injected from the tag via `tauri.version.json`) ∥ `publish-ios` (uploads the staged `.ipa` to TestFlight — deferred to the publish rank because TestFlight has no draft state). `[skip release]` in the merged PR's title ships server/web without cutting a version. Versions are git-tag-derived; every committed version field is a vestigial `0.0.0`.
 
This replaced the release-please → auto-merge train (2026-06-26 → 2026-07-04): the train's bot-authored release PR sat in every deploy's critical path and produced a steady tax of automation side-effects (auto-merge races, token migrations, stacked-PR closures, workflow-vs-workflow races). The stormdeck model keeps everything the train proved — the ordered cascade, the health gate, path filters, signed desktop artifacts — and removes every bot decision: nothing lands on main but human merges, and a release is a side-effect of shipping, not a PR a bot must land.

## Current model

The backend is a single `t4g.nano` EC2 instance (the locked lowest-cost standing compute). It runs `vegify-server` (Axum) under systemd, supervised by litestream, with the libSQL database on a dedicated GP3 EBS volume at `/data/vegify.db`. A stable Elastic IP fronts it; the web (CloudFront + SSR Lambda) and the desktop app call it over HTTPS.

The database is **stateful and co-located**: it lives on a single EBS volume tagged `vegify:role=data`. On a server change, `infra/lib/server-stack.ts` sets `userDataCausesReplacement: true`, so CloudFormation **replaces the instance**, and the new instance's user-data force-detaches the data volume from the old instance and re-attaches it (a serialized handoff). litestream streams every write to S3 for backup and first-boot restore.

## The deploy-window 504 (2026-06-28)

The web SSR calls the backend on **every page** (the auth gate in `apps/web/src/routes/__root.tsx`). During an instance replacement the backend is down for the launch + volume-handoff window (minutes), so that gate call times out and the whole site showed the error boundary. It was made worse by the release pipeline **redeploying the server on every release**, even web/desktop-only ones (the robots.txt fix that triggered the incident did not touch the server at all).

## Hardening (chosen: the lighter combo, option 1)

Zero-downtime for the common case *without* breaking the single-nano / no-ALB / self-hosted-DB locks:

1. **Graceful degradation (web). DONE** (commit `c1e962b`). A backend failure (anything but a clean 401) renders the public pages (landing, auth forms) anonymously instead of erroring; only gated pages fall through to the retry boundary. The landing is now immune to a backend blip.
2. **Pipeline path-filter. DONE** (and carried into the 2026-07-04 trigger model below). `deploy-server` is skipped when no server-relevant files changed (`crates/**`, `packages/db/**`, `packages/config/**`, the server/VPC stacks + app wiring, the Cargo manifests) — web/desktop-only pushes never touch the backend, so there is no blip at all. The builds still run as the all-or-nothing gate.
3. **In-place server restart. TODO (optional).** For genuine server changes, swap the binary and `systemctl restart` (~1s, the DB volume stays attached) via SSM, instead of replacing the instance (minutes). Requires decoupling the binary from the user-data text so a binary change no longer forces a CloudFormation replacement. Path-filter + graceful degradation already cover ~all practical cases, so this is the lowest-priority piece.

## Future: true blue-green (option 2, deferred but specced)

Textbook zero-downtime even during a server-binary change. Deferred because it collides with the locked infra; this is the spec for when it is worth it.

**The blocker is the stateful DB.** The libSQL database is on a single-attach EBS volume, so blue and green cannot both serve from it. Real blue-green therefore requires one of:

- **Externalize the DB** — run libSQL as its own service (a standing sqld primary, or managed Turso) that both app instances connect to. The app instances become stateless and blue-green is then trivial. Cost: a standing DB dependency (compute or a SaaS bill) on top of the app instances, which breaks the single-nano model.
- **libSQL primary + embedded replicas** — sqld supports a primary with embedded read-replicas; app instances read a local replica and write through to the primary, which allows more than one app instance. A bigger re-architecture, and the primary is still a single writer to coordinate on cutover.
- **litestream-restore cutover** — green restores the DB from the litestream S3 replica, health-checks, then a coordinated cutover pauses writes on blue, does a final litestream sync, and flips. Near-zero read downtime and no second standing cost, but a brief write-pause and operationally delicate (the sync must not lose the replication-lag window).

**Traffic flipping is already cheap:** the Elastic IP can be re-associated from blue to green in seconds, so no ALB is needed (a standing ALB was explicitly rejected). The EIP is the flip mechanism.

**When to revisit:** real revenue, an uptime SLA, or traffic where a ~1-2 minute blip during a genuine server deploy actually costs something. Until then, option 1 keeps the user-visible impact at zero for the common case.

## Observability (2026-07-06)

Logs, metrics, dashboards, and email alarms — added pre-public. The design keeps the deploy cascade uncoupled: every alarm + dashboard lives in the stack that owns the resource it watches (so it refreshes on each deploy and never points at a replaced instance), and the one shared resource — the SNS alarm topic — is created by `VegifyServer` and discovered elsewhere by ARN through SSM (`/vegify/monitor/alarm-topic-arn`) at deploy time, a value lookup rather than a CDK export that could wedge a redeploy.

**Logs → CloudWatch, with retention.** The server's stdout/stderr go to `/var/log/vegify/server.log` (logrotate-capped) and the on-box CloudWatch agent ships them to the `/vegify/server` group; the agent also publishes `mem_used_percent` + `disk_used_percent` (EC2 emits neither), rolled up to `{InstanceId, path}` so alarms need not guess per-boot device/fstype dimensions. The agent steps are tolerant (`|| true`) — telemetry must never block the server or the `/health` deploy gate. Every Lambda (web SSR, ingest, origin-secret) now has an explicit log group with retention (the implicit `/aws/lambda/*` groups never expire).

**Alarms → email.** An SNS topic emails `VEGIFY_ALARM_EMAIL` → SSM `alarm-email` → derived `hello@<domain>`. The email subscription lands **PendingConfirmation**: AWS sends a one-time confirm link that must be clicked once before anything delivers. Alarms: EC2 status check, t4g CPU-credit floor, memory, root + `/data` disk, API CloudFront 5xx, server ERROR log lines (`VegifyServer`); web CloudFront 5xx, SSR Lambda errors + throttles (`VegifyWebStart`); ingest Lambda errors (`VegifyClientLogs`). All within the always-free alarm tier.

**Dashboards.** `Vegify-Server` (EC2 CPU/credits/mem/disk, API requests + error rates, server ERROR lines) and `Vegify-Web` (site requests + error rates, SSR errors/throttles/invocations/duration).

**Gotcha baked in:** CloudFront metrics need BOTH `DistributionId` AND `Region=Global` dimensions; CDK's `Distribution.metricXxx()` helpers omit `Region`, so their metrics silently read no data. `monitoring.ts`'s `cloudFrontMetric()` builds them correctly. CloudFront metrics live in us-east-1 — fine here (everything is us-east-1); a non-us-east-1 self-host would need the CloudFront alarms in us-east-1.
