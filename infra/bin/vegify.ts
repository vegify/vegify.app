#!/usr/bin/env node
import { App } from "aws-cdk-lib";
import { VpcStack } from "../lib/vpc-stack.js";
import { WebStartStack } from "../lib/web-start-stack.js";
import { ServerStack } from "../lib/server-stack.js";
import { ClientLogsStack } from "../lib/client-logs-stack.js";
import { CiStack } from "../lib/ci-stack.js";

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

// One VPC, no NAT (cost). The standing Axum backend (VegifyServer) uses its public subnet; the web
// Lambda runs OUTSIDE the VPC (it just needs internet egress to call that backend's public CloudFront).
const net = new VpcStack(app, "VegifyVpc", { env });

// Browser-log ingestion: a dedicated scale-to-zero Lambda (Function URL) that writes the web shell's
// client-side logs to a CloudWatch group (/vegify/web-client). The browser beacons a same-origin
// /__ingest on the web distribution, which forwards here; web-start imports this Function URL to
// OAC-lock it (AWS_IAM, CloudFront-only). Instantiated BEFORE web-start so it can consume the FunctionUrl.
const clientLogs = new ClientLogsStack(app, "VegifyClientLogs", { env });

// web-start: a stateless SSR shell — Lambda (Function URL) + CloudFront + S3 — that calls the Axum
// backend over HTTP for all auth + content. No DB, no VPC, scale-to-zero (~$0/mo idle). Its CloudFront
// /__ingest behavior forwards to — and OAC-locks — the ClientLogs Function URL.
new WebStartStack(app, "VegifyWebStart", { env, ingestFnUrl: clientLogs.ingestFnUrl });

// The standing Axum backend (P2): a t3.micro running vegify-server over SQLite-WAL + Litestream→S3,
// fronted by its own CloudFront. Reuses the VPC's public subnet. Dissolves the web's 429 ceiling.
new ServerStack(app, "VegifyServer", { env, vpc: net.vpc });

// (Retired 2026-06-25) The old VegifySync stack — an S3 changeset-blob store for the desktop's former
// S3-mesh sync — is gone: the desktop now syncs through the standing Axum backend (content pull/push),
// so the S3BlobStore path was removed (sync engine step 8). Its deployed CloudFormation stack is torn
// down out-of-band; the changeset bucket was RETAIN, so it's deleted separately if/when wanted.

// CI: the GitHub Actions OIDC deploy role. One-time `cdk deploy VegifyCi`; the workflow assumes it.
new CiStack(app, "VegifyCi", { env, githubRepo: "vegify/vegify.app" });
