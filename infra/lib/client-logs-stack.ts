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
 * Wiring: the Function URL is stable across code deploys, so set it once as VITE_CLIENT_LOG_URL in the
 * web build (see the IngestUrl output). authType NONE + CORS(POST) so the browser reaches it directly;
 * it's unauthenticated fire-and-forget telemetry. Hardening TODO (matches web-start's OAC TODO): a
 * per-IP rate limit / WAF in front, since the endpoint is public.
 */
export class ClientLogsStack extends Stack {
  readonly ingestUrl: string;

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

    const url = fn.addFunctionUrl({
      authType: lambda.FunctionUrlAuthType.NONE,
      cors: {
        allowedOrigins: ["*"],
        allowedMethods: [lambda.HttpMethod.POST],
        allowedHeaders: ["content-type"],
        maxAge: Duration.days(1),
      },
    });
    this.ingestUrl = url.url;

    new CfnOutput(this, "IngestUrl", {
      value: url.url,
      description: "Browser log ingestion endpoint — set as VITE_CLIENT_LOG_URL in the web build",
    });
    new CfnOutput(this, "LogGroupName", { value: logGroup.logGroupName });
  }
}
