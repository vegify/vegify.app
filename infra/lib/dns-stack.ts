import {
  CfnDeletionPolicy,
  CfnOutput,
  Stack,
  type StackProps,
} from "aws-cdk-lib";
import * as acm from "aws-cdk-lib/aws-certificatemanager";
import * as route53 from "aws-cdk-lib/aws-route53";
import type { Construct } from "constructs";

// VegifyDns — vegify.app's hosted zone + records, ADOPTED from the prior Terraform management in
// johncarmack1984/my-infra-private (dns/) via `cdk import`. The values below are captured from the LIVE
// Route53 zone, NOT copied from the Terraform — the Terraform had drifted (its apex TXT held two separate
// `v=spf1` records, which is invalid; live was hand-merged to one). Because we import in place, every
// value here MUST equal live so `cdk diff` is empty (a non-empty diff would MODIFY live DNS on deploy).
//
// Deliberately NOT here:
//   - apex + www A/AAAA (CloudFront aliases) — owned by VegifyWebStart, resolve this zone by id.
//   - the zone's NS + SOA — owned implicitly by the hosted zone.
//   - the *.vegify.app ACM cert — ACM certs are not CloudFormation-importable; it stays in Terraform
//     (still valid: its DNS validation CNAMEs ARE adopted below), or is recreated in CDK separately.
//
// The three ttl=60 `*._domainkey` CNAMEs are stale SES DKIM rotation leftovers (the live identity signs
// with the three ttl=1800 tokens); adopted as-is for a zero-change import, safe to prune in a follow-up.
//
// Standing stack: `cdk deploy VegifyDns` on demand — the zone changes rarely, so it is NOT in the
// per-release cascade. See docs/dns-migration.md for the import runbook.

const ZONE_ID = "Z1SPMY1OKJ3UDI";

type RecordSpec = {
  id: string;
  name: string;
  type: string;
  ttl: number;
  values: string[];
};

// 18 adopted records — generated from the live zone (aws route53 list-resource-record-sets). The unused
// Mailchimp DKIM (k1._domainkey → dkim.mcsv.net) and its apex-SPF include:servers.mcsv.net were pruned.
const RECORDS: RecordSpec[] = [
  {
    id: "vegify_app_MX",
    name: "vegify.app.",
    type: "MX",
    ttl: 300,
    values: ["10 inbound-smtp.us-east-1.amazonaws.com."],
  },
  {
    id: "vegify_app_TXT",
    name: "vegify.app.",
    type: "TXT",
    ttl: 300,
    values: [
      '"v=spf1 include:amazonses.com ~all"',
      '"google-site-verification=bgECrGfVynKBnnWL9B8iopR_r_MIXV8Y2SQC2VrPzP4"',
      '"google-site-verification=e5YCGZ2Exi2ELp26Qt1-adKLcUBDEtwT2GM8gbBEGUc"',
    ],
  },
  {
    id: "1e4e1927c5972f392e1073b1d24c0b4e_vegify_app_CNAME",
    name: "_1e4e1927c5972f392e1073b1d24c0b4e.vegify.app.",
    type: "CNAME",
    ttl: 500,
    values: [
      "_d2dcc1ad9d094250e651ab9107232078.ljbhxbcwgb.acm-validations.aws.",
    ],
  },
  {
    id: "amazonses_vegify_app_TXT",
    name: "_amazonses.vegify.app.",
    type: "TXT",
    ttl: 60,
    values: ['"G/vvmTE36KO4O6JRI1uveXxP1unH08tQUhnKUtOYCjM="'],
  },
  {
    id: "dmarc_vegify_app_TXT",
    name: "_dmarc.vegify.app.",
    type: "TXT",
    ttl: 300,
    values: [
      '"v=DMARC1;p=quarantine;pct=100;fo=1;rua=mailto:dmarc@vegify.app"',
    ],
  },
  {
    id: "hup23umay4txaqw3fvmqeljqhe6kqnyt_domainkey_vegify_app_CNAME",
    name: "hup23umay4txaqw3fvmqeljqhe6kqnyt._domainkey.vegify.app.",
    type: "CNAME",
    ttl: 60,
    values: ["hup23umay4txaqw3fvmqeljqhe6kqnyt.dkim.amazonses.com"],
  },
  {
    id: "j72ozgr7l45gx5ugrvsqewepd4wzukvx_domainkey_vegify_app_CNAME",
    name: "j72ozgr7l45gx5ugrvsqewepd4wzukvx._domainkey.vegify.app.",
    type: "CNAME",
    ttl: 1800,
    values: ["j72ozgr7l45gx5ugrvsqewepd4wzukvx.dkim.amazonses.com"],
  },
  {
    id: "khfgpkypf5sx4bumgilqpefxaj66mtas_domainkey_vegify_app_CNAME",
    name: "khfgpkypf5sx4bumgilqpefxaj66mtas._domainkey.vegify.app.",
    type: "CNAME",
    ttl: 60,
    values: ["khfgpkypf5sx4bumgilqpefxaj66mtas.dkim.amazonses.com"],
  },
  {
    id: "ozkn526xz7lbieofqgtsghh6xz5ror6t_domainkey_vegify_app_CNAME",
    name: "ozkn526xz7lbieofqgtsghh6xz5ror6t._domainkey.vegify.app.",
    type: "CNAME",
    ttl: 1800,
    values: ["ozkn526xz7lbieofqgtsghh6xz5ror6t.dkim.amazonses.com"],
  },
  {
    id: "rxm2snlxdggmdztfcpti7lpwzw2jor6i_domainkey_vegify_app_CNAME",
    name: "rxm2snlxdggmdztfcpti7lpwzw2jor6i._domainkey.vegify.app.",
    type: "CNAME",
    ttl: 60,
    values: ["rxm2snlxdggmdztfcpti7lpwzw2jor6i.dkim.amazonses.com"],
  },
  {
    id: "zjgtewy7ddklmo7k7xxgi4xedihojpbr_domainkey_vegify_app_CNAME",
    name: "zjgtewy7ddklmo7k7xxgi4xedihojpbr._domainkey.vegify.app.",
    type: "CNAME",
    ttl: 1800,
    values: ["zjgtewy7ddklmo7k7xxgi4xedihojpbr.dkim.amazonses.com"],
  },
  {
    id: "e0e147fc21ede79a531df947fb1c87fb_vegify_app_CNAME",
    name: "_e0e147fc21ede79a531df947fb1c87fb.vegify.app.",
    type: "CNAME",
    ttl: 60,
    values: [
      "_f3bf70a462d97f3f49f2a12b5ac7b81a.tljzshvwok.acm-validations.aws",
    ],
  },
  {
    id: "6cc83c2b29e4f2437ba59e6a76ea36b8_dev_vegify_app_CNAME",
    name: "_6cc83c2b29e4f2437ba59e6a76ea36b8.dev.vegify.app.",
    type: "CNAME",
    ttl: 300,
    values: [
      "_207cc886a4c600c8c50c2c5ead7d5689.rlltrpyzyf.acm-validations.aws.",
    ],
  },
  {
    id: "mailfrom_vegify_app_MX",
    name: "mailfrom.vegify.app.",
    type: "MX",
    ttl: 300,
    values: ["10 feedback-smtp.us-east-1.amazonses.com"],
  },
  {
    id: "mailfrom_vegify_app_TXT",
    name: "mailfrom.vegify.app.",
    type: "TXT",
    ttl: 300,
    values: ['"v=spf1 include:amazonses.com ~all"'],
  },
  {
    id: "5ecec136149b54582759caa2b30bc41d_prod_vegify_app_CNAME",
    name: "_5ecec136149b54582759caa2b30bc41d.prod.vegify.app.",
    type: "CNAME",
    ttl: 300,
    values: [
      "_ce6f30ff15d0c2f201171b2841742e74.vtqfhvjlcp.acm-validations.aws.",
    ],
  },
  {
    id: "2bb4a8a446657ef0e422e84a588323c0_staging_vegify_app_CNAME",
    name: "_2bb4a8a446657ef0e422e84a588323c0.staging.vegify.app.",
    type: "CNAME",
    ttl: 300,
    values: [
      "_597b46f02908a17e5f4180e3dcd6cbc3.vtqfhvjlcp.acm-validations.aws.",
    ],
  },
  {
    id: "22ddd698dabfca5b2d9ada7695b56e5f_www_vegify_app_CNAME",
    name: "_22ddd698dabfca5b2d9ada7695b56e5f.www.vegify.app.",
    type: "CNAME",
    ttl: 300,
    values: [
      "_659915934bff13271eb052a5b73a18c5.tljzshvwok.acm-validations.aws.",
    ],
  },
];

