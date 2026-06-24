import {
  CfnOutput,
  Duration,
  RemovalPolicy,
  SecretValue,
  Stack,
  type StackProps,
} from "aws-cdk-lib";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as iam from "aws-cdk-lib/aws-iam";
import * as secrets from "aws-cdk-lib/aws-secretsmanager";
import type { Construct } from "constructs";

/**
 * S3 changeset-blob store for the desktop local-first sync — the scale-to-zero transport
 * (the desktop PUTs/GETs/compacts changeset objects; no always-on server, ~$0 idle, mandate-safe).
 *
 * A dedicated least-privilege IAM user gives the client scoped credentials (RW this bucket only,
 * not the operator's own keys). The access key + secret are surfaced via Secrets Manager so they
 * never land in a CloudFormation output in plaintext. Wire the desktop from the outputs:
 *   SYNC_S3_BUCKET=<BucketName>  SYNC_S3_REGION=<Region>
 *   SYNC_S3_ACCESS_KEY/SYNC_S3_SECRET_KEY=  (from `aws secretsmanager get-secret-value --secret-id vegify/sync-s3`)
 *
 * Production-distribution note: shipping these keys in a desktop binary is unsafe — for many
 * clients, front the bucket with a small scale-to-zero Lambda (PUT/GET/LIST/compact) the desktop
 * calls instead, so no AWS creds live on-device. Single-operator dev/self-host can use these keys.
 */
export class SyncStack extends Stack {
  readonly bucket: s3.Bucket;

  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    this.bucket = new s3.Bucket(this, "ChangesetBlobs", {
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      encryption: s3.BucketEncryption.S3_MANAGED,
      enforceSSL: true,
      // changesets are deleted/compacted by the client; just clean up abandoned multipart uploads
      lifecycleRules: [{ abortIncompleteMultipartUploadAfter: Duration.days(7) }],
      // holds the shared sync data — keep it if the stack is torn down (not auto-deleted)
      removalPolicy: RemovalPolicy.RETAIN,
    });

    // Dedicated least-privilege sync client (the desktop): read/write THIS bucket and nothing else.
    const user = new iam.User(this, "SyncClient", { userName: "vegify-sync" });
    this.bucket.grantReadWrite(user); // s3:GetObject/PutObject/DeleteObject + ListBucket on this bucket

    const accessKey = new iam.CfnAccessKey(this, "SyncClientKey", { userName: user.userName });

    new secrets.Secret(this, "SyncClientCreds", {
      secretName: "vegify/sync-s3",
      description: "Least-privilege S3 sync client credentials for the Vegify desktop app.",
      secretObjectValue: {
        SYNC_S3_ACCESS_KEY: SecretValue.unsafePlainText(accessKey.ref),
        SYNC_S3_SECRET_KEY: SecretValue.unsafePlainText(accessKey.attrSecretAccessKey),
      },
    });

    new CfnOutput(this, "BucketName", { value: this.bucket.bucketName });
    new CfnOutput(this, "Region", { value: this.region });
    new CfnOutput(this, "CredsSecret", {
      value: "vegify/sync-s3",
      description: "Secrets Manager secret holding SYNC_S3_ACCESS_KEY / SYNC_S3_SECRET_KEY",
    });
  }
}
