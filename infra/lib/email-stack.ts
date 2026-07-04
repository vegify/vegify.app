import { CfnDeletionPolicy, CfnOutput, Stack, type StackProps } from "aws-cdk-lib";
import * as ses from "aws-cdk-lib/aws-ses";
import type { Construct } from "constructs";
import { resolveZone } from "./zone.js";

// VegifyEmail — the transactional-email sending identity for the app's domain: an SES domain identity with
// Easy DKIM and a custom MAIL FROM, all DNS-published through the app's Route53 hosted zone. The server
// (crates/vegify-server/src/email.rs) sends password-reset + verification mail through it using the
// instance's AWS credential chain; its SES region is VEGIFY_SES_REGION (default us-east-1) and MUST match
// this stack's region.
//
// Self-host: `cdk deploy VegifyEmail` once for your domain — CDK enables Easy DKIM, writes the three DKIM
// CNAMEs + the MAIL FROM records (MX + SPF TXT) into your zone, and SES verifies the domain by DNS (async,
// a few minutes; the deploy itself returns immediately). Apex SPF + DMARC are deliberately NOT managed here
// (the apex TXT usually carries other records too) — see docs/self-host.md for the two lines to add.
//
// Two modes (manageDns, from VEGIFY_EMAIL_MANAGE_DNS): managed-DNS (default) is the turnkey self-host
// path — CDK writes the records. Identity-only manages just the SES identity and leaves DNS to another
// owner. vegify runs identity-only and ADOPTS its existing live identity via `cdk import` (its DKIM +
// MAIL FROM records already live in VegifyDns), so the adoption rotates no keys and touches no DNS. See
// docs/self-host.md. Values arrive as props from bin/vegify.ts's deployConfig() (@vegify/config/deploy).

export interface EmailStackProps extends StackProps {
  /** Domain for the SES identity (VEGIFY_EMAIL_DOMAIN, defaulting to the first web domain). */
  domain: string;
  /** Whether the domain came from real configuration (gates the zone lookup — see resolveZone). */
  domainConfigured: boolean;
  /** Explicit zone id override; default = lookup by the domain name (managed-DNS mode only). */
  hostedZoneIdOverride?: string;
  /** Custom MAIL FROM subdomain (default mail.<domain>). */
  mailFromDomain: string;
  /** false = identity-only (DNS managed elsewhere — vegify's records live in VegifyDns). */
  manageDns: boolean;
}

export class EmailStack extends Stack {
  constructor(scope: Construct, id: string, props: EmailStackProps) {
    super(scope, id, props);
    const { domain, mailFromDomain, manageDns } = props;

    // Managed-DNS publishes the DKIM CNAMEs + MAIL FROM records through the zone (self-host turnkey).
    // Identity-only just declares the SES identity and leaves DNS to its existing owner (vegify: VegifyDns).
    const identitySource = manageDns
      ? ses.Identity.publicHostedZone(
          resolveZone(this, "Zone", {
            zoneName: domain,
            configured: props.domainConfigured,
            overrideZoneId: props.hostedZoneIdOverride,
          }),
        )
      : ses.Identity.domain(domain);

    // Easy DKIM is on by default; pin the MAIL FROM MX-failure behavior to SES's default so an adopted
    // (cdk import) identity matches live exactly and the post-import diff is empty.
    const identity = new ses.EmailIdentity(this, "Identity", {
      identity: identitySource,
      mailFromDomain,
      mailFromBehaviorOnMxFailure: ses.MailFromBehaviorOnMxFailure.USE_DEFAULT_VALUE,
    });

    // Protect the identity: an accidental stack delete must NOT destroy it (that would rotate Easy DKIM and
    // break sending). RETAIN also matches what `cdk import` sets when adopting an existing live identity.
    const cfnIdentity = identity.node.defaultChild as ses.CfnEmailIdentity;
    cfnIdentity.cfnOptions.deletionPolicy = CfnDeletionPolicy.RETAIN;
    cfnIdentity.cfnOptions.updateReplacePolicy = CfnDeletionPolicy.RETAIN;

    new CfnOutput(this, "EmailIdentityName", { value: identity.emailIdentityName });
    new CfnOutput(this, "EmailDomain", { value: domain });
  }
}
