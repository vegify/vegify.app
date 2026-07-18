#!/usr/bin/env node
import { deployConfig } from "@vegify/config/deploy"
// Dedicated app for the CI/OIDC stack (VegifyCi) ONLY — the GitHub Actions deploy
// role and the release-signing role. Instantiating it in isolation (no server/web
// stacks) lets `cdk --app 'tsx bin/ci.ts' deploy VegifyCi` run without building the
// app assets the whole-app synth in bin/vegify.ts otherwise requires.
//
// Stack name, env, and props match bin/vegify.ts exactly, so this updates the SAME
// CloudFormation stack with identical logical IDs (no resource replacement). Used
// by deploy-ci.yml and by a local `VEGIFY_CI_ONLY`-style bootstrap; the app's
// routine cascade still owns VegifyCi through bin/vegify.ts for whole-app synths.
import { App } from "aws-cdk-lib"

import { CiStack } from "../lib/ci-stack.js"

const app = new App()
// Bootstrap context — env + placeholders ONLY, no SSM: this app deploys the very role policy that
// permits reading /vegify/deploy/*, so it must synth without that permission (deploy-ci.yml passes
// APPLE_SIGNING_SECRET_ID from a repository variable instead). See DeployConfigOptions.ssm.
const cfg = await deployConfig({ ssm: false })

new CiStack(app, "VegifyCi", {
  env: { account: cfg.account, region: cfg.region },
  githubRepo: cfg.githubRepo,
  appleSecretId: cfg.appleSecretId,
  masSecretId: cfg.masSecretId
})
