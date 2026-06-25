import * as path from "node:path";
import { CfnOutput, Fn, RemovalPolicy, Size, Stack, type StackProps } from "aws-cdk-lib";
import * as cloudfront from "aws-cdk-lib/aws-cloudfront";
import * as origins from "aws-cdk-lib/aws-cloudfront-origins";
import * as ec2 from "aws-cdk-lib/aws-ec2";
import * as iam from "aws-cdk-lib/aws-iam";
import * as s3 from "aws-cdk-lib/aws-s3";
import { Asset } from "aws-cdk-lib/aws-s3-assets";
import type { Construct } from "constructs";

const repoRoot = path.resolve(import.meta.dirname, "../..");
const assetsDir = path.join(repoRoot, "infra/assets");

// CloudFront's origin-facing managed prefix list (us-east-1). Locking the instance's app port to it
// means ONLY CloudFront can reach the origin — the box isn't directly hittable. Region-pinned; this
// account deploys us-east-1. (If we ever multi-region, look this up per-region instead.)
const CLOUDFRONT_ORIGIN_PL = "pl-3b927c52";
const APP_PORT = 8080;
const LITESTREAM = "v0.3.13";

interface ServerStackProps extends StackProps {
  vpc: ec2.Vpc;
}

/**
 * VegifyServer — the standing Axum backend (P2). A single **t4g.nano** (ARM/Graviton) in a public
 * subnet runs the `vegify-server` binary over SQLite-WAL on a dedicated **gp3 EBS** volume (RETAIN, so
 * the DB survives instance replacement); **Litestream** streams the WAL to S3 as the off-box replica +
 * the restore source on a fresh volume. A DEDICATED CloudFront distribution fronts it for HTTPS;
 * the origin port is locked to CloudFront's prefix list.
 *
 * Independent of the live web (VegifyWebStart): its own URL, so the desktop can re-point at P3 without
 * touching the live web until P4. The 429 ceiling dissolves here — WAL gives concurrent readers + a
 * serialized writer with no NFS and no reserved-concurrency-1.
 *
 * t4g.nano (ARM/Graviton) is the cheapest standing box (~$3/mo). This account predates the AWS free
 * tier (it is 9 years old), so there is NO free tier — the instance is chosen on absolute cost, and
 * t4g.nano beats t3.micro. The CI runner is x86, so the aarch64 binary is cross-compiled with `cross`.
 *
 * Delivery: CI cross-builds the aarch64 musl binary + a seed DB into infra/assets/ (shipped as S3 assets). User-data
 * installs Litestream, restores-or-seeds the DB, then runs `litestream replicate -exec vegify-server`
 * under systemd. A new binary changes the asset key → the user-data text → the instance is REPLACED
 * (brief downtime; Litestream preserves the data via S3). SSM (no SSH) is on for debugging.
 */
