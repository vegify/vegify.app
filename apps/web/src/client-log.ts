// Non-blocking BROWSER log shipper → the VegifyClientLogs ingestion Lambda (→ CloudWatch). Events are
// buffered and flushed with `navigator.sendBeacon` — the browser queues the POST and returns instantly
// (zero main-thread blocking) and the request survives page unload, so we never lose the last batch on
// navigate/close. Falls back to `fetch(keepalive)` where sendBeacon is unavailable. With no endpoint
// configured (dev, or before VITE_CLIENT_LOG_URL is wired) it's a console-only no-op. Client-only:
// guarded so importing it during SSR has no effect. Logging must NEVER throw into app code.

type Level = "debug" | "info" | "warn" | "error";
type LogEvent = {
  ts: number;
  level: Level;
  msg: string;
  ctx?: Record<string, unknown>;
  url?: string;
};

// Vite inlines import.meta.env.* at build; unset in dev → endpoint is undefined → ship() is a no-op.
const ENDPOINT =
  (import.meta.env.VITE_CLIENT_LOG_URL as string | undefined) || undefined;
const MAX_BATCH = 50; // flush early once this many events queue, so a busy page doesn't hoard a big batch
const FLUSH_MS = 5_000;

const buf: LogEvent[] = [];
// A short per-page-load id so a session's events land in one CloudWatch stream (see the Lambda).
const session = Math.random().toString(36).slice(2, 10);
let timer: ReturnType<typeof setTimeout> | undefined;

function ship() {
  timer = undefined;
  if (buf.length === 0) return;
  const events = buf.splice(0, buf.length);
  if (!ENDPOINT) return; // dev / unconfigured: events already hit the console in emit()
  const payload = JSON.stringify({ session, events });
  try {
    // Send as text/plain, NOT application/json. application/json is not a CORS-safelisted content
    // type, so it forces a preflight — and `sendBeacon` cannot perform a preflighted request, so the
    // cross-origin POST silently fails ("CORS request did not succeed"). text/plain keeps it a simple
    // request (no preflight); the ingest Lambda JSON.parses the body regardless of Content-Type.
    if (typeof navigator !== "undefined" && navigator.sendBeacon) {
      navigator.sendBeacon(
        ENDPOINT,
        new Blob([payload], { type: "text/plain" }),
      );
    } else {
      void fetch(ENDPOINT, {
        method: "POST",
        body: payload,
        headers: { "content-type": "text/plain" },
        keepalive: true,
      }).catch(() => {});
    }
  } catch {
    // never let telemetry surface an error into the app
  }
}

function schedule() {
  if (buf.length >= MAX_BATCH) {
    ship();
    return;
  }
  if (!timer) timer = setTimeout(ship, FLUSH_MS);
}

function emit(level: Level, msg: string, ctx?: Record<string, unknown>) {
  buf.push({
    ts: Date.now(),
    level,
    msg,
    ctx,
    url: typeof location !== "undefined" ? location.pathname : undefined,
  });
  // Mirror to the console in dev only: devtools surface everything locally, but a production page
  // shouldn't chatter to the console — the events already ship to the ingest endpoint, and error-level
  // mirrors would otherwise show up in Lighthouse's "errors logged to console" audit. import.meta.env.DEV
  // is statically false in the prod build, so Vite dead-code-eliminates this line out of the bundle.
  if (import.meta.env.DEV)
    (console[level] ?? console.log)(`[vegify] ${msg}`, ctx ?? "");
  schedule();
}

/** Structured client logger. Use instead of bare console.* where you want the event shipped too. */
export const clientLog = {
  debug: (msg: string, ctx?: Record<string, unknown>) =>
    emit("debug", msg, ctx),
  info: (msg: string, ctx?: Record<string, unknown>) => emit("info", msg, ctx),
  warn: (msg: string, ctx?: Record<string, unknown>) => emit("warn", msg, ctx),
  error: (msg: string, ctx?: Record<string, unknown>) =>
    emit("error", msg, ctx),
};

let installed = false;

/** Install global error capture + flush-on-hide. Call once on the client (no-op during SSR). */
export function initClientLogging() {
  if (installed || typeof window === "undefined") return;
  installed = true;

  window.addEventListener("error", (e) =>
    emit("error", e.message || "window.error", {
      src: e.filename,
      line: e.lineno,
      col: e.colno,
    }),
  );
  window.addEventListener("unhandledrejection", (e) =>
    emit("error", "unhandledrejection", {
      reason: String((e as PromiseRejectionEvent).reason),
    }),
  );
  // sendBeacon survives unload, so flush the buffer the moment the tab is backgrounded or closed.
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") ship();
  });
  window.addEventListener("pagehide", ship);

  if (ENDPOINT) clientLog.info("client logging initialized", { session });
}
