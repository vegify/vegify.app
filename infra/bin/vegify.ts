#!/usr/bin/env node
import { App } from "aws-cdk-lib";
import { VpcStack } from "../lib/vpc-stack.js";
import { WebStartStack } from "../lib/web-start-stack.js";
import { SyncStack } from "../lib/sync-stack.js";

// Hosting decision (see infra/README "Hosting decision"): the bake-off winner is TanStack Start
// (web-start); web-next is dropped. web-start ships on the FREE TIER — Lambda + CloudFront + S3 with
// libSQL on EFS — so there's no standing cost. The separate sqld-on-Fargate DbStack is gone too
// (the DB is now the Lambda's EFS file). The Bun-on-Fargate path (the perf winner, but a standing
// cost) is kept ready in lib/web-start-fargate-stack.ts for when revenue justifies it.

const app = new App();
const env = {
  account: process.env.CDK_DEFAULT_ACCOUNT,
  region: process.env.CDK_DEFAULT_REGION ?? "us-east-1",
};

// One VPC, no NAT (cost). The web Lambda + its EFS mount targets live in private-isolated subnets.
const net = new VpcStack(app, "VegifyVpc", { env });

// web-start, free tier: Lambda (Function URL) + CloudFront + S3, libSQL on EFS. ~$0/mo idle.
new WebStartStack(app, "VegifyWebStart", { env, vpc: net.vpc });

// Desktop local-first sync: a scale-to-zero S3 changeset-blob store + a least-privilege client.
// Independent of the VPC/web stack. Outputs the bucket + a Secrets Manager creds secret.
new SyncStack(app, "VegifySync", { env });
