#!/usr/bin/env node
import { App } from "aws-cdk-lib";
import { VpcStack } from "../lib/vpc-stack.js";
import { WebStartStack } from "../lib/web-start-stack.js";
import { ServerStack } from "../lib/server-stack.js";
import { ClientLogsStack } from "../lib/client-logs-stack.js";
import { CiStack } from "../lib/ci-stack.js";
import { DnsStack } from "../lib/dns-stack.js";
import { EmailStack } from "../lib/email-stack.js";

// Hosting decision (see infra/README "Hosting decision"): the bake-off winner is TanStack Start
// (web-start); web-next is dropped. As of P4 (web-SSR-calls-Axum) the web is a STATELESS SSR shell —
// Lambda + CloudFront + S3 — that fetches all auth + content from the standing Axum backend
// (VegifyServer) over HTTP; it holds no database. The Bun-on-Fargate path (the perf winner, but a
// standing cost) is kept ready in lib/web-start-fargate-stack.ts for when revenue justifies it.

const app = new App();
const env = {
  account: process.env.CDK_DEFAULT_ACCOUNT,
  region: process.env.CDK_DEFAULT_REGION ?? "us-east-1",
};

// Origin-verify secret (defense-in-depth, replaces the reverted OAC): CloudFront injects it as a custom
// header on the Function URL origins and the Lambdas reject any request lacking it — restricting the URLs
// to CloudFront-only WITHOUT OAC (OAC can't sign POST bodies to a Lambda URL → InvalidSignatureException,
// which is why the OAC attempt was reverted). Both stacks read the SAME value here, so the one synth
// process bakes an identical literal into each — no cross-stack reference, no cycle. Empty = hardening
// OFF (fail-open: never breaks the app). In CI it's the ORIGIN_VERIFY_SECRET GitHub Actions secret;
// locally it's usually unset (the built bundle still serves standalone).
const originSecret = process.env.ORIGIN_VERIFY_SECRET ?? "";

// One VPC, no NAT (cost). The standing Axum backend (VegifyServer) uses its public subnet; the web
// Lambda runs OUTSIDE the VPC (it just needs internet egress to call that backend's public CloudFront).
const net = new VpcStack(app, "VegifyVpc", { env });

// web-start: a stateless SSR shell — Lambda (Function URL) + CloudFront + S3 — that calls the Axum
// backend over HTTP for all auth + content. No DB, no VPC, scale-to-zero (~$0/mo idle).
new WebStartStack(app, "VegifyWebStart", { env, originSecret });

// The standing Axum backend (P2): a t4g.nano running vegify-server over SQLite-WAL + Litestream→S3,
// fronted by its own CloudFront. Reuses the VPC's public subnet. Dissolves the web's 429 ceiling.
new ServerStack(app, "VegifyServer", { env, vpc: net.vpc });

// (Retired 2026-06-25) The old VegifySync stack — an S3 changeset-blob store for the desktop's former
// S3-mesh sync — is gone: the desktop now syncs through the standing Axum backend (content pull/push),
// so the S3BlobStore path was removed (sync engine step 8). Its deployed CloudFormation stack is torn
// down out-of-band; the changeset bucket was RETAIN, so it's deleted separately if/when wanted.

// Browser-log ingestion: a dedicated scale-to-zero Lambda (Function URL) that writes the web shell's
// client-side logs to a CloudWatch group (/vegify/web-client). Standalone (doesn't touch web-start);
// wire the IngestUrl output into the web build as VITE_CLIENT_LOG_URL. See lib/client-logs-stack.ts.
new ClientLogsStack(app, "VegifyClientLogs", { env, originSecret });

// CI: the GitHub Actions OIDC deploy role. One-time `cdk deploy VegifyCi`; the workflow assumes it. The
// repo for the OIDC trust comes from GITHUB_REPOSITORY (auto-set in Actions; falls back to vegify's only
// for local hand-deploys), so a fork's CI works without editing this file.
new CiStack(app, "VegifyCi", { env, githubRepo: process.env.GITHUB_REPOSITORY ?? "vegify/vegify.app" });

// DNS: vegify.app's hosted zone + records, adopted (cdk import) from johncarmack1984/my-infra-private's
// Terraform so the domain is owned in this repo. Standing stack — deploy on demand, NOT in the cascade.
new DnsStack(app, "VegifyDns", { env });

// Email: the SES sending identity (Easy DKIM + custom MAIL FROM) for the app's domain, DNS-published
// through its zone. Generic + parameterized for self-host; deploy on demand (`cdk deploy VegifyEmail`),
// NOT in the cascade. vegify.app's own identity is still in Terraform — see docs/self-host.md for the
// gated cutover.
new EmailStack(app, "VegifyEmail", { env });
