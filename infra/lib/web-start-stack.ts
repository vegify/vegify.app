import * as path from "node:path";
import {
  CfnOutput,
  Duration,
  Fn,
  RemovalPolicy,
  Stack,
  type StackProps,
} from "aws-cdk-lib";
import * as acm from "aws-cdk-lib/aws-certificatemanager";
import * as cloudfront from "aws-cdk-lib/aws-cloudfront";
import * as origins from "aws-cdk-lib/aws-cloudfront-origins";
import * as cloudwatch from "aws-cdk-lib/aws-cloudwatch";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as logs from "aws-cdk-lib/aws-logs";
import * as route53 from "aws-cdk-lib/aws-route53";
import * as targets from "aws-cdk-lib/aws-route53-targets";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as s3deploy from "aws-cdk-lib/aws-s3-deployment";
import type { Construct } from "constructs";

import { cloudFrontMetric, importAlarmTopic, notify } from "./monitoring.js";
import { resolveZone } from "./zone.js";

const repoRoot = path.resolve(import.meta.dirname, "../..");
const webStart = path.join(repoRoot, "apps/web");

/**
 * web-start — a STATELESS SSR shell (P4: web-SSR-calls-Axum). It holds NO database: TanStack Start's
 * WinterCG handler runs on a scale-to-zero **Lambda** (Function URL) behind **CloudFront**, built
 * client assets are served from **S3**, and ALL auth + content is fetched over HTTP from the standing
 * **vegify-server** (the EBS-backed source of truth) at VEGIFY_API_URL.
 *
 * No VPC: the Lambda needs plain internet egress to reach the backend's public CloudFront, so it runs
 * OUTSIDE the VPC (a VPC-attached Lambda in our NAT-less subnets couldn't reach it) — which also drops
 * the ENI cold-start penalty. No EFS, no reservedConcurrentExecutions:1 (that single-writer cap over
 * NFS was the 429 source — gone), no native @libsql binding (the SSR build is self-contained via
 * vite `ssr.noExternal`), so the bundle ships as plain JS with no Docker build step.
 *
 * The Bun-on-Fargate perf path stays ready in web-start-fargate-stack.ts.
 * The Function URL stays authType NONE (CloudFront OAC can't sign POST bodies to a Lambda URL —
 * InvalidSignatureException, which is why the OAC attempt was reverted). Instead it's restricted to
 * CloudFront via an origin-verify secret header (props.originSecret) injected on the origin and checked
 * by the Lambda adapter (apps/web/aws/lambda-handler.mjs) — a check that works for POST.
 */
// Deployment coordinates arrive as props from bin/vegify.ts's one deployConfig() read. The backend
// origin and the ingest URL are wired CROSS-STACK from the sibling stacks (never hand-carried); the
// zone is looked up from the domain and the cert is created in-stack unless overridden — the only
// required input is the domain list. See @vegify/config/deploy.
export interface WebStartStackProps extends StackProps {
  /** Origin-verify secret: CloudFront injects it as `x-vegify-origin` on the Function URL origins and
   *  the Lambdas reject requests lacking it, restricting the URLs to CloudFront. Always present now —
   *  it is generated in-account and resolved at deploy time (ClientLogsStack.originSecret), never
   *  supplied by a human. */
  originSecret: string;
  /** The standing Axum backend's public origin; the SSR shell calls it for ALL auth + content
   *  (P4: web-SSR-calls-Axum). Wired from ServerStack.apiUrl (or the VEGIFY_API_URL override). */
  apiUrl: string;
  /** The browser-log Lambda's Function URL (VegifyClientLogs.ingestUrl, cross-stack); its host becomes
   *  the /__ingest origin — the client beacons a SAME-ORIGIN POST to /__ingest, forwarded first-party
   *  so ad/tracking blockers don't eat it (a raw cross-origin beacon gets dropped). */
  ingestUrl: string;
  /** The web shell's domains (first is primary — it becomes VEGIFY_PUBLIC_URL for the Lambda). */
  domainNames: string[];
  /** Whether the domains came from real configuration (gates the zone lookup — see resolveZone). */
  domainsConfigured: boolean;
  /** Explicit zone id override; default = lookup by the primary domain name. */
  hostedZoneIdOverride?: string;
  /** Bring-your-own us-east-1 ACM cert ARN; default = a DNS-validated cert created in this stack
   *  (which is us-east-1 — the CloudFront requirement). */
  certificateArnOverride?: string;
}

