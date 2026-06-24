import * as path from "node:path";
import { CfnOutput, Duration, RemovalPolicy, Stack, type StackProps } from "aws-cdk-lib";
import * as cloudfront from "aws-cdk-lib/aws-cloudfront";
import * as origins from "aws-cdk-lib/aws-cloudfront-origins";
import * as ec2 from "aws-cdk-lib/aws-ec2";
import * as efs from "aws-cdk-lib/aws-efs";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as s3deploy from "aws-cdk-lib/aws-s3-deployment";
import type { Construct } from "constructs";

const repoRoot = path.resolve(import.meta.dirname, "../..");
const webStart = path.join(repoRoot, "apps/web-start");

interface WebStartStackProps extends StackProps {
  vpc: ec2.Vpc;
}

/**
 * web-start on the FREE TIER (the chosen hosting shape — see infra/README "Hosting decision").
 *
 * TanStack Start's WinterCG handler runs on a scale-to-zero **Lambda** (Function URL) behind
 * **CloudFront**; built client assets are served from **S3**. The libSQL DB is a file on **EFS**
 * — the only persistent store a stateless Lambda can WRITE to inside the always-free tier — at
 * `DATABASE_URL=file:/mnt/data/vegify.db`. At zero/low traffic this is ~$0/mo: Lambda + CloudFront
 * + S3 sit in the always-free tier and EFS is 5 GB free for 12 months (a sub-1 GB DB ≈ pennies after).
 *
 * Why not Bun-on-Fargate (the measured perf winner)? That's a *standing* cost (a warm task + ALB),
 * unjustifiable for a zero-revenue app. The Fargate path is kept ready in web-start-fargate-stack.ts;
 * flip to it when revenue justifies the tail-latency win. See the README for the full rationale.
 *
 * EFS gotchas handled here:
 *  - Single writer. SQLite over NFS is lock-finicky, so the Lambda is pinned to
 *    `reservedConcurrentExecutions: 1` — one writer at a time. Fine at low traffic; not for heavy
 *    write concurrency (that's the Fargate trigger).
 *  - Rollback journal, not WAL. The bundled seed DB is in DELETE mode (assemble-bundle.mjs); WAL
 *    needs shared memory EFS can't provide. The handler seeds EFS from it on first cold start.
 *
 * TODO before prod: restrict the Function URL to CloudFront via OAC (currently authType NONE).
 */
export class WebStartStack extends Stack {
  constructor(scope: Construct, id: string, props: WebStartStackProps) {
    super(scope, id, props);
    const { vpc } = props;

    // EFS holds the writable SQLite file. RETAIN-worthy in prod; DESTROY here keeps dev teardown
    // clean (the DB re-seeds from the baked copy on next deploy).
    const fileSystem = new efs.FileSystem(this, "Data", {
      vpc,
      vpcSubnets: { subnetType: ec2.SubnetType.PRIVATE_ISOLATED },
      removalPolicy: RemovalPolicy.DESTROY,
    });
    const accessPoint = fileSystem.addAccessPoint("LambdaAp", {
      path: "/vegify",
      createAcl: { ownerUid: "1001", ownerGid: "1001", permissions: "750" },
      posixUser: { uid: "1001", gid: "1001" },
    });

    const fn = new lambda.Function(this, "ServerFn", {
      runtime: lambda.Runtime.NODEJS_22_X,
      // x86_64 so the Docker bundling runs NATIVELY on the x86 GitHub Actions runner (no qemu);
      // deploys run in CI now. The platform pin keeps the installed @libsql native binding matched.
      architecture: lambda.Architecture.X86_64,
      handler: "handler.handler",
      code: lambda.Code.fromAsset(path.join(webStart, ".aws-lambda"), {
        bundling: {
          image: lambda.Runtime.NODEJS_22_X.bundlingImage,
          platform: "linux/amd64",
          command: [
            "bash",
            "-c",
            // HOME=/tmp: the container runs as the host uid, so npm's default ~/.npm (/.npm) isn't writable.
            "export HOME=/tmp && cp -a /asset-input/. /asset-output/ && cd /asset-output && " +
              "npm install --omit=dev --no-package-lock --cache /tmp/.npm",
          ],
        },
      }),
      memorySize: 1024,
      timeout: Duration.seconds(30),
      vpc,
      vpcSubnets: { subnetType: ec2.SubnetType.PRIVATE_ISOLATED },
      filesystem: lambda.FileSystem.fromEfsAccessPoint(accessPoint, "/mnt/data"),
      reservedConcurrentExecutions: 1, // single writer over EFS
      environment: { DATABASE_URL: "file:/mnt/data/vegify.db", NODE_ENV: "production" },
    });
    fileSystem.connections.allowDefaultPortFrom(fn); // Lambda SG → EFS NFS (2049)
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

    const distribution = new cloudfront.Distribution(this, "Cdn", {
      defaultBehavior: {
        origin: new origins.FunctionUrlOrigin(fnUrl),
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
        allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
        cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
        originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
      },
      additionalBehaviors: {
        // Built client assets (hashed JS/CSS) — immutable, cache hard.
        "/assets/*": {
          origin: origins.S3BucketOrigin.withOriginAccessControl(assets),
          viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
        },
      },
    });

    new CfnOutput(this, "Url", { value: `https://${distribution.distributionDomainName}` });
    new CfnOutput(this, "FunctionUrl", { value: fnUrl.url });
  }
}
