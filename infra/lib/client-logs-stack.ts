import * as path from "node:path";
import { CfnOutput, Duration, RemovalPolicy, Stack, type StackProps } from "aws-cdk-lib";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as logs from "aws-cdk-lib/aws-logs";
import type { Construct } from "constructs";

const lambdaDir = path.join(import.meta.dirname, "../lambda/client-logs");

/**
 * VegifyClientLogs — a dedicated, scale-to-zero ingestion endpoint for BROWSER (client-side) logs.
 *
 * The web shell's non-blocking client logger (apps/web/src/client-log.ts) batches console + error
 * events and `navigator.sendBeacon()`s them to this Lambda's Function URL; the Lambda writes them to a
 * dedicated CloudWatch log group (/vegify/web-client) via PutLogEvents. Kept OUT of the web-start stack
 * on purpose: the working SSR deploy stays untouched, and client telemetry gets its own blast radius +
 * retention. The SSR shell's OWN server logs already go to its Lambda log group (this is only the browser).
 *
 * Wiring: the browser beacons a SAME-ORIGIN POST to /__ingest on the web-start CloudFront distribution,
 * which forwards here (VITE_CLIENT_LOG_URL=/__ingest in the web build). The Function URL is AWS_IAM and
 * locked to that distribution via OAC: web-start imports this FunctionUrl and calls
 * `FunctionUrlOrigin.withOriginAccessControl`. That's same-account cross-stack with NO cycle — the
 * invoke grant (a CfnPermission) lands in WEB-START, scoped to its own distributionId, so this stack
 * references nothing back. A direct, unsigned hit to the Function URL now gets 403.
 */
export class ClientLogsStack extends Stack {
  readonly ingestUrl: string;
  // Exposed so web-start can OAC-lock this Function URL (import it → withOriginAccessControl).
  readonly ingestFnUrl: lambda.FunctionUrl;

  constructor(scope: Construct, id: string, props?: StackProps) {
    super(scope, id, props);

    const logGroup = new logs.LogGroup(this, "WebClientLogs", {
      logGroupName: "/vegify/web-client",
      retention: logs.RetentionDays.TWO_WEEKS,
      removalPolicy: RemovalPolicy.DESTROY,
    });

    // 128 MB / ARM is plenty — the handler just shapes a batch and calls PutLogEvents. The runtime
    // provides the AWS SDK, so the asset is the single index.mjs (no bundling, no Docker).
    const fn = new lambda.Function(this, "IngestFn", {
      runtime: lambda.Runtime.NODEJS_22_X,
      architecture: lambda.Architecture.ARM_64,
      handler: "index.handler",
      code: lambda.Code.fromAsset(lambdaDir),
      memorySize: 128,
      timeout: Duration.seconds(10),
      environment: { LOG_GROUP_NAME: logGroup.logGroupName },
    });
    logGroup.grantWrite(fn); // logs:CreateLogStream + logs:PutLogEvents, scoped to this group only

    // AWS_IAM (not NONE): only the web-start CloudFront distribution may invoke this, via OAC (the
    // browser's same-origin POST /__ingest is signed + forwarded by CloudFront — see web-start-stack.ts).
    // No CORS: the browser never hits this URL cross-origin anymore (it hits same-origin /__ingest), so
    // the Function URL is purely a CloudFront origin; a direct unsigned hit gets 403.
    const url = fn.addFunctionUrl({ authType: lambda.FunctionUrlAuthType.AWS_IAM });
    this.ingestUrl = url.url;
    this.ingestFnUrl = url;

    new CfnOutput(this, "IngestUrl", {
      value: url.url,
      description: "Browser log ingestion endpoint — set as VITE_CLIENT_LOG_URL in the web build",
    });
    new CfnOutput(this, "LogGroupName", { value: logGroup.logGroupName });
  }
}
