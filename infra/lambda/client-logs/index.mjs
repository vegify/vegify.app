// VegifyClientLogs ingestion — receives batched BROWSER log events (POST via the Function URL) and
// writes them to the dedicated /vegify/web-client CloudWatch group. The Node 22 Lambda runtime bundles
// the AWS SDK v3 (incl. @aws-sdk/client-cloudwatch-logs), so this asset is just this one file — no
// node_modules to ship. Fire-and-forget telemetry: it never trusts client input beyond shape/size caps.
import {
  CloudWatchLogsClient,
  CreateLogStreamCommand,
  PutLogEventsCommand
} from "@aws-sdk/client-cloudwatch-logs"

const client = new CloudWatchLogsClient({})
const LOG_GROUP = process.env.LOG_GROUP_NAME
const MAX_EVENTS = 1000 // cap a single batch so a hostile client can't blow up one PutLogEvents call
// Origin-verify: CloudFront injects x-vegify-origin (set by the CDK from the origin-verify secret) on the
// /__ingest forward; reject requests lacking it (a direct hit to the public Function URL). FAIL CLOSED on
// Lambda when the secret is unset — a deployment missing it is a misconfiguration, not permission to run
// the endpoint unprotected (same rule as the web adapter, apps/web/aws/lambda-handler.mjs).
const ORIGIN_SECRET = process.env.ORIGIN_SECRET
const ON_LAMBDA = Boolean(process.env.AWS_LAMBDA_FUNCTION_NAME)

const reply = (statusCode) => ({
  statusCode,
  headers: { "content-type": "application/json" },
  body: ""
})

export const handler = async (event) => {
  if (ON_LAMBDA && !ORIGIN_SECRET) return reply(503)
  if (ORIGIN_SECRET && event.headers?.["x-vegify-origin"] !== ORIGIN_SECRET)
    return reply(403)
  // Function URL payload (HTTP API v2 shape). Only accept POSTs; the browser logger never GETs.
  if (event?.requestContext?.http?.method !== "POST") return reply(405)

  let raw = event.body ?? ""
  if (event.isBase64Encoded) raw = Buffer.from(raw, "base64").toString("utf8")
  let payload
  try {
    payload = JSON.parse(raw)
  } catch {
    return reply(400)
  }

  const events = Array.isArray(payload?.events)
    ? payload.events.slice(0, MAX_EVENTS)
    : []
  if (events.length === 0) return reply(204)

  const logEvents = events
    .map((e) => ({
      timestamp: Number(e?.ts) || Date.now(),
      message: JSON.stringify({
        level: e?.level ?? "info",
        msg: e?.msg ?? "",
        url: e?.url,
        ctx: e?.ctx,
        ua: event.headers?.["user-agent"],
        ip: event.headers?.["x-forwarded-for"]
      })
    }))
    .sort((a, b) => a.timestamp - b.timestamp) // PutLogEvents requires events in chronological order

  // One stream per UTC day + client session: a session's events stay together and streams stay small.
  // (PutLogEvents no longer needs a sequenceToken, so appending to an existing stream just works.)
  const day = new Date().toISOString().slice(0, 10)
  const session = String(
    payload.session ?? Math.random().toString(36).slice(2, 10)
  ).replace(/[^a-zA-Z0-9_-]/g, "")
  const logStreamName = `${day}/${session}`

  try {
    await client.send(
      new CreateLogStreamCommand({ logGroupName: LOG_GROUP, logStreamName })
    )
  } catch (e) {
    // Re-using a stream is the common case — the create then throws ResourceAlreadyExists; ignore it.
    if (e?.name !== "ResourceAlreadyExistsException") {
      console.error("CreateLogStream failed", e?.name, e?.message)
    }
  }
  try {
    await client.send(
      new PutLogEventsCommand({
        logGroupName: LOG_GROUP,
        logStreamName,
        logEvents
      })
    )
  } catch (e) {
    console.error("PutLogEvents failed", e?.name, e?.message)
    return reply(502)
  }
  return reply(204)
}
