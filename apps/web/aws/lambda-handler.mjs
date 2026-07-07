// AWS Lambda adapter for TanStack Start's WinterCG fetch handler. The web is a STATELESS SSR shell
// (P4: web-SSR-calls-Axum) — it holds no database; all auth + content is fetched from the standing
// vegify-server over HTTP (VEGIFY_API_URL, set by the CDK). This adapter just bridges the Lambda
// Function-URL event to the bundled, self-contained WinterCG handler.
import { SITEMAP_PATH, sitemapResponse } from "./sitemap.mjs"

const app = await import("./server/server.js")
const target = app.default ?? app
const fetchHandler =
  typeof target === "function" ? target : target.fetch.bind(target)

// A WinterCG Response → the Lambda Function-URL result shape (base64 body + split Set-Cookie).
async function toLambda(response) {
  const respHeaders = {}
  const setCookie = []
  response.headers.forEach((val, key) => {
    if (key.toLowerCase() === "set-cookie") setCookie.push(val)
    else respHeaders[key] = val
  })
  const buf = Buffer.from(await response.arrayBuffer())
  return {
    statusCode: response.status,
    headers: respHeaders,
    cookies: setCookie.length ? setCookie : undefined,
    body: buf.toString("base64"),
    isBase64Encoded: true
  }
}

// Origin-verify: CloudFront injects x-vegify-origin on every forwarded request (set by the CDK from
// $ORIGIN_VERIFY_SECRET). Reject anything that doesn't carry it — i.e. a direct hit to the public
// Function URL, bypassing CloudFront. FAIL CLOSED on Lambda: a deployment missing the secret is a
// misconfiguration, not permission to serve the raw Function URL unprotected. Standalone/local runs
// (no Lambda env) still serve without it. This replaces OAC, which can't sign POST bodies.
const ORIGIN_SECRET = process.env.ORIGIN_SECRET
const ON_LAMBDA = Boolean(process.env.AWS_LAMBDA_FUNCTION_NAME)

export const handler = async (event) => {
  if (ON_LAMBDA && !ORIGIN_SECRET) {
    return {
      statusCode: 503,
      headers: { "content-type": "text/plain" },
      body: "Service Unavailable"
    }
  }
  if (ORIGIN_SECRET && event.headers?.["x-vegify-origin"] !== ORIGIN_SECRET) {
    return {
      statusCode: 403,
      headers: { "content-type": "text/plain" },
      body: "Forbidden"
    }
  }
  const {
    rawPath = "/",
    rawQueryString = "",
    headers = {},
    cookies,
    body,
    isBase64Encoded
  } = event
  const method = event.requestContext?.http?.method ?? "GET"
  const host =
    headers["x-forwarded-host"] ||
    headers.host ||
    event.requestContext?.domainName ||
    "localhost"
  const proto = headers["x-forwarded-proto"] || "https"
  const url = `${proto}://${host}${rawPath}${rawQueryString ? `?${rawQueryString}` : ""}`

  // Dynamic sitemap: return generated XML BEFORE the SSR handler + its auth gate (which would 307
  // /sitemap.xml → /login and hide it from crawlers). Enumerates public recipes + ingredients.
  if (rawPath === SITEMAP_PATH) {
    // Sitemap locs must use the canonical public origin, not the function-URL host CloudFront
    // dials — VEGIFY_PUBLIC_URL is set by the CDK from the first custom domain.
    const origin = process.env.VEGIFY_PUBLIC_URL || `${proto}://${host}`
    return toLambda(await sitemapResponse(process.env.VEGIFY_API_URL, origin))
  }

  const h = new Headers()
  for (const [k, v] of Object.entries(headers)) if (v != null) h.set(k, v)
  if (cookies?.length) h.set("cookie", cookies.join("; "))

  let reqBody
  if (method !== "GET" && method !== "HEAD" && body != null) {
    reqBody = isBase64Encoded ? Buffer.from(body, "base64") : body
  }

  const response = await fetchHandler(
    new Request(url, { method, headers: h, body: reqBody, duplex: "half" })
  )
  return toLambda(response)
}
