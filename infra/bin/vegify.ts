#!/usr/bin/env node
import { App } from "aws-cdk-lib";
import { VpcStack } from "../lib/vpc-stack.js";
import { DbStack } from "../lib/db-stack.js";
import { WebNextStack } from "../lib/web-next-stack.js";
import { WebStartStack } from "../lib/web-start-stack.js";

const app = new App();
const env = {
  account: process.env.CDK_DEFAULT_ACCOUNT,
  region: process.env.CDK_DEFAULT_REGION ?? "us-east-1",
};

// Shared network: one VPC, no NAT (cost). Public subnets give Fargate egress to pull
// its image; the app Lambdas live in private-isolated subnets and reach AWS services
// via gateway/interface endpoints.
const net = new VpcStack(app, "VegifyVpc", { env });

// Self-hosted libSQL (sqld) on Fargate + EBS, reachable in-VPC at db.internalUrl.
const db = new DbStack(app, "VegifyDb", { env, vpc: net.vpc });

// Both apps: Lambda + CloudFront + S3, in the VPC, talking to sqld.
new WebNextStack(app, "VegifyWebNext", {
  env,
  vpc: net.vpc,
  dbClientSecurityGroup: db.clientSecurityGroup,
  databaseUrl: db.internalUrl,
});
new WebStartStack(app, "VegifyWebStart", {
  env,
  vpc: net.vpc,
  dbClientSecurityGroup: db.clientSecurityGroup,
  databaseUrl: db.internalUrl,
});
