import { Size, Stack, type StackProps } from "aws-cdk-lib";
import * as ec2 from "aws-cdk-lib/aws-ec2";
import * as ecs from "aws-cdk-lib/aws-ecs";
import * as logs from "aws-cdk-lib/aws-logs";
import type { Construct } from "constructs";

interface DbStackProps extends StackProps {
  vpc: ec2.Vpc;
}

/**
 * Self-hosted libSQL server (sqld) — one Fargate task (SQLite is single-writer) with a
 * persistent EBS volume, discoverable in-VPC at `http://sqld.vegify.internal:8080`.
 *
 * Access control is network-level: only members of `clientSecurityGroup` (the app Lambdas)
 * may reach port 8080. The task sits in a public subnet purely for image-pull egress (no NAT).
 *
 * TODO before prod: add sqld JWT auth (SQLD_AUTH_JWT_KEY) + a Secrets Manager
 * DATABASE_AUTH_TOKEN for defense-in-depth; pin the image to a digest; consider EBS snapshots.
 */
export class DbStack extends Stack {
  /** Attach app Lambdas to this SG to be allowed through to sqld. */
  readonly clientSecurityGroup: ec2.SecurityGroup;
  /** DATABASE_URL the apps should use. */
  readonly internalUrl = "http://sqld.vegify.internal:8080";

  constructor(scope: Construct, id: string, props: DbStackProps) {
    super(scope, id, props);
    const { vpc } = props;

    const cluster = new ecs.Cluster(this, "Cluster", {
      vpc,
      defaultCloudMapNamespace: { name: "vegify.internal" },
    });

    this.clientSecurityGroup = new ec2.SecurityGroup(this, "DbClientSg", {
      vpc,
      description: "Allowed to connect to sqld",
    });
    const serviceSg = new ec2.SecurityGroup(this, "SqldSg", { vpc, description: "sqld service" });
    serviceSg.addIngressRule(
      this.clientSecurityGroup,
      ec2.Port.tcp(8080),
      "app Lambdas → sqld HTTP",
    );

    const taskDef = new ecs.FargateTaskDefinition(this, "SqldTask", {
      cpu: 256,
      memoryLimitMiB: 512,
    });

    // Persistent block storage for the SQLite data dir (EBS, not EFS — SQLite needs real
    // file locking). Managed EBS volume attaches to the single task.
    const volume = new ecs.ServiceManagedVolume(this, "Data", {
      name: "sqld-data",
      managedEBSVolume: {
        size: Size.gibibytes(10),
        volumeType: ec2.EbsDeviceVolumeType.GP3,
        fileSystemType: ecs.FileSystemType.EXT4,
      },
    });
    taskDef.addVolume(volume);

    const container = taskDef.addContainer("sqld", {
      image: ecs.ContainerImage.fromRegistry("ghcr.io/tursodatabase/libsql-server:latest"),
      portMappings: [{ containerPort: 8080 }],
      environment: { SQLD_NODE: "primary", SQLD_HTTP_LISTEN_ADDR: "0.0.0.0:8080" },
      logging: ecs.LogDrivers.awsLogs({
        streamPrefix: "sqld",
        logRetention: logs.RetentionDays.ONE_MONTH,
      }),
    });
    container.addMountPoints({
      containerPath: "/var/lib/sqld",
      sourceVolume: volume.name,
      readOnly: false,
    });

    const service = new ecs.FargateService(this, "Sqld", {
      cluster,
      taskDefinition: taskDef,
      desiredCount: 1,
      // public subnet + public IP = egress to pull the image without a NAT gateway
      vpcSubnets: { subnetType: ec2.SubnetType.PUBLIC },
      assignPublicIp: true,
      securityGroups: [serviceSg],
      cloudMapOptions: { name: "sqld" },
      // single-writer DB: never run two tasks at once
      minHealthyPercent: 0,
      maxHealthyPercent: 100,
      circuitBreaker: { enable: true, rollback: true },
    });
    service.addVolume(volume);
  }
}
