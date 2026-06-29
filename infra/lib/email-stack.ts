import { CfnOutput, Stack, type StackProps } from "aws-cdk-lib";
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
// vegify.app's own identity still lives in Terraform (my-infra-private); adopting it into this stack is a
// separate, gated cutover, because re-creating an SES identity can rotate Easy DKIM and Easy DKIM is the
// deliverability-sensitive part. See docs/self-host.md.

const DOMAIN =
  process.env.VEGIFY_EMAIL_DOMAIN ??
  process.env.VEGIFY_DOMAIN_NAMES?.split(",")[0]?.trim() ??
  "example.com";
const ZONE_ID = process.env.VEGIFY_HOSTED_ZONE_ID ?? "ZEXAMPLE00000000";
const MAIL_FROM_DOMAIN = process.env.VEGIFY_MAIL_FROM_DOMAIN ?? `mail.${DOMAIN}`;

export class EmailStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    const zone = route53.HostedZone.fromHostedZoneAttributes(this, "Zone", {
      hostedZoneId: ZONE_ID,
      zoneName: DOMAIN,
    });

    // Over a public hosted zone, EmailIdentity enables Easy DKIM and writes the DKIM CNAMEs AND the custom
    // MAIL FROM records into the zone, so one deploy stands up a domain SES can verify and the server can
    // send from. DKIM signing is on by default.
    const identity = new ses.EmailIdentity(this, "Identity", {
      identity: ses.Identity.publicHostedZone(zone),
      mailFromDomain: MAIL_FROM_DOMAIN,
    });

    new CfnOutput(this, "EmailIdentityName", { value: identity.emailIdentityName });
    new CfnOutput(this, "EmailDomain", { value: DOMAIN });
  }
}
