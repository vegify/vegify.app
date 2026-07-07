import { createClient } from "@libsql/client";
import { databaseAuthToken, databaseUrl } from "@vegify/config";
import { drizzle } from "drizzle-orm/libsql";
import * as schema from "./schema";

// Local dev: a SQLite file at the repo root (works from apps/* and packages/db cwds).
// Prod: set DATABASE_URL (+ DATABASE_AUTH_TOKEN for Turso) — same client either way.
const url = databaseUrl();
const authToken = databaseAuthToken();

export const client = createClient(authToken ? { url, authToken } : { url });
export const db = drizzle(client, { schema });

export * from "./access";
export * from "./auth";
export * from "./mutations";
export * from "./nutrition";
export * from "./schema";
export * as schema from "./schema";