export class ServerStack extends Stack {
  constructor(scope: Construct, id: string, props: ServerStackProps) {
    super(scope, id, props);
    const { vpc } = props;

    // Durable WAL replica + the restore source on a fresh/replaced instance.
    const replica = new s3.Bucket(this, "Replica", {
      removalPolicy: RemovalPolicy.RETAIN,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
    });

    // Shipped artifacts (CI builds these into infra/assets/; a placeholder binary is fine for synth).
    const serverBin = new Asset(this, "ServerBin", { path: path.join(assetsDir, "vegify-server") });
    const seedDb = new Asset(this, "SeedDb", { path: path.join(assetsDir, "seed.db") });

    const role = new iam.Role(this, "InstanceRole", {
      assumedBy: new iam.ServicePrincipal("ec2.amazonaws.com"),
      // SSM session access for debugging — no SSH, no inbound 22.
      managedPolicies: [iam.ManagedPolicy.fromAwsManagedPolicyName("AmazonSSMManagedInstanceCore")],
    });
    serverBin.grantRead(role);
    seedDb.grantRead(role);
    replica.grantReadWrite(role); // litestream replicate + restore

    const sg = new ec2.SecurityGroup(this, "Sg", { vpc, allowAllOutbound: true });
    sg.addIngressRule(
      ec2.Peer.prefixList(CLOUDFRONT_ORIGIN_PL),
      ec2.Port.tcp(APP_PORT),
      "CloudFront origin-facing → app port",
    );

    const userData = ec2.UserData.forLinux();
    userData.addCommands(
      "set -eux",
      "dnf install -y tar gzip",
      // Mount the dedicated EBS data volume at /data — format on first boot, preserve on reattach (the
      // volume is RETAIN → the DB survives instance replacement). Find the non-root NVMe disk by name.
      "for i in $(seq 1 60); do [ $(lsblk -dno NAME | grep -c nvme) -ge 2 ] && break || sleep 2; done",
      "ROOTDISK=/dev/$(lsblk -no PKNAME $(findmnt -no SOURCE /) | head -1)",
      "DATADEV=$(lsblk -dpno NAME,TYPE | awk '$2==\"disk\"{print $1}' | grep -vx \"$ROOTDISK\" | head -1)",
      "blkid \"$DATADEV\" || mkfs.ext4 -L vegifydata \"$DATADEV\"",
      "mkdir -p /data",
      "mount LABEL=vegifydata /data",
      "grep -q vegifydata /etc/fstab || echo 'LABEL=vegifydata /data ext4 defaults,nofail 0 2' >> /etc/fstab",
      // Litestream (static linux-arm64 release — matches the t4g/Graviton box).
      `curl -fsSL -o /tmp/ls.tgz https://github.com/benbjohnson/litestream/releases/download/${LITESTREAM}/litestream-${LITESTREAM}-linux-arm64.tar.gz`,
      "tar -C /usr/local/bin -xzf /tmp/ls.tgz litestream",
      // Server binary (instance profile reads the asset bucket).
      `aws s3 cp s3://${serverBin.s3BucketName}/${serverBin.s3ObjectKey} /usr/local/bin/vegify-server`,
      "chmod +x /usr/local/bin/vegify-server",
      // Litestream config: replicate /data/vegify.db ↔ the replica bucket.
      "cat >/etc/litestream.yml <<EOF",
      "dbs:",
      "  - path: /data/vegify.db",
      "    replicas:",
      "      - type: s3",
      `        bucket: ${replica.bucketName}`,
      "        path: vegify.db",
      `        region: ${this.region}`,
      "EOF",
      // Restore from S3 if a replica exists; otherwise lay down the baked seed (first-ever boot).
      "if [ ! -f /data/vegify.db ]; then",
      "  litestream restore -if-replica-exists -config /etc/litestream.yml /data/vegify.db || \\",
      `    aws s3 cp s3://${seedDb.s3BucketName}/${seedDb.s3ObjectKey} /data/vegify.db`,
      "fi",
      // systemd: litestream supervises the server (-exec) so every write is captured to S3.
      "cat >/etc/systemd/system/vegify.service <<EOF",
      "[Unit]",
      "Description=vegify-server (litestream-replicated)",
      "After=network-online.target",
      "Wants=network-online.target",
      "[Service]",
      "Environment=DATABASE_PATH=/data/vegify.db",
      `Environment=PORT=${APP_PORT}`,
      "ExecStart=/usr/local/bin/litestream replicate -config /etc/litestream.yml -exec /usr/local/bin/vegify-server",
      "Restart=always",
      "RestartSec=2",
      "[Install]",
      "WantedBy=multi-user.target",
      "EOF",
      "systemctl daemon-reload",
      "systemctl enable --now vegify.service",
    );

    // Dedicated gp3 EBS data volume for the SQLite DB — RETAIN so it (and the data) survives instance
    // replacement; pinned to the instance's AZ. Litestream→S3 is the off-box replica/restore on top.
    const dataVolume = new ec2.Volume(this, "DataVolume", {
      availabilityZone: vpc.publicSubnets[0].availabilityZone,
      size: Size.gibibytes(10),
      volumeType: ec2.EbsDeviceVolumeType.GP3,
      removalPolicy: RemovalPolicy.RETAIN,
    });

    const instance = new ec2.Instance(this, "Server", {
      vpc,
      vpcSubnets: { subnets: [vpc.publicSubnets[0]] }, // pin the AZ to match the data volume
      instanceType: ec2.InstanceType.of(ec2.InstanceClass.T4G, ec2.InstanceSize.NANO),
      machineImage: ec2.MachineImage.latestAmazonLinux2023({ cpuType: ec2.AmazonLinuxCpuType.ARM_64 }),
      securityGroup: sg,
      role,
      userData,
      associatePublicIpAddress: true,
      blockDevices: [
        {
          deviceName: "/dev/xvda", // AL2023 root
          volume: ec2.BlockDeviceVolume.ebs(8, { volumeType: ec2.EbsDeviceVolumeType.GP3 }),
        },
      ],
    });
    new ec2.CfnVolumeAttachment(this, "DataAttach", {
      device: "/dev/sdf", // Nitro maps it to an NVMe disk; user-data finds it as the non-root device
      instanceId: instance.instanceId,
      volumeId: dataVolume.volumeId,
    });

    // Stable address so the CloudFront origin survives instance replacement.
    const eip = new ec2.CfnEIP(this, "Eip", { instanceId: instance.instanceId });
    // us-east-1 public DNS for the EIP: ec2-<dashed-ip>.compute-1.amazonaws.com
    const originDns = Fn.join("", [
      "ec2-",
      Fn.join("-", Fn.split(".", eip.attrPublicIp)),
      ".compute-1.amazonaws.com",
    ]);

    const distribution = new cloudfront.Distribution(this, "Cdn", {
      defaultBehavior: {
        origin: new origins.HttpOrigin(originDns, {
          protocolPolicy: cloudfront.OriginProtocolPolicy.HTTP_ONLY,
          httpPort: APP_PORT,
        }),
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
        allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
        cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
        // Forward everything except Host so the Authorization Bearer header reaches the server.
        originRequestPolicy: cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
      },
    });

    new CfnOutput(this, "Url", { value: `https://${distribution.distributionDomainName}` });
    new CfnOutput(this, "EipAddress", { value: eip.attrPublicIp });
    new CfnOutput(this, "ReplicaBucket", { value: replica.bucketName });
    new CfnOutput(this, "InstanceId", { value: instance.instanceId });
  }
}
