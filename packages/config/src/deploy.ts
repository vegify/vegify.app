// Deploy-time (CDK synth) configuration — every deployment-specific identifier the infra consumes,
// resolved in ONE place and passed to the stacks as typed props (no stack reads process.env).
//
// Resolution order, per value: environment variable (explicit override / CI escape hatch) → SSM
// Parameter Store decision (/vegify/deploy/*, written once by `just init` — the account itself is
// the config store) → derivation/placeholder. The placeholders keep a fresh clone `cdk synth`ing
// with zero setup and no credentials: the SSM read fails soft (no creds / no params → {}), and the
// open-source tree carries no account-specific value.
//
// Derivations beat inputs: everything computable from the domain list (public URL, email domain,
// MAIL FROM, the From address) is derived rather than asked for, and everything knowable from the
// CDK app itself (backend origin, ingest URL, hosted zone, certificate, origin-verify secret) is
// wired cross-stack / looked up / created in infra.

import { GetParametersByPathCommand, SSMClient } from "@aws-sdk/client-ssm";

/** Parameter Store home of the deploy decisions (`just init` / `just config-set` write here). */
export const DEPLOY_PARAM_PATH = "/vegify/deploy/";

/** Zone id used on the unconfigured placeholder path (fresh-clone synth never touches AWS). */
export const PLACEHOLDER_ZONE_ID = "ZEXAMPLE00000000";

async function ssmDecisions(region: string): Promise<Record<string, string>> {
  try {
    const ssm = new SSMClient({ region });
    const out: Record<string, string> = {};
    let token: string | undefined;
    do {
      const page = await ssm.send(
        new GetParametersByPathCommand({
          Path: DEPLOY_PARAM_PATH,
          WithDecryption: true,
          NextToken: token,
        }),
      );
      for (const p of page.Parameters ?? []) {
        if (p.Name && p.Value !== undefined)
          out[p.Name.slice(DEPLOY_PARAM_PATH.length)] = p.Value;
      }
      token = page.NextToken;
    } while (token);
    return out;
  } catch (e) {
    // In CI a failed read is NEVER "unconfigured" — it's a broken grant or region, and continuing
    // would synth placeholders into a real deploy (v0.18.0 shipped example.com email config to prod
    // exactly this way). Fail the synth loudly instead.
    if (process.env.GITHUB_ACTIONS) {
      throw new Error(
        `cannot read the ${DEPLOY_PARAM_PATH} decisions in region ${region} — fix the deploy role's ssm:GetParametersByPath grant or the parameter region; refusing to synth with placeholders in CI (${e})`,
      );
    }
    // Locally: no credentials / no parameters is the legitimate fresh-clone, zero-config path.
    return {};
  }
}

export interface DeployConfig {
  /** CDK env — from the ambient credentials (CDK_DEFAULT_ACCOUNT/REGION). */
  account: string | undefined;
  region: string;
  /** "owner/repo" pinned into the CI OIDC role trust. Auto-set inside GitHub Actions. */
  githubRepo: string;
  /** Rotation lever for the in-account generated origin-verify secret: after rotating the SSM parameter
   *  (aws ssm put-parameter --overwrite), bump ORIGIN_VERIFY_ROTATE so the custom resource re-reads it
   *  and the next deploy re-syncs the CloudFront header + Lambda envs. The secret VALUE never passes
   *  through here — it is generated and resolved entirely in-account (see client-logs-stack.ts). */
  originSecretRotationNonce: string;
  /** OVERRIDE for the backend origin the web SSR calls. Default (unset) = the VegifyServer stack's own
   *  CloudFront URL, wired cross-stack in bin/vegify.ts. (Desktop CI builds bake VEGIFY_API_URL at
   *  cargo-build time, resolved from the /vegify/deploy/api-url parameter the server stack writes.) */
  apiUrlOverride: string | undefined;
  /** The web shell's domains (first is primary — it drives every derivation below).
   *  Env VEGIFY_DOMAIN_NAMES → SSM domain-names → placeholder. */
  domainNames: string[];
  /** True when the domains came from real configuration (env or SSM): real domains may be
   *  zone-LOOKED-UP against live AWS; the placeholder default never is. */
  domainsConfigured: boolean;
  /** OVERRIDE for the Route53 hosted zone id. Default (unset) = looked up by the primary domain name. */
  hostedZoneIdOverride: string | undefined;
  /** OVERRIDE: bring-your-own us-east-1 ACM cert ARN (env VEGIFY_CERT_ARN → SSM cert-arn). Default
   *  (unset) = the web stack creates a DNS-validated certificate for the domains in its own zone. */
  certificateArnOverride: string | undefined;
  /** Canonical public origin — derived: https://<first domain>. */
  publicUrl: string;
  /** Signups gate the server enforces (env VEGIFY_SIGNUPS_OPEN → SSM signups-open → closed). Lands in
   *  the instance's systemd env; flip with `just config-set signups-open 1` + a release. */
  signupsOpen: boolean;
  /** Admin email allowlist (VEGIFY_ADMIN_EMAILS → SSM admin-emails). Accounts allowed to invite new
   *  users while signups stay closed. Set with `just config-set admin-emails 'you@domain'` + a release. */
  adminEmails: string;
  /** SES identity domain — env VEGIFY_EMAIL_DOMAIN → SSM email-domain → the first domain. */
  emailDomain: string;
  /** True when either the email domain or the domain list was really configured (gates the email
   *  stack's zone lookup, same rule as domainsConfigured). */
  emailConfigured: boolean;
  /** From: header for transactional mail — env VEGIFY_EMAIL_FROM → SSM email-from → derived. */
  emailFrom: string;
  /** Custom MAIL FROM subdomain — env VEGIFY_MAIL_FROM_DOMAIN → SSM mail-from-domain → mail.<domain>. */
  mailFromDomain: string;
  /** VEGIFY_EMAIL_MANAGE_DNS: "0" = identity-only (DNS managed elsewhere); default managed. */
  manageEmailDns: boolean;
  /** Secrets Manager id of the shared Apple signing secret (desktop notarization) —
   *  env APPLE_SIGNING_SECRET_ID → SSM apple-secret-id → placeholder. */
  appleSecretId: string;
  /** Where CloudWatch alarm notifications are emailed — env VEGIFY_ALARM_EMAIL → SSM alarm-email →
   *  derived hello@<email domain> (the mailbox the domain forwarder already catches). */
  alarmEmail: string;
}

