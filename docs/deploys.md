# Deploys and zero-downtime

## Current model

The backend is a single `t4g.nano` EC2 instance (the locked lowest-cost standing compute). It runs `vegify-server` (Axum) under systemd, supervised by litestream, with the libSQL database on a dedicated GP3 EBS volume at `/data/vegify.db`. A stable Elastic IP fronts it; the web (CloudFront + SSR Lambda) and the desktop app call it over HTTPS.

The database is **stateful and co-located**: it lives on a single EBS volume tagged `vegify:role=data`. On a server change, `infra/lib/server-stack.ts` sets `userDataCausesReplacement: true`, so CloudFormation **replaces the instance**, and the new instance's user-data force-detaches the data volume from the old instance and re-attaches it (a serialized handoff). litestream streams every write to S3 for backup and first-boot restore.

## The deploy-window 504 (2026-06-28)

The web SSR calls the backend on **every page** (the auth gate in `apps/web/src/routes/__root.tsx`). During an instance replacement the backend is down for the launch + volume-handoff window (minutes), so that gate call times out and the whole site showed the error boundary. It was made worse by the release pipeline **redeploying the server on every release**, even web/desktop-only ones (the robots.txt fix that triggered the incident did not touch the server at all).

## Hardening (chosen: the lighter combo, option 1)

Zero-downtime for the common case *without* breaking the single-nano / no-ALB / self-hosted-DB locks:

1. **Graceful degradation (web). DONE** (commit `c1e962b`). A backend failure (anything but a clean 401) renders the public pages (landing, auth forms) anonymously instead of erroring; only gated pages fall through to the retry boundary. The landing is now immune to a backend blip.
2. **Pipeline path-filter. TODO.** Skip `build-server` + `deploy-server` when no server-relevant files changed (`crates/**`, `infra/lib/server-stack.ts`, the Cargo manifests). Most releases are web/desktop-only and would then never touch the backend, so there is no blip at all.
3. **In-place server restart. TODO (optional).** For genuine server changes, swap the binary and `systemctl restart` (~1s, the DB volume stays attached) via SSM, instead of replacing the instance (minutes). Requires decoupling the binary from the user-data text so a binary change no longer forces a CloudFormation replacement. Path-filter + graceful degradation already cover ~all practical cases, so this is the lowest-priority piece.

## Future: true blue-green (option 2, deferred but specced)

Textbook zero-downtime even during a server-binary change. Deferred because it collides with the locked infra; this is the spec for when it is worth it.

**The blocker is the stateful DB.** The libSQL database is on a single-attach EBS volume, so blue and green cannot both serve from it. Real blue-green therefore requires one of:

- **Externalize the DB** — run libSQL as its own service (a standing sqld primary, or managed Turso) that both app instances connect to. The app instances become stateless and blue-green is then trivial. Cost: a standing DB dependency (compute or a SaaS bill) on top of the app instances, which breaks the single-nano model.
- **libSQL primary + embedded replicas** — sqld supports a primary with embedded read-replicas; app instances read a local replica and write through to the primary, which allows more than one app instance. A bigger re-architecture, and the primary is still a single writer to coordinate on cutover.
- **litestream-restore cutover** — green restores the DB from the litestream S3 replica, health-checks, then a coordinated cutover pauses writes on blue, does a final litestream sync, and flips. Near-zero read downtime and no second standing cost, but a brief write-pause and operationally delicate (the sync must not lose the replication-lag window).

**Traffic flipping is already cheap:** the Elastic IP can be re-associated from blue to green in seconds, so no ALB is needed (a standing ALB was explicitly rejected). The EIP is the flip mechanism.

**When to revisit:** real revenue, an uptime SLA, or traffic where a ~1-2 minute blip during a genuine server deploy actually costs something. Until then, option 1 keeps the user-visible impact at zero for the common case.
