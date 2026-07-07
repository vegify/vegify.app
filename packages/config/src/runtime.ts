// Runtime configuration for the JS side — the single home for every environment-sourced setting the
// web shell and db tooling read while RUNNING. (Deploy-time synth settings live in ./deploy.ts; the
// Rust binaries' twin is crates/vegify-config.) Accessors, not constants: each call reads the live
// process env, so nothing is captured at import time and a bundler can't inline a build machine's
// values. These modules ARE the config documentation — every knob, default, and override lives here
// (plus docs/self-host.md for the deploy story). There is no .env file anywhere, by design.
//
// The two Lambda entry files (apps/web/aws/lambda-handler.mjs, infra/lambda/client-logs/index.mjs)
// deliberately do NOT import this module: they ship as standalone assets with no node_modules, so
// their reads stay inline. Their values are injected by the CDK at deploy time, not by humans.

/** The standing Axum backend's base URL. Dev default: a local `cargo run -p vegify-server`.
 *  Deployed: set by the CDK on the Lambda env / by CI on client builds. */
export function apiUrl(): string {
  return process.env.VEGIFY_API_URL ?? "http://localhost:8787"
}

/** Canonical public site origin for generated absolute URLs (the sitemap). Unset → undefined, and
 *  callers fall back to the request origin (right for local serving; behind CloudFront the Lambda
 *  only ever sees its function-URL host, so deploys always set this). */
export function publicUrl(): string | undefined {
  return process.env.VEGIFY_PUBLIC_URL || undefined
}

/** libSQL database URL. Dev default: the repo-root SQLite file (the ../../ works from both apps/*
 *  and packages/* cwds). Remote (Turso/sqld): set DATABASE_URL (+ DATABASE_AUTH_TOKEN). */
export function databaseUrl(): string {
  return process.env.DATABASE_URL ?? "file:../../.data/vegify.db"
}

/** Auth token for a remote DATABASE_URL; undefined for local files. */
export function databaseAuthToken(): string | undefined {
  return process.env.DATABASE_AUTH_TOKEN || undefined
}

/** Listen port for a local server process (PORT), with the caller's fallback. */
export function listenPort(fallback: number): number {
  const p = Number(process.env.PORT)
  return Number.isInteger(p) && p > 0 ? p : fallback
}
