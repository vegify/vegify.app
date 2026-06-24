import * as path from "node:path";
import { CfnOutput, Size, Stack, type StackProps } from "aws-cdk-lib";
import * as cloudfront from "aws-cdk-lib/aws-cloudfront";
import * as origins from "aws-cdk-lib/aws-cloudfront-origins";
import * as ec2 from "aws-cdk-lib/aws-ec2";
import * as ecs from "aws-cdk-lib/aws-ecs";
import * as elbv2 from "aws-cdk-lib/aws-elasticloadbalancingv2";
import * as logs from "aws-cdk-lib/aws-logs";
import type { Construct } from "constructs";

const repoRoot = path.resolve(import.meta.dirname, "../..");
const webStart = path.join(repoRoot, "apps/web-start");

interface WebStartFargateStackProps extends StackProps {
  vpc: ec2.Vpc;
}

/**
 * web-start on Bun/Fargate — the PERFORMANCE path, kept ready but deliberately NOT deployed.
 *
 * This is the bake-off winner (Bun: ~+10% read throughput and a ~10x tighter p99.9 tail vs Node on
 * the same build — see docs/benchmark.md), realized by a long-running `Bun.serve` process in the
 * DB's VPC. It is a STANDING cost (a warm Fargate task + an ALB ≈ $25-30/mo), unjustifiable for a
 * zero-revenue app — so today the free-tier WebStartStack (Lambda + EFS) serves instead.
 *
 * FLIP TO THIS when revenue justifies the tail win:
 *   1. In bin/vegify.ts, replace `new WebStartStack(...)` with `new WebStartFargateStack(app,
 *      "VegifyWebStart", { env, vpc: net.vpc })`.
 *   2. `pnpm --filter web-start build:aws` (produces dist/ + the seed the Dockerfile bakes).
 *   3. `cdk deploy VegifyVpc VegifyWebStart`.
 *
 * DB shape: option (ii) "collapse" — libSQL embedded IN-PROCESS in the task against a mounted EBS
 * volume (`DATABASE_URL=file:/data/vegify.db`). One task, no separate DB service, proper local-disk
 * file locking (WAL is fine on EBS, unlike EFS/NFS), single-writer at this scale. The image is
 * apps/web-start/Dockerfile (oven/bun + the build + serve-bun.mjs + @libsql/client + seed DB).
 */
export class WebStartFargateStack extends Stack {
  constructor(scope: Construct, id: string, props: WebStartFargateStackProps) {
    super(scope, id, props);
    const { vpc } = props;

    const cluster = new ecs.Cluster(this, "Cluster", { vpc });
    const taskDef = new ecs.FargateTaskDefinition(this, "Task", { cpu: 512, memoryLimitMiB: 1024 });

    // In-process libSQL on a persistent EBS volume (proper file locking; WAL works here).
    const volume = new ecs.ServiceManagedVolume(this, "Data", {
      name: "vegify-data",
      managedEBSVolume: {
        size: Size.gibibytes(10),
        volumeType: ec2.EbsDeviceVolumeType.GP3,
        fileSystemType: ecs.FileSystemType.EXT4,
      },
    });
    taskDef.addVolume(volume);

    const container = taskDef.addContainer("web", {
      image: ecs.ContainerImage.fromAsset(webStart, { file: "Dockerfile" }),
      portMappings: [{ containerPort: 3001 }],
      environment: { DATABASE_URL: "file:/data/vegify.db", PORT: "3001", NODE_ENV: "production" },
      logging: ecs.LogDrivers.awsLogs({
        streamPrefix: "web-start",
        logRetention: logs.RetentionDays.ONE_MONTH,
      }),
    });
    container.addMountPoints({ containerPath: "/data", sourceVolume: volume.name, readOnly: false });

    const service = new ecs.FargateService(this, "Service", {
      cluster,
      taskDefinition: taskDef,
      desiredCount: 1, // single-writer libSQL — never run two tasks at once
      vpcSubnets: { subnetType: ec2.SubnetType.PUBLIC }, // public IP for image-pull egress (no NAT)
      assignPublicIp: true,
      minHealthyPercent: 0,
      maxHealthyPercent: 100,
      circuitBreaker: { enable: true, rollback: true },
    });
    service.addVolume(volume);

    // ALB is the stable origin; CloudFront in front for HTTPS (its default cert) + asset caching.
    const alb = new elbv2.ApplicationLoadBalancer(this, "Alb", { vpc, internetFacing: true });
    alb.addListener("Http", { port: 80, open: true }).addTargets("Web", {
      port: 3001,
      targets: [service],
      healthCheck: { path: "/", healthyHttpCodes: "200-399" },
    });

    const distribution = new cloudfront.Distribution(this, "Cdn", {
      defaultBehavior: {
        origin: new origins.LoadBalancerV2Origin(alb, {
          protocolPolicy: cloudfront.OriginProtocolPolicy.HTTP_ONLY,
        }),
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
        allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
        cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
        originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER,
      },
    });

    new CfnOutput(this, "Url", { value: `https://${distribution.distributionDomainName}` });
  }
}
