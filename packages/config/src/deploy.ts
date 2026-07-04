// Deploy-time (CDK synth) configuration — every deployment-specific identifier the infra consumes,
// resolved in ONE place and passed to the stacks as typed props (no stack reads process.env). Values
// come from the environment: CI injects them from repository secrets; locally your shell (or nothing).
// The fallbacks are inert placeholders so a fresh clone `cdk synth`s with zero setup and the
// open-source tree carries no account-specific value. See .env.example + docs/self-host.md.
//
// Derivations beat inputs: everything computable from the domain list (public URL, email domain,
// MAIL FROM, the From address) is derived here rather than asked for, and everything knowable from
// the CDK app itself (the backend origin, the ingest Function URL, the hosted zone, the certificate)
// is wired cross-stack / looked up / created in infra — the env vars for those are OVERRIDES now,
// not required inputs.

/** Zone id used on the unconfigured placeholder path (fresh-clone synth never touches AWS). */
export const PLACEHOLDER_ZONE_ID = 'ZEXAMPLE00000000'

export interface DeployConfig {
  /** CDK env — from the ambient credentials (CDK_DEFAULT_ACCOUNT/REGION). */
  account: string | undefined
  region: string
  /** "owner/repo" pinned into the CI OIDC role trust. Auto-set inside GitHub Actions. */
  githubRepo: string
  /** Rotation lever for the in-account generated origin-verify secret: after rotating the SSM parameter
   *  (aws ssm put-parameter --overwrite), bump ORIGIN_VERIFY_ROTATE so the custom resource re-reads it
   *  and the next deploy re-syncs the CloudFront header + Lambda envs. The secret VALUE never passes
   *  through here — it is generated and resolved entirely in-account (see client-logs-stack.ts). */
  originSecretRotationNonce: string
  /** OVERRIDE for the backend origin the web SSR calls. Default (unset) = the VegifyServer stack's own
   *  CloudFront URL, wired cross-stack in bin/vegify.ts. (Desktop CI builds still bake VEGIFY_API_URL
   *  directly at cargo-build time — that consumer doesn't flow through here.) */
  apiUrlOverride: string | undefined
  /** The web shell's domains (first is primary — it drives every derivation below). */
  domainNames: string[]
  /** True when VEGIFY_DOMAIN_NAMES was actually set: real domains may be zone-LOOKED-UP against live
   *  AWS; the placeholder default must never be (fresh-clone `cdk synth` needs no credentials). */
  domainsConfigured: boolean
  /** OVERRIDE for the Route53 hosted zone id. Default (unset) = looked up by the primary domain name. */
  hostedZoneIdOverride: string | undefined
  /** OVERRIDE: bring-your-own us-east-1 ACM cert ARN. Default (unset) = the web stack creates a
   *  DNS-validated certificate for the domains in its own zone. */
  certificateArnOverride: string | undefined
  /** Canonical public origin — derived: https://<first domain>. */
  publicUrl: string
  /** SES identity domain — VEGIFY_EMAIL_DOMAIN, else the first domain. */
  emailDomain: string
  /** True when either VEGIFY_EMAIL_DOMAIN or VEGIFY_DOMAIN_NAMES was set (gates the email zone lookup,
   *  same rule as domainsConfigured). */
  emailConfigured: boolean
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
  const rawDomains = (process.env.VEGIFY_DOMAIN_NAMES ?? '')
    .split(',')
    .map((d) => d.trim())
    .filter(Boolean)
  const domainsConfigured = rawDomains.length > 0
  const domainNames = domainsConfigured ? rawDomains : ['example.com', 'www.example.com']
  const emailDomain = process.env.VEGIFY_EMAIL_DOMAIN ?? domainNames[0]
  return {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region: process.env.CDK_DEFAULT_REGION ?? 'us-east-1',
    githubRepo: process.env.GITHUB_REPOSITORY ?? 'vegify/vegify.app',
    originSecretRotationNonce: process.env.ORIGIN_VERIFY_ROTATE ?? '0',
    apiUrlOverride: process.env.VEGIFY_API_URL || undefined,
    domainNames,
    domainsConfigured,
    hostedZoneIdOverride: process.env.VEGIFY_HOSTED_ZONE_ID || undefined,
    certificateArnOverride: process.env.VEGIFY_CERT_ARN || undefined,
    publicUrl: `https://${domainNames[0]}`,
    emailDomain,
    emailConfigured: Boolean(process.env.VEGIFY_EMAIL_DOMAIN) || domainsConfigured,
    emailFrom: process.env.VEGIFY_EMAIL_FROM ?? `Vegify <hello@${emailDomain}>`,
    mailFromDomain: process.env.VEGIFY_MAIL_FROM_DOMAIN ?? `mail.${emailDomain}`,
    manageEmailDns: (process.env.VEGIFY_EMAIL_MANAGE_DNS ?? '1') !== '0',
    appleSecretId: process.env.APPLE_SIGNING_SECRET_ID ?? 'your-org/apple-signing',
  }
}
