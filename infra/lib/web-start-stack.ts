import * as path from "node:path";
import { CfnOutput, Duration, RemovalPolicy, Stack, type StackProps } from "aws-cdk-lib";
import * as cloudfront from "aws-cdk-lib/aws-cloudfront";
import * as origins from "aws-cdk-lib/aws-cloudfront-origins";
import * as ec2 from "aws-cdk-lib/aws-ec2";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as s3deploy from "aws-cdk-lib/aws-s3-deployment";
import type { Construct } from "constructs";

const repoRoot = path.resolve(import.meta.dirname, "../..");
const webStart = path.join(repoRoot, "apps/web-start");

interface WebStartStackProps extends StackProps {
  vpc: ec2.Vpc;
  dbClientSecurityGroup: ec2.SecurityGroup;
  databaseUrl: string;
}

/**
 * web-start: the WinterCG fetch handler wrapped by `aws/lambda-handler.mjs`, behind CloudFront,
 * with `dist/client/*` static assets on S3.
 *
 * Deploy prep (see infra/README): assemble `apps/web-start/.aws-lambda/` =
 *   { handler.mjs (from aws/lambda-handler.mjs), server/ (from dist/server), node_modules with
 *     @libsql/client built for the Lambda arch }. The native binding MUST match Lambda's arch.
 *
 * TODO before prod: restrict the Function URL to CloudFront via OAC (currently authType NONE).
 */
export class WebStartStack extends Stack {
  constructor(scope: Construct, id: string, props: WebStartStackProps) {
    super(scope, id, props);
    const { vpc, dbClientSecurityGroup, databaseUrl } = props;

    const fn = new lambda.Function(this, "ServerFn", {
      runtime: lambda.Runtime.NODEJS_22_X,
      architecture: lambda.Architecture.ARM_64,
      handler: "handler.handler",
      code: lambda.Code.fromAsset(path.join(webStart, ".aws-lambda")),
      memorySize: 1024,
      timeout: Duration.seconds(30),
      vpc,
      vpcSubnets: { subnetType: ec2.SubnetType.PRIVATE_ISOLATED },
      securityGroups: [dbClientSecurityGroup],
      environment: { DATABASE_URL: databaseUrl, NODE_ENV: "production" },
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

    const distribution = new cloudfront.Distribution(this, "Cdn", {
      defaultBehavior: {
        origin: new origins.FunctionUrlOrigin(fnUrl),
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
        allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
        cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
        originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
      },
      additionalBehaviors: {
        "/assets/*": {
          origin: origins.S3BucketOrigin.withOriginAccessControl(assets),
          viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
        },
      },
    });

    new CfnOutput(this, "Url", { value: `https://${distribution.distributionDomainName}` });
  }
}
