# Self-hosting vegify.app

This repo is meant to deploy into **your own** AWS account and domain from a clean clone. There are no env files and no account-specific values in the tree: the deploy decisions live in **your account's** SSM Parameter Store (written once by `just init`), everything else is derived, wired cross-stack, or generated in-account, and inert placeholders keep a fresh clone building and `cdk synth`ing with zero setup. This guide is the end-to-end path: clone → configure → deploy → wire DNS + email.

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

There is no `.env` file. Your AWS account is the config store: record the deploy decisions once, in SSM Parameter Store under `/vegify/deploy/`, and every synth (local or CI) reads them from there.

```sh
just init example.com,www.example.com    # the ONE required decision (first domain is primary)
just config-set signups-open 1           # optional: open signups (default closed)
just config                              # show what's recorded
```

Everything else derives: the hosted zone is looked up by the domain name, the TLS cert is created (DNS-validated) by the web stack, the backend + ingest origins are wired cross-stack, the server's email config (public URL, From address, SES grant) is derived and injected by the CDK, and the origin-verify secret is generated in your account (SSM SecureString `/vegify/origin-verify`) on first deploy.

Optional decisions (`just config-set <key> <value>`): `email-domain` / `email-from` / `mail-from-domain` if the derived defaults don't fit, `cert-arn` to bring your own us-east-1 cert, `apple-secret-id` for desktop signing. Environment variables (`VEGIFY_DOMAIN_NAMES`, `VEGIFY_CERT_ARN`, `VEGIFY_HOSTED_ZONE_ID`, `VEGIFY_API_URL`, …) override any decision per-synth — every knob is documented where it's defined, in `packages/config/src/deploy.ts` (deploy), `packages/config/src/runtime.ts` (JS runtime), and `crates/vegify-config` (Rust runtime).

## 3. Bootstrap + the CI deploy role

```sh
cdk bootstrap aws://<account>/<region>
GITHUB_REPOSITORY=your-org/your-repo cdk deploy VegifyCi   # one-time OIDC role for GitHub Actions
```

`VegifyCi` creates the GitHub Actions OIDC role scoped to `GITHUB_REPOSITORY`. Inside Actions that variable is set automatically; for this one-time hand-deploy you pass it explicitly so the trust matches **your** repo, not vegify's.

## 4. Deploy the app stacks

```sh
cdk deploy VegifyVpc VegifyServer      # the standing Axum backend (+ its CloudFront; with domains configured it also gets api.<your-domain> — cert, DNS records, and the published api-url all derive automatically)
cdk deploy VegifyEmail                 # SES sending identity (see §6)
cdk deploy VegifyWebStart VegifyClientLogs   # the web wires itself to the backend cross-stack
```

**The USDA plant catalog (optional but recommended):** reference data lives in the server's S3 Data bucket, not the repo. Download the FDC datasets (Foundation + SR Legacy JSON from fdc.nal.usda.gov/download-datasets into `.data/import/usda/`), then `just usda-data && just usda-upload` — the server ingests it at its next boot (any server deploy). Skipping this just means an empty communal catalog.

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

Merging to main ships: the deploy workflow cuts a patch release in-job (no release PRs, no bots on main) and runs the ordered cascade. `[skip release]` in a PR title ships without a version; `just release minor|major` dispatches a bigger bump. To enable CI deploys, set the following (the deploy role from §3 is assumed via OIDC — no static AWS keys):

Repository **variables** (none are sensitive):

- `AWS_ACCOUNT_ID` — your 12-digit account id; the workflows construct the two fixed-name role ARNs (`vegify-github-deploy`, `vegify-release-signing`) from it.
- `AWS_REGION` — your deploy region if not `us-east-1`.
- `APPLE_SIGNING_SECRET_ID` — only for the one-time `VegifyCi` bootstrap via deploy-ci (the release flow reads the SSM decision instead).
- `VEGIFY_APPLE_TEAM_ID` — only for desktop publishing; public by construction (it's served in the AASA).

Repository **secrets**:

- `VEGIFY_PROVISION_PROFILE_B64` — only for desktop publishing (the embedded provisioning profile). If you skip the desktop app, CI needs **zero secrets**.

Everything else CI needs comes from your account at run time: the domain list and Apple secret id from the `/vegify/deploy/*` decisions, the backend origin from the parameter `VegifyServer` publishes, and the origin-verify secret generated in-account.

## What's vegify-specific (and safe to ignore)

- **`VegifyDns`** — vegify.app's exact hosted zone + records. Useful as a worked example of adopting a zone into CDK, but it's vegify's data; you manage your own DNS as above.
- The fallback literals in code (e.g. the CI repo defaulting to `vegify/vegify.app` for local hand-deploys) — overridden by your env/decisions, never reached in your Actions runs.
