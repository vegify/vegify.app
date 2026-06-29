import { CfnDeletionPolicy, CfnOutput, Stack, type StackProps } from "aws-cdk-lib";
import * as route53 from "aws-cdk-lib/aws-route53";
import * as ses from "aws-cdk-lib/aws-ses";
import type { Construct } from "constructs";

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
// Two modes (VEGIFY_EMAIL_MANAGE_DNS): managed-DNS (default "1") is the turnkey self-host path — CDK writes
// the records. Identity-only ("0") manages just the SES identity and leaves DNS to another owner. vegify
// runs identity-only and ADOPTS its existing live identity via `cdk import` (its DKIM + MAIL FROM records
// already live in VegifyDns), so the adoption rotates no keys and touches no DNS. See docs/self-host.md.

const DOMAIN =
  process.env.VEGIFY_EMAIL_DOMAIN ??
  process.env.VEGIFY_DOMAIN_NAMES?.split(",")[0]?.trim() ??
  "example.com";
const ZONE_ID = process.env.VEGIFY_HOSTED_ZONE_ID ?? "ZEXAMPLE00000000";
const MAIL_FROM_DOMAIN = process.env.VEGIFY_MAIL_FROM_DOMAIN ?? `mail.${DOMAIN}`;
const MANAGE_DNS = (process.env.VEGIFY_EMAIL_MANAGE_DNS ?? "1") !== "0";

export class EmailStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    // Managed-DNS publishes the DKIM CNAMEs + MAIL FROM records through the zone (self-host turnkey).
    // Identity-only just declares the SES identity and leaves DNS to its existing owner (vegify: VegifyDns).
    const identitySource = MANAGE_DNS
      ? ses.Identity.publicHostedZone(
          route53.HostedZone.fromHostedZoneAttributes(this, "Zone", { hostedZoneId: ZONE_ID, zoneName: DOMAIN }),
        )
      : ses.Identity.domain(DOMAIN);

    // Easy DKIM is on by default; pin the MAIL FROM MX-failure behavior to SES's default so an adopted
    // (cdk import) identity matches live exactly and the post-import diff is empty.
    const identity = new ses.EmailIdentity(this, "Identity", {
      identity: identitySource,
      mailFromDomain: MAIL_FROM_DOMAIN,
      mailFromBehaviorOnMxFailure: ses.MailFromBehaviorOnMxFailure.USE_DEFAULT_VALUE,
    });

    // Protect the identity: an accidental stack delete must NOT destroy it (that would rotate Easy DKIM and
    // break sending). RETAIN also matches what `cdk import` sets when adopting an existing live identity.
    const cfnIdentity = identity.node.defaultChild as ses.CfnEmailIdentity;
    cfnIdentity.cfnOptions.deletionPolicy = CfnDeletionPolicy.RETAIN;
    cfnIdentity.cfnOptions.updateReplacePolicy = CfnDeletionPolicy.RETAIN;

    new CfnOutput(this, "EmailIdentityName", { value: identity.emailIdentityName });
    new CfnOutput(this, "EmailDomain", { value: DOMAIN });
  }
}
