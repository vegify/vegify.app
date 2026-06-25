// AWS Lambda adapter for TanStack Start's WinterCG fetch handler. The web is a STATELESS SSR shell
// (P4: web-SSR-calls-Axum) — it holds no database; all auth + content is fetched from the standing
// vegify-server over HTTP (VEGIFY_API_URL, set by the CDK). This adapter just bridges the Lambda
// Function-URL event to the bundled, self-contained WinterCG handler.
const app = await import("./server/server.js");
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
