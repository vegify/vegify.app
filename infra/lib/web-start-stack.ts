import * as path from "node:path";
import { CfnOutput, Duration, RemovalPolicy, Stack, type StackProps } from "aws-cdk-lib";
import * as cloudfront from "aws-cdk-lib/aws-cloudfront";
import * as origins from "aws-cdk-lib/aws-cloudfront-origins";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as s3deploy from "aws-cdk-lib/aws-s3-deployment";
import * as acm from "aws-cdk-lib/aws-certificatemanager";
import * as route53 from "aws-cdk-lib/aws-route53";
import * as targets from "aws-cdk-lib/aws-route53-targets";
import type { Construct } from "constructs";

const repoRoot = path.resolve(import.meta.dirname, "../..");
const webStart = path.join(repoRoot, "apps/web");

// Deployment-specific identifiers come from the environment (CI injects them from repository
// secrets) so the open-source tree carries no account-specific values. The fallbacks are inert
// placeholders that let a fresh `cdk synth` succeed; real deploys set the env.
//
//   VEGIFY_API_URL        — the standing Axum backend's public origin; the SSR shell calls it for
//                           ALL auth + content (P4: web-SSR-calls-Axum).
//   VEGIFY_INGEST_ORIGIN  — the browser-log Lambda's Function URL host (VegifyClientLogs); the client
//                           beacons a SAME-ORIGIN POST to /__ingest, forwarded here first-party so ad/
//                           tracking blockers don't eat it (a raw cross-origin beacon gets dropped).
//   VEGIFY_DOMAIN_NAMES / _HOSTED_ZONE_ID / _CERT_ARN — custom-domain wiring; the cert must be in
//                           us-east-1 (CloudFront requirement), which this stack is.
const VEGIFY_API_URL = process.env.VEGIFY_API_URL ?? "https://api.example.com";
const INGEST_ORIGIN = process.env.VEGIFY_INGEST_ORIGIN ?? "ingest.example.com";
const DOMAIN_NAMES = (process.env.VEGIFY_DOMAIN_NAMES ?? "example.com,www.example.com").split(",");
const HOSTED_ZONE_ID = process.env.VEGIFY_HOSTED_ZONE_ID ?? "ZEXAMPLE00000000";
const CERTIFICATE_ARN =
  process.env.VEGIFY_CERT_ARN ??
  "arn:aws:acm:us-east-1:123456789012:certificate/00000000-0000-0000-0000-000000000000";

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
export interface WebStartStackProps extends StackProps {
  /** Origin-verify secret: CloudFront injects it as `x-vegify-origin` on the Function URL origins and the
   *  Lambdas reject requests lacking it, restricting the URLs to CloudFront. Empty = off (fail-open).
   *  Sourced from $ORIGIN_VERIFY_SECRET (a GitHub Actions secret in CI) — see bin/vegify.ts. */
  originSecret: string;
}

