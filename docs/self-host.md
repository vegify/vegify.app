# Self-hosting vegify.app

This repo is meant to deploy into **your own** AWS account and domain from a clean clone. Everything account-specific comes from the environment (`.env` locally, GitHub Actions secrets in CI) with inert placeholders in code, so a fresh clone builds and `cdk synth`s without any of vegify's values. This guide is the end-to-end path: clone → configure → deploy → wire DNS + email.

If something here blocks a clean self-host, it's a bug — please open an issue.

## What you get, and the ground rules

- **AWS only, via AWS CDK only.** No Vercel, no Cloudflare. The web shell is OpenNext-style stateless SSR (Lambda + CloudFront + S3); the backend is a small standing Axum service; everything is one CDK app under `infra/`.
- **Low absolute cost / scale-to-zero.** The web + log Lambdas idle at ~$0. The one standing cost is the backend instance (a `t4g.nano`-class box) plus its EBS volume and CloudFront.
- **SQLite-class relational DB.** The backend runs libSQL/SQLite on its EBS volume (WAL), portable back to Postgres if you outgrow it. No managed Postgres required.

## Prerequisites

- An **AWS account** with credentials configured locally (`aws sts get-caller-identity` works), and `cdk bootstrap` run once per account/region.
- A **domain in Route53** (a public hosted zone) in that account.
- Tooling: **Node 20+**, **pnpm 10+**, **Rust** (stable, for the server + desktop), the **AWS CDK CLI** (`npm i -g aws-cdk` or `pnpm dlx cdk`), and the AWS CLI.
- For email: SES in your region (note the **sandbox** caveat below).

## 1. Clone, install, build

```sh
git clone https://github.com/your-org/your-repo vegify-app   # name the dir explicitly (not *.app)
cd vegify-app
pnpm install
pnpm --filter @vegify/tokens build   # design tokens → CSS (once)
pnpm db:push                          # create .data/vegify.db from the Drizzle schema
pnpm db:seed                          # optional sample data
pnpm build                            # turbo build of everything
```

`pnpm dev` runs both app shells locally (web-next :3000, web-start :3001) against the local SQLite file — no AWS needed to develop.

## 2. Configure

Copy `.env.example` to `.env` (or just export the vars) — every variable is documented inline there. The one required deploy input:

- **`VEGIFY_DOMAIN_NAMES`** — your domains (first is primary). Everything else derives: the hosted zone is looked up by that name, the TLS cert is created (DNS-validated) by the web stack, the backend + ingest origins are wired cross-stack, and the server's email config (public URL, From address, SES grant) is derived and injected by the CDK.

Common opt-ins: `VEGIFY_SIGNUPS_OPEN=1` to allow signups, and `VEGIFY_EMAIL_DOMAIN`/`VEGIFY_EMAIL_FROM` if they differ from the derived defaults. The origin-verify secret needs nothing from you — it's generated in your account (SSM SecureString `/vegify/origin-verify`) on first deploy and wired into CloudFront + the Lambdas automatically. Overrides for unusual setups: `VEGIFY_HOSTED_ZONE_ID` (explicit zone), `VEGIFY_CERT_ARN` (bring your own us-east-1 cert), `VEGIFY_API_URL` (point the web at a different backend).

## 3. Bootstrap + the CI deploy role

```sh
cdk bootstrap aws://<account>/<region>
GITHUB_REPOSITORY=your-org/your-repo cdk deploy VegifyCi   # one-time OIDC role for GitHub Actions
```

`VegifyCi` creates the GitHub Actions OIDC role scoped to `GITHUB_REPOSITORY`. Inside Actions that variable is set automatically; for this one-time hand-deploy you pass it explicitly so the trust matches **your** repo, not vegify's.

## 4. Deploy the app stacks

```sh
cdk deploy VegifyVpc VegifyServer      # the standing Axum backend (+ its CloudFront)
cdk deploy VegifyEmail                 # SES sending identity (see §6)
cdk deploy VegifyWebStart VegifyClientLogs   # the web wires itself to the backend cross-stack
```