export class DnsStack extends Stack {
  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    // The hosted zone itself (adopted in place — its NS/SOA, and so the domain's delegation, are
    // untouched by the import). RETAIN so CloudFormation can NEVER delete the live zone (a stack
    // destroy or resource removal leaves it standing in Route53).
    const zone = new route53.CfnHostedZone(this, "Zone", {
      name: "vegify.app.",
    });
    zone.cfnOptions.deletionPolicy = CfnDeletionPolicy.RETAIN;
    zone.cfnOptions.updateReplacePolicy = CfnDeletionPolicy.RETAIN;

    // `AWS::Route53::RecordSet` is not CloudFormation-importable, so the records can't ride the same
    // `cdk import` as the zone. The one-time migration imports the zone alone (`-c dnsImportZoneOnly=1`),
    // then a normal `cdk deploy` adopts the records: CloudFormation creates RecordSets via Route53 UPSERT,
    // so deploying values that already exist verbatim is a no-op that simply brings them under management.
    if (this.node.tryGetContext("dnsImportZoneOnly")) return;

    // Each record is pinned to the literal zone id so the `cdk import` identity (HostedZoneId|Name|Type)
    // is unambiguous and independent of zone-resource ordering. All RETAIN — same reasoning as the zone.
    for (const r of RECORDS) {
      const rec = new route53.CfnRecordSet(this, r.id, {
        hostedZoneId: ZONE_ID,
        name: r.name,
        type: r.type,
        ttl: String(r.ttl),
        resourceRecords: r.values,
      });
      rec.cfnOptions.deletionPolicy = CfnDeletionPolicy.RETAIN;
      rec.cfnOptions.updateReplacePolicy = CfnDeletionPolicy.RETAIN;
    }

    // The TLS certificate, recreated in CDK to replace the Terraform-managed one (ACM certs are not
    // CloudFormation-importable). apex + wildcard covers vegify.app and every subdomain (www, dev,
    // staging, prod), matching the old cert's coverage. DNS-validated through the now-CDK-owned zone —
    // CDK writes the validation records and waits for ISSUED. CloudFront (VegifyWebStart) is repointed
    // to CertArn via the VEGIFY_CERT_ARN secret; the old cert is then dropped from Terraform.
    const zoneRef = route53.HostedZone.fromHostedZoneAttributes(
      this,
      "ZoneRef",
      {
        hostedZoneId: ZONE_ID,
        zoneName: "vegify.app",
      },
    );
    const cert = new acm.Certificate(this, "Cert", {
      domainName: "vegify.app",
      subjectAlternativeNames: ["*.vegify.app"],
      validation: acm.CertificateValidation.fromDns(zoneRef),
    });
    new CfnOutput(this, "CertArn", { value: cert.certificateArn });
  }
}
