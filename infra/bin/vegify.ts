#!/usr/bin/env node
import { App } from "aws-cdk-lib";
import { VpcStack } from "../lib/vpc-stack.js";
import { WebStartStack } from "../lib/web-start-stack.js";
import { ServerStack } from "../lib/server-stack.js";
import { SyncStack } from "../lib/sync-stack.js";
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

// web-start: a stateless SSR shell — Lambda (Function URL) + CloudFront + S3 — that calls the Axum
// backend over HTTP for all auth + content. No DB, no VPC, scale-to-zero (~$0/mo idle).
new WebStartStack(app, "VegifyWebStart", { env });

// The standing Axum backend (P2): a t3.micro running vegify-server over SQLite-WAL + Litestream→S3,
// fronted by its own CloudFront. Reuses the VPC's public subnet. Dissolves the web's 429 ceiling.
new ServerStack(app, "VegifyServer", { env, vpc: net.vpc });

// Desktop local-first sync: a scale-to-zero S3 changeset-blob store + a least-privilege client.
// Independent of the VPC/web stack. Outputs the bucket + a Secrets Manager creds secret.
new SyncStack(app, "VegifySync", { env });

// Browser-log ingestion: a dedicated scale-to-zero Lambda (Function URL) that writes the web shell's
// client-side logs to a CloudWatch group (/vegify/web-client). Standalone (doesn't touch web-start);
// wire the IngestUrl output into the web build as VITE_CLIENT_LOG_URL. See lib/client-logs-stack.ts.
new ClientLogsStack(app, "VegifyClientLogs", { env });

// CI: the GitHub Actions OIDC deploy role. One-time `cdk deploy VegifyCi`; the workflow assumes it.
new CiStack(app, "VegifyCi", { env, githubRepo: "vegify/vegify.app" });
