import * as path from "node:path";
import {
  CfnOutput,
  Duration,
  Fn,
  RemovalPolicy,
  Size,
  Stack,
  type StackProps,
  Tags,
} from "aws-cdk-lib";
import * as acm from "aws-cdk-lib/aws-certificatemanager";
import * as cloudfront from "aws-cdk-lib/aws-cloudfront";
import * as origins from "aws-cdk-lib/aws-cloudfront-origins";
import * as cloudwatch from "aws-cdk-lib/aws-cloudwatch";
import * as ec2 from "aws-cdk-lib/aws-ec2";
import * as iam from "aws-cdk-lib/aws-iam";
import * as logs from "aws-cdk-lib/aws-logs";
import * as route53 from "aws-cdk-lib/aws-route53";
import * as targets from "aws-cdk-lib/aws-route53-targets";
import * as s3 from "aws-cdk-lib/aws-s3";
import { Asset } from "aws-cdk-lib/aws-s3-assets";
import * as ssm from "aws-cdk-lib/aws-ssm";
import type { Construct } from "constructs";
import {
  cloudFrontMetric,
  createAlarmTopic,
  notify,
  SERVER_METRIC_NS,
} from "./monitoring.js";
import { resolveZone } from "./zone.js";

/** The CloudWatch log group the on-box agent ships the server's stdout/stderr to. */
const SERVER_LOG_GROUP = "/vegify/server";

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
  /** Public site origin (https://<primary domain>) — the base of links in transactional email. The
   *  server REFUSES email sends without it (no fallback: a wrong default would silently mail links
   *  pointing at another site), so it lands in the instance's systemd env. */
  publicUrl: string;
  /** From: header for transactional mail (default derived: `Vegify <hello@<email domain>>`). Required
   *  to send, same fail-closed rule as publicUrl. */
  emailFrom: string;
  /** SES identity domain the instance may send as (scopes the ses:SendEmail grant). */
  emailDomain: string;
  /** Signups gate (SSM decision signups-open / VEGIFY_SIGNUPS_OPEN; default closed). Lands in the
   *  systemd env; the server rejects signups unless it's "1". */
  signupsOpen: boolean;
  /** Admin email allowlist (SSM admin-emails / VEGIFY_ADMIN_EMAILS) — accounts allowed to invite new
   *  users while signups stay closed. Empty by default. */
  adminEmails: string;
  /** Site domains (primary first) — the API's own hostname is DERIVED: `api.<primary>`. Same
   *  decision the web stack consumes; no separate api-domain knob to configure or drift. */
  domainNames: string[];
  /** False on a fresh clone (placeholder domains): the distribution then keeps only its default
   *  *.cloudfront.net name and no cert/records are created — zero-env synth stays green. */
  domainsConfigured: boolean;
  hostedZoneIdOverride?: string;
  /** Where CloudWatch alarms email (derived hello@<domain> unless overridden). This stack owns the
   *  shared SNS alarm topic; the web/log stacks discover it by ARN via SSM. */
  alarmEmail: string;
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
  /** The backend's public origin (its CloudFront URL) — wired cross-stack into the web shell, so the
   *  web ↔ backend coupling is never hand-carried through env/secrets. */
  readonly apiUrl: string;

  constructor(scope: Construct, id: string, props: ServerStackProps) {
    super(scope, id, props);
    const {
      vpc,
      publicUrl,
      emailFrom,
      emailDomain,
      signupsOpen,
      adminEmails,
      domainNames,
      domainsConfigured,
      alarmEmail,
    } = props;

    // Durable WAL replica + the restore source on a fresh/replaced instance.
    // Reference DATA (the USDA catalog artifact, future imports) — data lives in S3, not the repo.
    // Written by the operator (`just usda-data && just usda-upload`; the bucket name is published to
    // SSM below), read by the server at boot. RETAIN: it's data.
    const data = new s3.Bucket(this, "Data", {
      removalPolicy: RemovalPolicy.RETAIN,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
    });

    // USER MEDIA (recipe photos, avatars): uploaded by clients via server-issued presigned PUTs,
    // served through THIS stack's CloudFront at /media/* (no new distribution). RETAIN: user content.
    // CORS: presigned PUTs come straight from browsers/webviews; '*' is safe here (no credentials
    // ride a presigned URL — the signature IS the authorization).
    const media = new s3.Bucket(this, "Media", {
      removalPolicy: RemovalPolicy.RETAIN,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      cors: [
        {
          allowedMethods: [s3.HttpMethods.PUT, s3.HttpMethods.GET],
          allowedOrigins: ["*"],
          allowedHeaders: ["*"],
          maxAge: 3600,
        },
      ],
    });

    const replica = new s3.Bucket(this, "Replica", {
      removalPolicy: RemovalPolicy.RETAIN,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
    });

    // Shipped artifacts (CI builds these into infra/assets/; a placeholder binary is fine for synth).
    const serverBin = new Asset(this, "ServerBin", {
      path: path.join(assetsDir, "vegify-server"),
    });
    const seedDb = new Asset(this, "SeedDb", {
      path: path.join(assetsDir, "seed.db"),
    });

    // The server's stdout/stderr ships here via the on-box CloudWatch agent. RETAIN so operational
    // history survives a stack teardown; one month is plenty for a solo service's debugging window.
    const serverLogGroup = new logs.LogGroup(this, "ServerLogGroup", {
      logGroupName: SERVER_LOG_GROUP,
      retention: logs.RetentionDays.ONE_MONTH,
      removalPolicy: RemovalPolicy.RETAIN,
    });

    const role = new iam.Role(this, "InstanceRole", {
      assumedBy: new iam.ServicePrincipal("ec2.amazonaws.com"),
      managedPolicies: [
        // SSM session access for debugging — no SSH, no inbound 22.
        iam.ManagedPolicy.fromAwsManagedPolicyName(
          "AmazonSSMManagedInstanceCore",
        ),
        // Lets the on-box agent create log streams + put events and publish the mem/disk metrics.
        iam.ManagedPolicy.fromAwsManagedPolicyName(
          "CloudWatchAgentServerPolicy",
        ),
      ],
    });
    serverBin.grantRead(role);
    seedDb.grantRead(role);
    replica.grantReadWrite(role); // litestream replicate + restore
    data.grantRead(role); // boot-time catalog ingest reads the artifact
    media.grantPut(role); // the server PRESIGNS client uploads (the clients PUT directly to S3)
    // The instance self-attaches its data volume in user-data (robust across replacement — a CFN
    // VolumeAttachment deadlocks a replace, since the new attach can't precede the old detach on one
    // volume). DescribeVolumes is account-wide (no resource-level support); tighten attach/detach later.
    role.addToPolicy(
      new iam.PolicyStatement({
        actions: ["ec2:DescribeVolumes"],
        resources: ["*"],
      }),
    );
    role.addToPolicy(
      new iam.PolicyStatement({
        actions: ["ec2:AttachVolume", "ec2:DetachVolume"],
        resources: ["*"],
      }),
    );
    // Transactional email (password reset, A5) via SES — send-only, scoped to the deployment's own
    // verified domain identity (VegifyEmail). Parameterized: a self-host's grant follows ITS domain.
    role.addToPolicy(
      new iam.PolicyStatement({
        actions: ["ses:SendEmail"],
        resources: [
          `arn:aws:ses:${this.region}:${this.account}:identity/${emailDomain}`,
        ],
      }),
    );

    const sg = new ec2.SecurityGroup(this, "Sg", {
      vpc,
      allowAllOutbound: true,
    });
    sg.addIngressRule(
      ec2.Peer.prefixList(CLOUDFRONT_ORIGIN_PL),
      ec2.Port.tcp(APP_PORT),
      "CloudFront origin-facing to the app port",
    );

    const userData = ec2.UserData.forLinux();
    userData.addCommands(
      "set -eux",
      `export AWS_DEFAULT_REGION=${this.region}`,
      "dnf install -y tar gzip",
      // Self-attach the dedicated data volume (found by tag) to THIS instance — robust across instance
      // replacement (force-detach from any prior holder, then attach). No CFN VolumeAttachment.
      "TOKEN=$(curl -s -X PUT http://169.254.169.254/latest/api/token -H 'X-aws-ec2-metadata-token-ttl-seconds: 300' || true)",
      'IID=$(curl -s -H "X-aws-ec2-metadata-token: $TOKEN" http://169.254.169.254/latest/meta-data/instance-id)',
      "VOL=$(aws ec2 describe-volumes --filters Name=tag:vegify:role,Values=data --query 'Volumes[0].VolumeId' --output text)",
      "for i in $(seq 1 50); do",
      "  OWNER=$(aws ec2 describe-volumes --volume-ids \"$VOL\" --query 'Volumes[0].Attachments[0].InstanceId' --output text 2>/dev/null || echo None)",
      '  if [ "$OWNER" = "$IID" ]; then break; fi',
      '  if [ "$OWNER" = "None" ] || [ -z "$OWNER" ]; then aws ec2 attach-volume --volume-id "$VOL" --instance-id "$IID" --device /dev/sdf 2>/dev/null || true; else aws ec2 detach-volume --volume-id "$VOL" --force 2>/dev/null || true; fi',
      "  sleep 6",
      "done",
      // Mount the dedicated EBS data volume at /data — format on first boot, preserve on reattach (the
      // volume is RETAIN → the DB survives instance replacement). Find the non-root NVMe disk by name.
      "for i in $(seq 1 60); do [ $(lsblk -dno NAME | grep -c nvme) -ge 2 ] && break || sleep 2; done",
      "ROOTDISK=/dev/$(lsblk -no PKNAME $(findmnt -no SOURCE /) | head -1)",
      // Restrict to NVMe disks — excludes AL2023's zram0 swap device, which lsblk also reports as a disk.
      'DATADEV=$(lsblk -dpno NAME,TYPE | awk \'$2=="disk"{print $1}\' | grep /dev/nvme | grep -vx "$ROOTDISK" | head -1)',
      'blkid "$DATADEV" || mkfs.ext4 -L vegifydata "$DATADEV"',
      "mkdir -p /data",
      "mount LABEL=vegifydata /data",
      "grep -q vegifydata /etc/fstab || echo 'LABEL=vegifydata /data ext4 defaults,nofail 0 2' >> /etc/fstab",
      // One-shot prod-data migration: if a snapshot is staged in S3, adopt it as the live DB (replacing
      // whatever's on the EBS), clear the old litestream generations so replication restarts on the new
      // DB, then consume the staged object so future boots skip this. Idempotent + self-disabling.
      `if aws s3 ls s3://${replica.bucketName}/migration/vegify.db >/dev/null 2>&1; then`,
      "  systemctl stop vegify 2>/dev/null || true",
      "  rm -f /data/vegify.db /data/vegify.db-wal /data/vegify.db-shm",
      `  aws s3 rm s3://${replica.bucketName}/vegify.db/ --recursive || true`,
      `  aws s3 cp s3://${replica.bucketName}/migration/vegify.db /data/vegify.db`,
      `  aws s3 rm s3://${replica.bucketName}/migration/vegify.db`,
      "fi",
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
      // Restore from S3 if a replica exists; otherwise lay down the baked seed. NOT -if-replica-exists:
      // with no replica that's a no-op SUCCESS, so the `|| seed` fallback would never fire (empty DB).
      "if [ ! -f /data/vegify.db ]; then",
      "  litestream restore -config /etc/litestream.yml /data/vegify.db || \\",
      `    aws s3 cp s3://${seedDb.s3BucketName}/${seedDb.s3ObjectKey} /data/vegify.db`,
      "fi",
      // systemd: litestream supervises the server (-exec) so every write is captured to S3.
      // The server's stdout/stderr go to a file (not journald) so the CloudWatch agent below can ship
      // them; `journalctl -u vegify` still shows the unit's lifecycle, and SSM session is on for debug.
      "mkdir -p /var/log/vegify",
      "cat >/etc/systemd/system/vegify.service <<EOF",
      "[Unit]",
      "Description=vegify-server (litestream-replicated)",
      "After=network-online.target",
      "Wants=network-online.target",
      "[Service]",
      "Environment=DATABASE_PATH=/data/vegify.db",
      `Environment=PORT=${APP_PORT}`,
      // Email config — REQUIRED by the server's fail-closed send path (vegify-config has no fallback
      // for either: a default domain would silently mail links pointing at someone else's site).
      `Environment=VEGIFY_PUBLIC_URL=${publicUrl}`,
      `Environment="VEGIFY_EMAIL_FROM=${emailFrom}"`,
      `Environment=VEGIFY_SES_REGION=${this.region}`,
      `Environment=VEGIFY_SIGNUPS_OPEN=${signupsOpen ? "1" : "0"}`,
      `Environment=VEGIFY_ADMIN_EMAILS=${adminEmails}`,
      // Reference-data bucket: the boot ingest fetches catalog/usda-plants.json.gz from here
      // (marker-gated; a missing object logs a warning and the server serves without the catalog).
      `Environment=VEGIFY_DATA_BUCKET=${data.bucketName}`,
      // Media bucket for presigned upload URLs (photos/avatars); served at <api>/media/*.
      `Environment=VEGIFY_MEDIA_BUCKET=${media.bucketName}`,
      "ExecStart=/usr/local/bin/litestream replicate -config /etc/litestream.yml -exec /usr/local/bin/vegify-server",
      "Restart=always",
      "RestartSec=2",
      "StandardOutput=append:/var/log/vegify/server.log",
      "StandardError=append:/var/log/vegify/server.log",
      "[Install]",
      "WantedBy=multi-user.target",
      "EOF",
      "systemctl daemon-reload",
      "systemctl enable --now vegify.service",
      // Cap the log file so it can't fill the 8 GiB root: daily rotation, 7 days kept, copytruncate
      // (the server holds the file open — copytruncate rotates without a restart or reopen).
      "cat >/etc/logrotate.d/vegify <<EOF",
      "/var/log/vegify/server.log {",
      "  daily",
      "  rotate 7",
      "  compress",
      "  missingok",
      "  notifempty",
      "  copytruncate",
      "}",
      "EOF",
      // On-box CloudWatch agent: tail the server log → the /vegify/server group, and publish mem +
      // disk (EC2 emits neither) tagged with THIS instance id so the dashboard + alarms find them.
      // Tolerant (|| true): an agent/repo hiccup must never block the server or fail the deploy gate —
      // missing telemetry is recoverable, a down backend is not.
      "dnf install -y amazon-cloudwatch-agent || true",
      "mkdir -p /opt/aws/amazon-cloudwatch-agent/etc",
      "cat >/opt/aws/amazon-cloudwatch-agent/etc/vegify.json <<CWEOF",
      "{",
      '  "agent": { "metrics_collection_interval": 60, "run_as_user": "root" },',
      '  "logs": { "logs_collected": { "files": { "collect_list": [',
      `    { "file_path": "/var/log/vegify/server.log", "log_group_name": "${SERVER_LOG_GROUP}", "log_stream_name": "{instance_id}" }`,
      "  ] } } },",
      `  "metrics": { "namespace": "${SERVER_METRIC_NS}", "append_dimensions": { "InstanceId": "$IID" },`,
      // Roll the disk metric up to {InstanceId, path} so the alarm can reference it WITHOUT guessing the
      // per-boot device/fstype dimensions the agent otherwise tags (nvme names + xfs/ext4 vary). mem
      // carries only InstanceId. Both aggregations are emitted alongside the base metrics.
      '    "aggregation_dimensions": [["InstanceId"], ["InstanceId", "path"]],',
      '    "metrics_collected": {',
      '      "mem": { "measurement": ["mem_used_percent"] },',
      '      "disk": { "measurement": ["used_percent"], "resources": ["/", "/data"] }',
      "    } }",
      "}",
      "CWEOF",
      "/opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl -a fetch-config -m ec2 -s -c file:/opt/aws/amazon-cloudwatch-agent/etc/vegify.json || true",
    );

    // Dedicated gp3 EBS data volume for the SQLite DB — RETAIN so it (and the data) survives instance
    // replacement; pinned to the instance's AZ. Litestream→S3 is the off-box replica/restore on top.
    const primarySubnet = vpc.publicSubnets[0];
    if (!primarySubnet) throw new Error("vpc has no public subnets");
    const dataVolume = new ec2.Volume(this, "DataVolume", {
      availabilityZone: primarySubnet.availabilityZone,
      size: Size.gibibytes(10),
      volumeType: ec2.EbsDeviceVolumeType.GP3,
      removalPolicy: RemovalPolicy.RETAIN,
    });
    Tags.of(dataVolume).add("vegify:role", "data"); // user-data finds + self-attaches it by this tag

    const instance = new ec2.Instance(this, "Server", {
      vpc,
      vpcSubnets: { subnets: [primarySubnet] }, // pin the AZ to match the data volume
      instanceType: ec2.InstanceType.of(
        ec2.InstanceClass.T4G,
        ec2.InstanceSize.NANO,
      ),
      machineImage: ec2.MachineImage.latestAmazonLinux2023({
        cpuType: ec2.AmazonLinuxCpuType.ARM_64,
      }),
      securityGroup: sg,
      role,
      userData,
      associatePublicIpAddress: true,
      userDataCausesReplacement: true, // a user-data/binary change replaces the instance (it re-attaches the volume)
      blockDevices: [
        {
          deviceName: "/dev/xvda", // AL2023 root
          volume: ec2.BlockDeviceVolume.ebs(8, {
            volumeType: ec2.EbsDeviceVolumeType.GP3,
          }),
        },
      ],
    });

    // Stable address so the CloudFront origin survives instance replacement.
    const eip = new ec2.CfnEIP(this, "Eip", {
      instanceId: instance.instanceId,
    });
    // us-east-1 public DNS for the EIP: ec2-<dashed-ip>.compute-1.amazonaws.com
    const originDns = Fn.join("", [
      "ec2-",
      Fn.join("-", Fn.split(".", eip.attrPublicIp)),
      ".compute-1.amazonaws.com",
    ]);

    // api.<primary domain> — the backend's stable public name (the dynamically assigned
    // *.cloudfront.net domain stays alive as an alias-less default, so everything already pointing
    // at it keeps working; web + desktop adopt the stable name on their next deploys). The cert is
    // DNS-validated in the site zone, issued automatically on first deploy.
    const primaryDomain = domainNames[0];
    if (!primaryDomain) throw new Error("domain names are empty");
    const apiDomain = `api.${primaryDomain}`;
    const zone = resolveZone(this, "Zone", {
      zoneName: primaryDomain,
      configured: domainsConfigured,
      overrideZoneId: props.hostedZoneIdOverride,
    });
    const certificate = domainsConfigured
      ? new acm.Certificate(this, "ApiCert", {
          domainName: apiDomain,
          validation: acm.CertificateValidation.fromDns(zone),
        })
      : undefined;

    const distribution = new cloudfront.Distribution(this, "Cdn", {
      ...(certificate ? { domainNames: [apiDomain], certificate } : {}),
      additionalBehaviors: {
        // User media, cached hard at the edge (immutable keys — a re-upload mints a new key).
        "/media/*": {
          origin: origins.S3BucketOrigin.withOriginAccessControl(media),
          viewerProtocolPolicy:
            cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
        },
      },
      defaultBehavior: {
        origin: new origins.HttpOrigin(originDns, {
          protocolPolicy: cloudfront.OriginProtocolPolicy.HTTP_ONLY,
          httpPort: APP_PORT,
        }),
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
        allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
        cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
        // Forward everything except Host so the Authorization Bearer header reaches the server.
        originRequestPolicy:
          cloudfront.OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
      },
    });

    if (domainsConfigured) {
      const aliasTarget = route53.RecordTarget.fromAlias(
        new targets.CloudFrontTarget(distribution),
      );
      new route53.ARecord(this, "ApiA", {
        zone,
        recordName: "api",
        target: aliasTarget,
      });
      new route53.AaaaRecord(this, "ApiAaaa", {
        zone,
        recordName: "api",
        target: aliasTarget,
      });
    }

    this.apiUrl = domainsConfigured
      ? `https://${apiDomain}`
      : `https://${distribution.distributionDomainName}`;

    // Publish the backend origin as an account fact: publish-desktop reads it to bake VEGIFY_API_URL
    // into the shipped binary (replacing the repository secret), and anything else in the account can
    // discover the API the same way.
    // The data bucket as an account fact: `just usda-upload` resolves it from here — the operator
    // never types a bucket name.
    new ssm.StringParameter(this, "DataBucketParam", {
      parameterName: "/vegify/deploy/data-bucket",
      stringValue: data.bucketName,
    });

    new ssm.StringParameter(this, "ApiUrlParam", {
      parameterName: "/vegify/deploy/api-url",
      stringValue: this.apiUrl,
    });

    new CfnOutput(this, "Url", { value: this.apiUrl });
    new CfnOutput(this, "EipAddress", { value: eip.attrPublicIp });
    new CfnOutput(this, "ReplicaBucket", { value: replica.bucketName });
    new CfnOutput(this, "InstanceId", { value: instance.instanceId });

    // ── Observability ────────────────────────────────────────────────────────────────────────────
    // This stack owns the shared alarm topic (created first in the cascade); the web + log stacks
    // discover it by ARN via SSM. Every alarm + the dashboard here reference THIS stack's own
    // constructs (instance, api distribution, server log group), so they refresh on each deploy and
    // never point at a replaced instance. Budget: these six alarms sit within the always-free tier.
    const alarmTopic = createAlarmTopic(this, alarmEmail);

    const instanceDim = { InstanceId: instance.instanceId };
    const ec2Metric = (
      metricName: string,
      extra?: Partial<cloudwatch.MetricProps>,
    ) =>
      new cloudwatch.Metric({
        namespace: "AWS/EC2",
        metricName,
        dimensionsMap: instanceDim,
        period: Duration.minutes(5),
        ...extra,
      });
    // mem/disk come from the on-box agent (its custom namespace), not AWS/EC2.
    const agentMetric = (metricName: string, dims: Record<string, string>) =>
      new cloudwatch.Metric({
        namespace: SERVER_METRIC_NS,
        metricName,
        dimensionsMap: dims,
        period: Duration.minutes(5),
        statistic: "Average",
      });
    const memUsed = agentMetric("mem_used_percent", instanceDim);
    // Reference the {InstanceId, path} aggregation the agent config emits (not the raw metric, whose
    // device/fstype dimensions vary per boot). Root and the DB volume are watched separately — either
    // filling is bad, for different reasons (system breakage vs. the DB going read-only).
    const rootDisk = agentMetric("disk_used_percent", {
      ...instanceDim,
      path: "/",
    });
    const dataDisk = agentMetric("disk_used_percent", {
      ...instanceDim,
      path: "/data",
    });

    // A metric filter turns "ERROR" server log lines into a countable metric we can alarm on. The
    // Rust logs are `LEVEL message…`; the pattern matches the ERROR level token.
    const errorMetric = new cloudwatch.Metric({
      namespace: SERVER_METRIC_NS,
      metricName: "ServerErrorLogs",
      statistic: "Sum",
      period: Duration.minutes(5),
    });
    new logs.MetricFilter(this, "ServerErrorFilter", {
      logGroup: serverLogGroup,
      filterPattern: logs.FilterPattern.anyTerm("ERROR"),
      metricNamespace: SERVER_METRIC_NS,
      metricName: "ServerErrorLogs",
      metricValue: "1",
      defaultValue: 0,
    });

    // Abuse signal: the server logs a `rate_limited` warn every time it rejects a request (auth-endpoint
    // limits + the general per-IP cap). A sustained burst = credential stuffing, scraping, or a
    // misconfigured client — the visibility the read-endpoint limits alone don't give.
    const rateLimitMetric = new cloudwatch.Metric({
      namespace: SERVER_METRIC_NS,
      metricName: "RateLimitHits",
      statistic: "Sum",
      period: Duration.minutes(5),
    });
    new logs.MetricFilter(this, "RateLimitFilter", {
      logGroup: serverLogGroup,
      filterPattern: logs.FilterPattern.anyTerm("rate_limited"),
      metricNamespace: SERVER_METRIC_NS,
      metricName: "RateLimitHits",
      metricValue: "1",
      defaultValue: 0,
    });

    const alarms: cloudwatch.Alarm[] = [
      new cloudwatch.Alarm(this, "InstanceStatusAlarm", {
        alarmDescription:
          "EC2 instance/system status check failing — the backend host is unhealthy.",
        metric: ec2Metric("StatusCheckFailed", { statistic: "Maximum" }),
        threshold: 1,
        evaluationPeriods: 2,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_OR_EQUAL_TO_THRESHOLD,
        // MISSING, not BREACHING: a running-but-unhealthy instance emits StatusCheckFailed=1 (a real
        // datapoint that fires this regardless), whereas a just-replaced instance emits NO data for its
        // first ~10 min — BREACHING turned that into a false alarm on every deploy. A truly dead +
        // unreplaced instance still surfaces via the site's CloudFront 5xx alarm.
        treatMissingData: cloudwatch.TreatMissingData.MISSING,
      }),
      // SUSTAINED high CPU is the real overload signal — NOT CPUCreditBalance. This t4g.nano runs in
      // `unlimited` credit mode (the T-family default), where a 0 credit balance means "bursting on
      // surplus credits," not "throttled" — so a low-credit alarm false-fires on every freshly
      // launched instance (a new box starts near 0 and the boot work — litestream restore, USDA
      // ingest, the CW-agent install — spends what little it has) while the site stays perfectly
      // healthy. 20 min of >85% average CPU catches genuine overload in either credit mode and rides
      // out brief boot spikes.
      new cloudwatch.Alarm(this, "CpuHighAlarm", {
        alarmDescription:
          "Server CPU > 85% sustained (20 min) — genuine overload (credit mode aside).",
        metric: ec2Metric("CPUUtilization", { statistic: "Average" }),
        threshold: 85,
        evaluationPeriods: 4,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      new cloudwatch.Alarm(this, "MemoryAlarm", {
        alarmDescription:
          "Server memory > 85% — the 512 MB nano is under pressure (OOM risk).",
        metric: memUsed,
        threshold: 85,
        evaluationPeriods: 3,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      new cloudwatch.Alarm(this, "RootDiskAlarm", {
        alarmDescription:
          "Root disk > 85% — logs/system filling the 8 GiB root volume.",
        metric: rootDisk,
        threshold: 85,
        evaluationPeriods: 1,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      new cloudwatch.Alarm(this, "DataDiskAlarm", {
        alarmDescription:
          "DB volume (/data) > 85% — SQLite + litestream WAL are filling the EBS volume.",
        metric: dataDisk,
        threshold: 85,
        evaluationPeriods: 1,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      new cloudwatch.Alarm(this, "ApiCloudFront5xxAlarm", {
        alarmDescription:
          "API CloudFront 5xx rate > 5% — the backend is erroring or unreachable.",
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
      new cloudwatch.Alarm(this, "ServerErrorLogAlarm", {
        alarmDescription:
          "Server logged ERROR lines — application-level failures (email, DB, panics).",
        metric: errorMetric,
        threshold: 5,
        evaluationPeriods: 1,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      new cloudwatch.Alarm(this, "RateLimitAbuseAlarm", {
        alarmDescription:
          "Sustained rate-limit rejections (>100/5min) — credential stuffing, scraping, or a misconfigured client.",
        metric: rateLimitMetric,
        threshold: 100,
        evaluationPeriods: 1,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
    ];
    for (const a of alarms) notify(a, alarmTopic);

    new cloudwatch.Dashboard(this, "ServerDashboard", {
      dashboardName: "Vegify-Server",
      widgets: [
        [
          new cloudwatch.GraphWidget({
            title: "CPU %",
            left: [ec2Metric("CPUUtilization", { statistic: "Average" })],
            width: 8,
          }),
          new cloudwatch.GraphWidget({
            title: "CPU credit balance",
            left: [ec2Metric("CPUCreditBalance", { statistic: "Minimum" })],
            width: 8,
          }),
          new cloudwatch.GraphWidget({
            title: "Memory % / Disk %",
            left: [memUsed, rootDisk, dataDisk],
            width: 8,
          }),
        ],
        [
          new cloudwatch.GraphWidget({
            title: "API requests",
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
            title: "API error rates %",
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
            title: "Server ERROR log lines",
            left: [errorMetric],
            width: 8,
          }),
        ],
        [
          new cloudwatch.GraphWidget({
            title: "Rate-limit rejections (abuse signal)",
            left: [rateLimitMetric],
            width: 24,
          }),
        ],
      ],
    });
  }
}