export class WebStartStack extends Stack {
  constructor(scope: Construct, id: string, props: WebStartStackProps) {
    super(scope, id, props);
    const { apiUrl, domainNames } = props;
    // https://<host>/ → <host>: CloudFront's HttpOrigin wants a domain, and the split/select works on
    // the deploy-time token (Fn::Select over Fn::Split over the cross-stack import).
    const ingestHost = Fn.select(2, Fn.split("/", props.ingestUrl));

    // CloudFront injects this on every forwarded origin request; the Function URL Lambdas reject anything
    // lacking it (a direct public hit). The value is a deploy-time token — hardening is always on.
    const verifyHeaders = { "x-vegify-origin": props.originSecret };

    const fn = new lambda.Function(this, "ServerFn", {
      runtime: lambda.Runtime.NODEJS_22_X,
      architecture: lambda.Architecture.X86_64,
      handler: "handler.handler",
      // The SSR build is self-contained (ssr.noExternal bundles every dep), so the bundle is just
      // handler.mjs + server/ + package.json({type:module}) — no node_modules, no Docker bundling.
      code: lambda.Code.fromAsset(path.join(webStart, ".aws-lambda")),
      memorySize: 1024,
      timeout: Duration.seconds(30),
      // Explicit log group with retention — the implicit /aws/lambda/<name> group never expires.
      logGroup: new logs.LogGroup(this, "ServerFnLogs", {
        retention: logs.RetentionDays.ONE_MONTH,
        removalPolicy: RemovalPolicy.DESTROY,
      }),
      environment: {
        VEGIFY_API_URL: apiUrl,
        // Canonical public origin for generated URLs (the sitemap): behind CloudFront the Lambda
        // sees only its function-URL host, so the request Host can never be the real site.
        VEGIFY_PUBLIC_URL: `https://${domainNames[0]}`,
        NODE_ENV: "production",
        ORIGIN_SECRET: props.originSecret,
      },
    });
    const fnUrl = fn.addFunctionUrl({
      authType: lambda.FunctionUrlAuthType.NONE,
    });

    const assets = new s3.Bucket(this, "Assets", {
      removalPolicy: RemovalPolicy.DESTROY,
      autoDeleteObjects: true,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
    });
    new s3deploy.BucketDeployment(this, "DeployClient", {
      sources: [s3deploy.Source.asset(path.join(webStart, "dist/client"))],
      destinationBucket: assets,
    });

    const primaryDomain = domainNames[0];
    if (!primaryDomain) throw new Error("domain names are empty");
    const zone = resolveZone(this, "Zone", {
      zoneName: primaryDomain,
      configured: props.domainsConfigured,
      overrideZoneId: props.hostedZoneIdOverride,
    });
    // Bring-your-own cert if an ARN is supplied (vegify's live deploy — avoids cert churn); otherwise
    // create a DNS-validated cert for the domains in the zone above. Issuance is automatic (minutes,
    // first deploy only) — no manual ACM step.
    const certificate = props.certificateArnOverride
      ? acm.Certificate.fromCertificateArn(
          this,
          "Cert",
          props.certificateArnOverride,
        )
      : new acm.Certificate(this, "ManagedCert", {
          domainName: primaryDomain,
          subjectAlternativeNames: domainNames.slice(1),
          validation: acm.CertificateValidation.fromDns(zone),
        });

    // Force `Content-Type: application/json` on /.well-known/* responses. S3 stores the extensionless
    // apple-app-site-association file as `binary/octet-stream`, but Apple's swcd requires
    // `application/json` to accept a webcredentials association — without it macOS Password AutoFill
    // never recognizes the domain (1Password offers nothing at the sign-in form). override=true so it
    // replaces S3's default type. (The AASA is the only well-known file today; revisit if a non-JSON
    // /.well-known/* file is ever added.)
    const wellKnownJson = new cloudfront.ResponseHeadersPolicy(
      this,
      "WellKnownJson",
      {
        customHeadersBehavior: {
          customHeaders: [
            {
              header: "content-type",
              value: "application/json",
              override: true,
            },
          ],
        },
      },
    );

    // A CloudFront behavior serving a static file from the client S3 bucket (used for the root statics).
    const staticFromS3: cloudfront.BehaviorOptions = {
      origin: origins.S3BucketOrigin.withOriginAccessControl(assets),
      viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
      cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
    };
    const distribution = new cloudfront.Distribution(this, "Cdn", {
      domainNames,
      certificate,
      defaultBehavior: {
        origin: new origins.FunctionUrlOrigin(fnUrl, {
          customHeaders: verifyHeaders,
        }),
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
        allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
        cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
        // Forward everything except Host so the session cookie reaches the SSR handler.
        originRequestPolicy:
          cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
      },
      additionalBehaviors: {
        // Built client assets (hashed JS/CSS) — immutable, cache hard.
        "/assets/*": {
          origin: origins.S3BucketOrigin.withOriginAccessControl(assets),
          viewerProtocolPolicy:
            cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
        },
        // First-party browser-log ingestion. The client beacons same-origin POST /__ingest; we forward
        // it to the VegifyClientLogs Lambda Function URL. First-party so blockers don't eat it; no
        // caching; POST allowed; ALL_VIEWER_EXCEPT_HOST_HEADER so CloudFront sets Host to the Function
        // URL host (a Lambda Function URL rejects a mismatched Host).
        "/__ingest": {
          origin: new origins.HttpOrigin(ingestHost, {
            customHeaders: verifyHeaders,
          }),
          viewerProtocolPolicy:
            cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
          cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
          originRequestPolicy:
            cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
        },
        // Apple App Site Association (and any future /.well-known/*) — static, from S3, uncached so a
        // Team-ID fix or rotation goes live immediately. Lets macOS Password AutoFill associate the
        // desktop app with vegify.app (webcredentials). The response-headers policy rewrites S3's
        // binary/octet-stream to application/json (swcd requires it — see above).
        "/.well-known/*": {
          origin: origins.S3BucketOrigin.withOriginAccessControl(assets),
          viewerProtocolPolicy:
            cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
          responseHeadersPolicy: wellKnownJson,
        },
        // robots.txt is a true static — served from S3, NOT the SSR Lambda (whose auth gate 307s any
        // unknown path to /login, which would hide it from crawlers). Uncached so an update ships
        // immediately (the BucketDeployment does not invalidate CloudFront). S3 sets text/plain from the
        // extension, so no content-type override is needed.
        // (/sitemap.xml is NOT here: it's now DYNAMIC — the runtime wrapper, lambda-handler.mjs,
        // intercepts it BEFORE the auth gate and returns generated XML, so it flows through the default
        // Lambda behavior and enumerates every public recipe + ingredient. See apps/web/aws/sitemap.mjs.)
        "/robots.txt": {
          origin: origins.S3BucketOrigin.withOriginAccessControl(assets),
          viewerProtocolPolicy:
            cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
        },
        // Root static files (PWA icons + manifest + favicon), from public/ → S3, NOT the SSR Lambda
        // (whose auth gate 307s unknown paths to /login — which hid the icons/manifest from iOS
        // home-screen, 1Password's icon crawler, and PWA install). Same fix as /robots.txt above.
        "/favicon.ico": staticFromS3,
        "/manifest.json": staticFromS3,
        "/apple-touch-icon.png": staticFromS3,
        "/logo192.png": staticFromS3,
        "/logo512.png": staticFromS3,
      },
    });

    // Point the apex + www at the distribution (alias A/AAAA; both are covered by the cert above).
    const aliasTarget = route53.RecordTarget.fromAlias(
      new targets.CloudFrontTarget(distribution),
    );
    new route53.ARecord(this, "ApexA", { zone, target: aliasTarget });
    new route53.AaaaRecord(this, "ApexAaaa", { zone, target: aliasTarget });
    new route53.ARecord(this, "WwwA", {
      zone,
      recordName: "www",
      target: aliasTarget,
    });
    new route53.AaaaRecord(this, "WwwAaaa", {
      zone,
      recordName: "www",
      target: aliasTarget,
    });

    new CfnOutput(this, "Url", {
      value: `https://${distribution.distributionDomainName}`,
    });
    new CfnOutput(this, "CustomDomain", { value: `https://${domainNames[0]}` });
    new CfnOutput(this, "FunctionUrl", { value: fnUrl.url });

    // ── Observability ────────────────────────────────────────────────────────────────────────────
    // Alarms + a web dashboard on THIS stack's own resources (SSR Lambda + web CloudFront); the shared
    // alarm topic is discovered by ARN from SSM (ServerStack created it earlier in the cascade).
    const alarmTopic = importAlarmTopic(this, "AlarmTopic");
    const p5 = { period: Duration.minutes(5) };
    notify(
      new cloudwatch.Alarm(this, "SsrErrorsAlarm", {
        alarmDescription:
          "Web SSR Lambda erroring — every page render hits this function.",
        metric: fn.metricErrors(p5),
        threshold: 5,
        evaluationPeriods: 2,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      alarmTopic,
    );
    notify(
      new cloudwatch.Alarm(this, "SsrThrottlesAlarm", {
        alarmDescription:
          "Web SSR Lambda throttled — concurrency ceiling hit; pages will 5xx.",
        metric: fn.metricThrottles(p5),
        threshold: 1,
        evaluationPeriods: 1,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_OR_EQUAL_TO_THRESHOLD,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      alarmTopic,
    );
    notify(
      new cloudwatch.Alarm(this, "WebCloudFront5xxAlarm", {
        alarmDescription:
          "Web CloudFront 5xx rate > 5% — the site is serving errors.",
        metric: cloudFrontMetric(
          this,
          distribution.distributionId,
          "5xxErrorRate",
          Duration.minutes(5),
        ),
        threshold: 5,
        evaluationPeriods: 2,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      alarmTopic,
    );

    new cloudwatch.Dashboard(this, "WebDashboard", {
      dashboardName: "Vegify-Web",
      widgets: [
        [
          new cloudwatch.GraphWidget({
            title: "Site requests",
            left: [
              cloudFrontMetric(
                this,
                distribution.distributionId,
                "Requests",
                Duration.minutes(5),
              ),
            ],
            width: 8,
          }),
          new cloudwatch.GraphWidget({
            title: "Site error rates %",
            left: [
              cloudFrontMetric(
                this,
                distribution.distributionId,
                "5xxErrorRate",
                Duration.minutes(5),
              ),
              cloudFrontMetric(
                this,
                distribution.distributionId,
                "4xxErrorRate",
                Duration.minutes(5),
              ),
            ],
            width: 8,
          }),
          new cloudwatch.GraphWidget({
            title: "SSR errors / throttles",
            left: [fn.metricErrors(p5), fn.metricThrottles(p5)],
            width: 8,
          }),
        ],
        [
          new cloudwatch.GraphWidget({
            title: "SSR invocations",
            left: [fn.metricInvocations(p5)],
            width: 12,
          }),
          new cloudwatch.GraphWidget({
            title: "SSR duration (p50/p99 ms)",
            left: [
              fn.metricDuration({ ...p5, statistic: "p50" }),
              fn.metricDuration({ ...p5, statistic: "p99" }),
            ],
            width: 12,
          }),
        ],
      ],
    });
  }
}
