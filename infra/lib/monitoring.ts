// Shared observability plumbing. The design goal is ZERO cascade coupling: alarms + dashboards live
// in the stack that OWNS the resource they watch (so they refresh with every deploy — no staleness
// when the server instance is replaced, no cross-stack export locks that could wedge a redeploy), and
// the ONE shared resource — the SNS alarm topic — is created by ServerStack (always first in the
// cascade) and discovered elsewhere by ARN through SSM at deploy time (a value lookup, not a CDK
// dependency edge). WebStart already depends on ServerStack (apiUrl), and bin/ adds an explicit
// ClientLogs→Server edge, so the topic + param always exist before a consumer reads them.

import { CfnOutput, type Duration } from "aws-cdk-lib"
import * as cloudwatch from "aws-cdk-lib/aws-cloudwatch"
import * as cwActions from "aws-cdk-lib/aws-cloudwatch-actions"
import * as sns from "aws-cdk-lib/aws-sns"
import * as subscriptions from "aws-cdk-lib/aws-sns-subscriptions"
import * as ssm from "aws-cdk-lib/aws-ssm"
import type { Construct } from "constructs"

/** The alarm topic's ARN as an account fact — written by ServerStack, read by every other stack. */
export const ALARM_TOPIC_ARN_PARAM = "/vegify/monitor/alarm-topic-arn"

/** Custom-metric namespace the on-box CloudWatch agent publishes mem/disk under (EC2 emits neither). */
export const SERVER_METRIC_NS = "Vegify/Server"

/**
 * Create the alarm topic + its email subscription (ServerStack only) and publish the ARN to SSM.
 * The email subscription lands PendingConfirmation: AWS emails a one-time confirm link (once, not per
 * deploy) — until it's clicked, alarms fire but deliver nothing.
 */
export function createAlarmTopic(
  scope: Construct,
  alarmEmail: string
): sns.Topic {
  const topic = new sns.Topic(scope, "AlarmTopic", {
    displayName: "Vegify alarms"
  })
  topic.addSubscription(new subscriptions.EmailSubscription(alarmEmail))
  new ssm.StringParameter(scope, "AlarmTopicArnParam", {
    parameterName: ALARM_TOPIC_ARN_PARAM,
    stringValue: topic.topicArn,
    description:
      "SNS topic CloudWatch alarms publish to (created by VegifyServer, read account-wide)."
  })
  new CfnOutput(scope, "AlarmTopicArn", { value: topic.topicArn })
  return topic
}

/** Discover the alarm topic by ARN (deploy-time SSM lookup — no CDK cross-stack dependency). */
export function importAlarmTopic(scope: Construct, id: string): sns.ITopic {
  const arn = ssm.StringParameter.valueForStringParameter(
    scope,
    ALARM_TOPIC_ARN_PARAM
  )
  return sns.Topic.fromTopicArn(scope, id, arn)
}

/**
 * A CloudFront distribution metric with the CORRECT dimensions. The built-in
 * `Distribution.metricXxx()` helpers set only `DistributionId`, but CloudFront publishes to
 * `AWS/CloudFront` with BOTH `DistributionId` AND `Region=Global` — so the helper's metric matches
 * nothing and silently reads no data (an alarm on it never fires). CloudFront metrics live in
 * us-east-1; this deployment is us-east-1 throughout, so the alarm (stack region) and metric line up
 * without an explicit region. `4xx/5xxErrorRate` are percentages (Average); `Requests` is a Sum.
 */
export function cloudFrontMetric(
  _scope: Construct,
  distributionId: string,
  metricName: "Requests" | "4xxErrorRate" | "5xxErrorRate",
  period: Duration
): cloudwatch.Metric {
  return new cloudwatch.Metric({
    namespace: "AWS/CloudFront",
    metricName,
    dimensionsMap: { DistributionId: distributionId, Region: "Global" },
    statistic: metricName === "Requests" ? "Sum" : "Average",
    period
  })
}

/**
 * Notify the topic when an alarm fires. Deliberately NO OK action: the instance-keyed alarms (memory,
 * disk, CPU credit, status) re-enter INSUFFICIENT_DATA on every server deploy (instance replacement
 * changes the InstanceId dimension) and would email an "OK" as each re-settles — a recurring flurry on
 * every server-path merge. Break alerts are what matter; recovery is visible on the dashboards.
 */
export function notify(
  alarm: cloudwatch.Alarm,
  topic: sns.ITopic
): cloudwatch.Alarm {
  alarm.addAlarmAction(new cwActions.SnsAction(topic))
  return alarm
}