The web finds the backend through the `VegifyServer` stack's exports (no URL to copy), creates its DNS-validated certificate on first deploy (issuance takes a few minutes while ACM validates through your zone), and writes the apex/`www` CloudFront alias records into the zone it looked up from your domain. Deeper per-stack notes live in `infra/README.md`.

## 5. DNS

`VegifyDns` is **vegify's own production zone** (its records are vegify's specific SES/verification tokens) — you do **not** deploy it. You manage your domain's zone yourself: `VegifyWebStart` creates the apex/`www` aliases, `VegifyEmail` creates the email records, and you add the SPF + DMARC lines below.

## 6. Email (SES)

`cdk deploy VegifyEmail` creates an SES domain identity for your domain with **Easy DKIM** and a **custom MAIL FROM**, and writes the DKIM CNAMEs + MAIL FROM records into your zone. SES then verifies the domain by DNS (async, usually minutes — the deploy returns immediately).

If you manage DNS outside CDK, set `VEGIFY_EMAIL_MANAGE_DNS=0` to create the identity **only** and publish the DKIM/MAIL FROM records yourself (also set `VEGIFY_MAIL_FROM_DOMAIN` to your chosen subdomain). This identity-only mode is how vegify itself runs — its records live in the `VegifyDns` stack, and its existing live identity was adopted into `VegifyEmail` via `cdk import` rather than recreated.

Two records the stack deliberately does **not** manage (because the apex TXT usually carries other records too) — add them to your zone:

- **Apex SPF** (TXT on the apex): `"v=spf1 include:amazonses.com ~all"`
- **DMARC** (TXT at `_dmarc.<domain>`): `"v=DMARC1; p=quarantine; rua=mailto:dmarc@<domain>"`

**SES sandbox (important):** new SES accounts start in the **sandbox** — you can only send to verified addresses, and at a low rate. Request **production access** in the SES console before real users sign up, or verification/reset emails to arbitrary addresses will silently fail.

**Region:** the server sends in `VEGIFY_SES_REGION` (default `us-east-1`). It must match where `VegifyEmail` deployed the identity.

**Inbound mail is not in this repo.** vegify receives mail to its domain through a separate, shared mail-forwarder that lives outside this repo. If you need to *receive* mail (e.g. a `dmarc@` mailbox for DMARC reports, or replies to your From address), set that up yourself — SES receipt rules to S3 + a forwarder Lambda, or any mailbox provider with the right MX. Sending (the app's actual need) does not require it.

## 7. CI/CD (optional, GitHub Actions)

Pushing conventional commits drives release-please, which cuts a release and runs the deploy cascade. To enable it, set these repository **secrets** (the deploy role from §3 is assumed via OIDC — no static AWS keys):

- `AWS_DEPLOY_ROLE_ARN` — the `VegifyCi` role ARN (from its stack output).
- `VEGIFY_DOMAIN_NAMES` — your domain list (drives the zone lookup, the server's email env, and the derived URLs).
- `VEGIFY_API_URL` — only for desktop publishing (baked into the binary at build time); the web derives its backend origin cross-stack.
- `VEGIFY_CERT_ARN` — optional bring-your-own cert; omit it and the web stack manages one.
- `RELEASE_APP_PRIVATE_KEY` (secret) + `RELEASE_APP_CLIENT_ID` (repository **variable**) — a GitHub App that lets release-please open/label its release PRs and lets the merge trigger the deploy cascade. Create a minimal App (Contents + Pull requests read/write), install it on the repo, and store its client id + private key.
- Desktop signing/notarization (only if you publish the desktop app): `AWS_RELEASE_SIGNING_ROLE_ARN`, `APPLE_SIGNING_SECRET_ID`, `VEGIFY_APPLE_TEAM_ID`, `VEGIFY_PROVISION_PROFILE_B64`.

## What's vegify-specific (and safe to ignore)

- **`VegifyDns`** — vegify.app's exact hosted zone + records. Useful as a worked example of adopting a zone into CDK, but it's vegify's data; you manage your own DNS as above.
- The fallback literals in code (e.g. the CI repo defaulting to `vegify/vegify.app` for local hand-deploys) — overridden by your env, never reached in your Actions runs.
