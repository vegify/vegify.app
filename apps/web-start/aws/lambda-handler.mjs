// AWS Lambda adapter for TanStack Start's WinterCG fetch handler.
// TanStack Start (v1, no Nitro) builds `dist/server/server.js` exporting a fetch handler;
// this wraps it for a Lambda Function URL (payload format v2). The CDK packages this file
// next to the build output as { handler.mjs, server/ } and serves static `dist/client/*`
// from S3 + CloudFront. Native @libsql/client must be bundled for the Lambda arch.
import * as app from "./server/server.js";

const target = app.default ?? app;
const fetchHandler = typeof target === "function" ? target : target.fetch.bind(target);

export const handler = async (event) => {
  const { rawPath = "/", rawQueryString = "", headers = {}, cookies, body, isBase64Encoded } = event;
  const method = event.requestContext?.http?.method ?? "GET";
  const host = headers["x-forwarded-host"] || headers.host || event.requestContext?.domainName || "localhost";
  const proto = headers["x-forwarded-proto"] || "https";
  const url = `${proto}://${host}${rawPath}${rawQueryString ? `?${rawQueryString}` : ""}`;

  const h = new Headers();
  for (const [k, v] of Object.entries(headers)) if (v != null) h.set(k, v);
  if (cookies?.length) h.set("cookie", cookies.join("; "));

  let reqBody;
  if (method !== "GET" && method !== "HEAD" && body != null) {
    reqBody = isBase64Encoded ? Buffer.from(body, "base64") : body;
  }

  const response = await fetchHandler(
    new Request(url, { method, headers: h, body: reqBody, duplex: "half" }),
  );

  const respHeaders = {};
  const setCookie = [];
  response.headers.forEach((val, key) => {
    if (key.toLowerCase() === "set-cookie") setCookie.push(val);
    else respHeaders[key] = val;
  });
  const buf = Buffer.from(await response.arrayBuffer());
  return {
    statusCode: response.status,
    headers: respHeaders,
    cookies: setCookie.length ? setCookie : undefined,
    body: buf.toString("base64"),
    isBase64Encoded: true,
  };
};
