// Deploy-time (CDK synth) configuration — every deployment-specific identifier the infra consumes,
// resolved in ONE place and passed to the stacks as typed props (no stack reads process.env). Values
// come from the environment: CI injects them from repository secrets; locally your shell (or nothing).
// The fallbacks are inert placeholders so a fresh clone `cdk synth`s with zero setup and the
// open-source tree carries no account-specific value. See .env.example + docs/self-host.md.
//
// Derivations beat inputs: everything computable from the domain list (public URL, email domain,
// MAIL FROM, the From address) is derived here rather than asked for, so a real deploy configures the
// minimum and a fork can't end up with another site's values.

export interface DeployConfig {
  /** CDK env — from the ambient credentials (CDK_DEFAULT_ACCOUNT/REGION). */
  account: string | undefined
  region: string
  /** "owner/repo" pinned into the CI OIDC role trust. Auto-set inside GitHub Actions. */
  githubRepo: string
  /** Origin-verify secret CloudFront injects + the web/ingest Lambdas require. Empty = hardening off
   *  at synth (no header injected); the web Lambda itself fails closed when deployed without it. */
  originSecret: string
  /** The standing Axum backend's public origin, baked into the web Lambda's env. */
  apiUrl: string
  /** The browser-log Lambda's Function URL host (VegifyClientLogs) for the /__ingest forward. */
  ingestOrigin: string
  /** The web shell's domains; the first is primary and drives every derivation below. */
  domainNames: string[]
  /** Route53 hosted zone for the domains. */
  hostedZoneId: string
  /** us-east-1 ACM cert for the domains (CloudFront requirement). */
  certificateArn: string
  /** Canonical public origin — derived: https://<first domain>. */
  publicUrl: string
  /** SES identity domain — VEGIFY_EMAIL_DOMAIN, else the first domain. */
  emailDomain: string
  /** From: header for transactional mail — VEGIFY_EMAIL_FROM, else derived from the email domain. */
  emailFrom: string
  /** Custom MAIL FROM subdomain — VEGIFY_MAIL_FROM_DOMAIN, else mail.<email domain>. */
  mailFromDomain: string
  /** VEGIFY_EMAIL_MANAGE_DNS: "0" = identity-only (DNS managed elsewhere); default managed. */
  manageEmailDns: boolean
  /** Secrets Manager id of the shared Apple signing secret (desktop notarization). */
  appleSecretId: string
}

export function deployConfig(): DeployConfig {
  const domainNames = (process.env.VEGIFY_DOMAIN_NAMES ?? 'example.com,www.example.com')
    .split(',')
    .map((d) => d.trim())
    .filter(Boolean)
  const emailDomain = process.env.VEGIFY_EMAIL_DOMAIN ?? domainNames[0]
  return {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region: process.env.CDK_DEFAULT_REGION ?? 'us-east-1',
    githubRepo: process.env.GITHUB_REPOSITORY ?? 'vegify/vegify.app',
    originSecret: process.env.ORIGIN_VERIFY_SECRET ?? '',
    apiUrl: process.env.VEGIFY_API_URL ?? 'https://api.example.com',
    ingestOrigin: process.env.VEGIFY_INGEST_ORIGIN ?? 'ingest.example.com',
    domainNames,
    hostedZoneId: process.env.VEGIFY_HOSTED_ZONE_ID ?? 'ZEXAMPLE00000000',
    certificateArn:
      process.env.VEGIFY_CERT_ARN ??
      'arn:aws:acm:us-east-1:123456789012:certificate/00000000-0000-0000-0000-000000000000',
    publicUrl: `https://${domainNames[0]}`,
    emailDomain,
    emailFrom: process.env.VEGIFY_EMAIL_FROM ?? `Vegify <hello@${emailDomain}>`,
    mailFromDomain: process.env.VEGIFY_MAIL_FROM_DOMAIN ?? `mail.${emailDomain}`,
    manageEmailDns: (process.env.VEGIFY_EMAIL_MANAGE_DNS ?? '1') !== '0',
    appleSecretId: process.env.APPLE_SIGNING_SECRET_ID ?? 'your-org/apple-signing',
  }
}
