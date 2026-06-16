import { createClient } from "@libsql/client";
import { drizzle } from "drizzle-orm/libsql";
import * as schema from "./schema";

// Local dev: a SQLite file at the repo root (works from apps/* and packages/db cwds).
// Prod: set DATABASE_URL (+ DATABASE_AUTH_TOKEN for Turso) — same client either way.
const url = process.env.DATABASE_URL ?? "file:../../.data/vegify.db";
const authToken = process.env.DATABASE_AUTH_TOKEN;

export const client = createClient(authToken ? { url, authToken } : { url });
export const db = drizzle(client, { schema });

export * from "./schema";
export * as schema from "./schema";
export * from "./mutations";
export * from "./nutrition";
