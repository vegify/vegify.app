import * as path from "node:path";
import {
  CfnOutput,
  CustomResource,
  Duration,
  RemovalPolicy,
  Stack,
  type StackProps,
} from "aws-cdk-lib";
import * as cloudwatch from "aws-cdk-lib/aws-cloudwatch";
import * as iam from "aws-cdk-lib/aws-iam";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as logs from "aws-cdk-lib/aws-logs";
import type { Construct } from "constructs";

import { importAlarmTopic, notify } from "./monitoring.js";

const lambdaDir = path.join(import.meta.dirname, "../lambda/client-logs");
const secretLambdaDir = path.join(
  import.meta.dirname,
  "../lambda/origin-secret",
);

/** Parameter Store home of the origin-verify secret (SSM SecureString, generated on first deploy). */
const ORIGIN_SECRET_PARAM = "/vegify/origin-verify";

/**
 * VegifyClientLogs — a dedicated, scale-to-zero ingestion endpoint for BROWSER (client-side) logs.
 *
 * The web shell's non-blocking client logger (apps/web/src/client-log.ts) batches console + error
 * events and `navigator.sendBeacon()`s them to this Lambda's Function URL; the Lambda writes them to a
 * dedicated CloudWatch log group (/vegify/web-client) via PutLogEvents. Kept OUT of the web-start stack
 * on purpose: the working SSR deploy stays untouched, and client telemetry gets its own blast radius +
 * retention. The SSR shell's OWN server logs already go to its Lambda log group (this is only the browser).
 *
 * Wiring: the Function URL feeds web-start's /__ingest origin cross-stack (the browser beacons the
 * same-origin /__ingest path; VITE_CLIENT_LOG_URL bakes that path into the web build). authType NONE,
 * but the Lambda fails closed without the origin-verify secret and 403s a missing/mismatched
 * x-vegify-origin header — only CloudFront-forwarded traffic gets through. This stack is also the
 * HOME of that secret (generated in-account, below), because web-start already depends on it.
 */
export interface ClientLogsStackProps extends StackProps {
  /** Bumping this re-invokes the origin-secret custom resource so a manually rotated parameter
   *  (aws ssm put-parameter --overwrite) re-syncs into the CloudFront header + Lambda envs on the
   *  next deploy. Comes from $ORIGIN_VERIFY_ROTATE via deployConfig; default "0". */
  originSecretRotationNonce: string;
}

export class ClientLogsStack extends Stack {
  readonly ingestUrl: string;
  /** The origin-verify secret, resolved AT DEPLOY TIME from the in-account generated SSM SecureString.
   *  This stack is its home because the web stack already depends on this one (ingestUrl) — hosting it
   *  in web-start would create a dependency cycle. Consumed here (IngestFn env) and by WebStartStack
   *  (CloudFront custom header + SSR Lambda env). No human ever knows or transports the value. */
  readonly originSecret: string;

  constructor(scope: Construct, id: string, props: ClientLogsStackProps) {
    super(scope, id, props);

    // Get-or-create the secret parameter and hand its value to the template (see the handler file).
    const secretFn = new lambda.Function(this, "OriginSecretFn", {
      runtime: lambda.Runtime.NODEJS_22_X,
      architecture: lambda.Architecture.ARM_64,
      handler: "index.handler",
      code: lambda.Code.fromAsset(secretLambdaDir),
      memorySize: 128,
      timeout: Duration.seconds(30),
      // Explicit, short-retention log group — the implicit /aws/lambda/<name> never expires, and this
      // custom-resource Lambda runs only on deploys.
      logGroup: new logs.LogGroup(this, "OriginSecretFnLogs", {
        retention: logs.RetentionDays.ONE_WEEK,
        removalPolicy: RemovalPolicy.DESTROY,
      }),
    });
    secretFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ["ssm:GetParameter", "ssm:PutParameter"],
        resources: [
          `arn:aws:ssm:${this.region}:${this.account}:parameter${ORIGIN_SECRET_PARAM}`,
        ],
      }),
    );
    const secret = new CustomResource(this, "OriginSecret", {
      resourceType: "Custom::OriginVerifySecret",
      serviceToken: secretFn.functionArn,
      properties: {
        ParameterName: ORIGIN_SECRET_PARAM,
        RotationNonce: props.originSecretRotationNonce,
      },
    });
    this.originSecret = secret.getAttString("Value");

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
      environment: {
        LOG_GROUP_NAME: logGroup.logGroupName,
        ORIGIN_SECRET: this.originSecret,
      },
      // The function's OWN execution logs (distinct from the browser-log group it writes into).
      logGroup: new logs.LogGroup(this, "IngestFnLogs", {
        retention: logs.RetentionDays.ONE_MONTH,
        removalPolicy: RemovalPolicy.DESTROY,
      }),
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
      description:
        "Browser log ingestion endpoint — set as VITE_CLIENT_LOG_URL in the web build",
    });
    new CfnOutput(this, "LogGroupName", { value: logGroup.logGroupName });

    // Alarm on ingest failures — a low-severity signal (dropped browser logs), but a spike usually
    // means the origin-secret check is rejecting everything or the handler is misconfigured. The
    // shared alarm topic is discovered by ARN from SSM (ServerStack owns it; bin/ orders us after it).
    notify(
      new cloudwatch.Alarm(this, "IngestErrorsAlarm", {
        alarmDescription:
          "Client-log ingest Lambda erroring — browser logs are being dropped.",
        metric: fn.metricErrors({ period: Duration.minutes(5) }),
        threshold: 10,
        evaluationPeriods: 3,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      importAlarmTopic(this, "AlarmTopic"),
    );
  }
}