export class WebStartStack extends Stack {
  constructor(scope: Construct, id: string, props: WebStartStackProps) {
    super(scope, id, props);

    // CloudFront injects this on every forwarded origin request; the Function URL Lambdas reject anything
    // lacking it (a direct public hit). undefined when no secret is set → no header, no enforcement.
    const verifyHeaders = props.originSecret ? { "x-vegify-origin": props.originSecret } : undefined;

    const fn = new lambda.Function(this, "ServerFn", {
      runtime: lambda.Runtime.NODEJS_22_X,
      architecture: lambda.Architecture.X86_64,
      handler: "handler.handler",
      // The SSR build is self-contained (ssr.noExternal bundles every dep), so the bundle is just
      // handler.mjs + server/ + package.json({type:module}) — no node_modules, no Docker bundling.
      code: lambda.Code.fromAsset(path.join(webStart, ".aws-lambda")),
      memorySize: 1024,
      timeout: Duration.seconds(30),
      environment: { VEGIFY_API_URL, NODE_ENV: "production", ORIGIN_SECRET: props.originSecret },
    });
    const fnUrl = fn.addFunctionUrl({ authType: lambda.FunctionUrlAuthType.NONE });

    const assets = new s3.Bucket(this, "Assets", {
      removalPolicy: RemovalPolicy.DESTROY,
      autoDeleteObjects: true,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
    });
    new s3deploy.BucketDeployment(this, "DeployClient", {
      sources: [s3deploy.Source.asset(path.join(webStart, "dist/client"))],
      destinationBucket: assets,
    });

    const zone = route53.HostedZone.fromHostedZoneAttributes(this, "Zone", {
      hostedZoneId: HOSTED_ZONE_ID,
      zoneName: DOMAIN_NAMES[0],
    });
    const certificate = acm.Certificate.fromCertificateArn(this, "Cert", CERTIFICATE_ARN);

    // Force `Content-Type: application/json` on /.well-known/* responses. S3 stores the extensionless
    // apple-app-site-association file as `binary/octet-stream`, but Apple's swcd requires
    // `application/json` to accept a webcredentials association — without it macOS Password AutoFill
    // never recognizes the domain (1Password offers nothing at the sign-in form). override=true so it
    // replaces S3's default type. (The AASA is the only well-known file today; revisit if a non-JSON
    // /.well-known/* file is ever added.)
    const wellKnownJson = new cloudfront.ResponseHeadersPolicy(this, "WellKnownJson", {
      customHeadersBehavior: {
        customHeaders: [{ header: "content-type", value: "application/json", override: true }],
      },
    });

    const distribution = new cloudfront.Distribution(this, "Cdn", {
      domainNames: DOMAIN_NAMES,
      certificate,
      defaultBehavior: {
        origin: new origins.FunctionUrlOrigin(fnUrl, { customHeaders: verifyHeaders }),
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
        allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
        cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
        // Forward everything except Host so the session cookie reaches the SSR handler.
        originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
      },
      additionalBehaviors: {
        // Built client assets (hashed JS/CSS) — immutable, cache hard.
        "/assets/*": {
          origin: origins.S3BucketOrigin.withOriginAccessControl(assets),
          viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
        },
        // First-party browser-log ingestion. The client beacons same-origin POST /__ingest; we forward
        // it to the VegifyClientLogs Lambda Function URL. First-party so blockers don't eat it; no
        // caching; POST allowed; ALL_VIEWER_EXCEPT_HOST_HEADER so CloudFront sets Host to the Function
        // URL host (a Lambda Function URL rejects a mismatched Host).
        "/__ingest": {
          origin: new origins.HttpOrigin(INGEST_ORIGIN, { customHeaders: verifyHeaders }),
          viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
          cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
          originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
        },
        // Apple App Site Association (and any future /.well-known/*) — static, from S3, uncached so a
        // Team-ID fix or rotation goes live immediately. Lets macOS Password AutoFill associate the
        // desktop app with vegify.app (webcredentials). The response-headers policy rewrites S3's
        // binary/octet-stream to application/json (swcd requires it — see above).
        "/.well-known/*": {
          origin: origins.S3BucketOrigin.withOriginAccessControl(assets),
          viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
          responseHeadersPolicy: wellKnownJson,
        },
      },
    });

    // Point the apex + www at the distribution (alias A/AAAA; both are covered by the cert above).
    const aliasTarget = route53.RecordTarget.fromAlias(new targets.CloudFrontTarget(distribution));
    new route53.ARecord(this, "ApexA", { zone, target: aliasTarget });
    new route53.AaaaRecord(this, "ApexAaaa", { zone, target: aliasTarget });
    new route53.ARecord(this, "WwwA", { zone, recordName: "www", target: aliasTarget });
    new route53.AaaaRecord(this, "WwwAaaa", { zone, recordName: "www", target: aliasTarget });

    new CfnOutput(this, "Url", { value: `https://${distribution.distributionDomainName}` });
    new CfnOutput(this, "CustomDomain", { value: `https://${DOMAIN_NAMES[0]}` });
    new CfnOutput(this, "FunctionUrl", { value: fnUrl.url });
  }
}