export interface DeployConfigOptions {
  /** false = resolve from env + placeholders ONLY, never touching SSM. This exists for the VegifyCi
   *  bootstrap (bin/ci.ts): the deploy role's permission to read /vegify/deploy/* is itself granted
   *  BY VegifyCi, so the stack that creates the grant must be able to synth without it — otherwise
   *  a broken/missing grant can never be redeployed (the v0.18.1 chicken-and-egg). */
  ssm?: boolean;
}

export async function deployConfig(
  opts: DeployConfigOptions = {},
): Promise<DeployConfig> {
  const region = process.env.CDK_DEFAULT_REGION ?? "us-east-1";
  const ssm = opts.ssm === false ? {} : await ssmDecisions(region);
  /** env override → SSM decision → undefined. Empty strings count as unset. */
  const pick = (envKey: string, ssmKey: string): string | undefined =>
    process.env[envKey] || ssm[ssmKey] || undefined;

  const rawDomains = (pick("VEGIFY_DOMAIN_NAMES", "domain-names") ?? "")
    .split(",")
    .map((d) => d.trim())
    .filter(Boolean);
  const domainsConfigured = rawDomains.length > 0;
  const domainNames = domainsConfigured
    ? rawDomains
    : ["example.com", "www.example.com"];
  const primaryDomain = domainNames[0];
  if (!primaryDomain) throw new Error("domain-names decision is empty");
  const emailDomain =
    pick("VEGIFY_EMAIL_DOMAIN", "email-domain") ?? primaryDomain;

  return {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region,
    githubRepo: process.env.GITHUB_REPOSITORY ?? "vegify/vegify.app",
    originSecretRotationNonce: process.env.ORIGIN_VERIFY_ROTATE ?? "0",
    apiUrlOverride: process.env.VEGIFY_API_URL || undefined,
    domainNames,
    domainsConfigured,
    hostedZoneIdOverride: process.env.VEGIFY_HOSTED_ZONE_ID || undefined,
    certificateArnOverride: pick("VEGIFY_CERT_ARN", "cert-arn"),
    publicUrl: `https://${primaryDomain}`,
    signupsOpen: (pick("VEGIFY_SIGNUPS_OPEN", "signups-open") ?? "0") === "1",
    adminEmails: pick("VEGIFY_ADMIN_EMAILS", "admin-emails") ?? "",
    emailDomain,
    emailConfigured:
      Boolean(pick("VEGIFY_EMAIL_DOMAIN", "email-domain")) || domainsConfigured,
    emailFrom:
      pick("VEGIFY_EMAIL_FROM", "email-from") ??
      `Vegify <hello@${emailDomain}>`,
    mailFromDomain:
      pick("VEGIFY_MAIL_FROM_DOMAIN", "mail-from-domain") ??
      `mail.${emailDomain}`,
    manageEmailDns: (process.env.VEGIFY_EMAIL_MANAGE_DNS ?? "1") !== "0",
    appleSecretId:
      pick("APPLE_SIGNING_SECRET_ID", "apple-secret-id") ??
      "your-org/apple-signing",
    alarmEmail:
      pick("VEGIFY_ALARM_EMAIL", "alarm-email") ?? `hello@${emailDomain}`,
  };
}
