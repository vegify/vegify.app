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
const openNext = path.join(repoRoot, "apps/web-next/.open-next");

interface WebNextStackProps extends StackProps {
  vpc: ec2.Vpc;
  dbClientSecurityGroup: ec2.SecurityGroup;
  databaseUrl: string;
}

/**
 * web-next via OpenNext: a server Lambda (SSR/RSC/server actions) + image-optimization
 * Lambda behind CloudFront, with static assets on S3. Run `pnpm --filter web-next build:aws`
 * first so `apps/web-next/.open-next/` exists.
 *
 * TODO before prod: restrict the Function URLs to CloudFront via OAC (currently authType NONE);
 * add OpenNext's revalidation queue + tag cache (SQS/DynamoDB) if/when ISR is used (today the
 * app is force-dynamic, so it isn't); bundle @libsql/client for the Lambda arch (see infra/README).
 */
export class WebNextStack extends Stack {
  constructor(scope: Construct, id: string, props: WebNextStackProps) {
    super(scope, id, props);
    const { vpc, dbClientSecurityGroup, databaseUrl } = props;

    const lambdaProps = {
      runtime: lambda.Runtime.NODEJS_22_X,
      vpc,
      vpcSubnets: { subnetType: ec2.SubnetType.PRIVATE_ISOLATED },
      securityGroups: [dbClientSecurityGroup],
      environment: { DATABASE_URL: databaseUrl, NODE_ENV: "production" },
    };

    const serverFn = new lambda.Function(this, "ServerFn", {
      ...lambdaProps,
      handler: "index.handler",
      code: lambda.Code.fromAsset(path.join(openNext, "server-functions/default")),
      memorySize: 1024,
      timeout: Duration.seconds(30),
    });

    const imageFn = new lambda.Function(this, "ImageFn", {
      ...lambdaProps,
      handler: "index.handler",
      code: lambda.Code.fromAsset(path.join(openNext, "image-optimization-function")),
      memorySize: 1536,
      timeout: Duration.seconds(30),
    });

    const serverUrl = serverFn.addFunctionUrl({ authType: lambda.FunctionUrlAuthType.NONE });
    const imageUrl = imageFn.addFunctionUrl({ authType: lambda.FunctionUrlAuthType.NONE });

    const assets = new s3.Bucket(this, "Assets", {
      removalPolicy: RemovalPolicy.DESTROY,
      autoDeleteObjects: true,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
    });
    new s3deploy.BucketDeployment(this, "DeployAssets", {
      sources: [s3deploy.Source.asset(path.join(openNext, "assets"))],
      destinationBucket: assets,
    });

    const s3Origin = origins.S3BucketOrigin.withOriginAccessControl(assets);
    const longCacheS3 = {
      origin: s3Origin,
      viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
      cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
    };
    const lambdaBehavior = (url: lambda.IFunctionUrl) => ({
      origin: new origins.FunctionUrlOrigin(url),
      viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
      allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
      cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
      originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
    });

    const distribution = new cloudfront.Distribution(this, "Cdn", {
      defaultBehavior: lambdaBehavior(serverUrl),
      additionalBehaviors: {
        "/_next/static/*": longCacheS3,
        "/_next/image*": lambdaBehavior(imageUrl),
        // NOTE: reconcile remaining static/public routes against .open-next/open-next.output.json
      },
    });

    new CfnOutput(this, "Url", { value: `https://${distribution.distributionDomainName}` });
  }
}
