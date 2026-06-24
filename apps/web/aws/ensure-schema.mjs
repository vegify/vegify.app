// Idempotent, additive migration for the long-lived EFS database.
//
// EFS persists across deploys, and the handler's seed-copy only runs on an EMPTY volume — so a DB
// that was seeded before the auth schema existed (A0) is missing `users.password_hash` and the
// `sessions` table, and a redeploy won't fix it (the seed-copy is skipped on a non-empty volume).
// This adds them in place, without touching data, before the server opens the DB. It is safe to run
// on every cold start: the column add is guarded and the table/indexes use IF NOT EXISTS, so it is a
// no-op once applied. (Dev/CI run `db:push` on a fresh DB; this path exists only for the EFS volume.)
import { createClient } from "@libsql/client";

export async function ensureAuthSchema(dbPath) {
  const db = createClient({ url: `file:${dbPath}` });
  try {
    const cols = await db.execute("PRAGMA table_info(users)");
    if (!cols.rows.some((r) => r.name === "password_hash")) {
      await db.execute("ALTER TABLE users ADD COLUMN password_hash text");
    }
    await db.executeMultiple(`
      CREATE TABLE IF NOT EXISTS sessions (
        id text PRIMARY KEY NOT NULL,
        user_id text NOT NULL,
        hashed_token text NOT NULL,
        expires_at integer NOT NULL,
        created_at integer,
        updated_at integer,
        FOREIGN KEY (user_id) REFERENCES users(id) ON UPDATE no action ON DELETE cascade
      );
      CREATE UNIQUE INDEX IF NOT EXISTS sessions_hashed_token_unique ON sessions (hashed_token);
      CREATE INDEX IF NOT EXISTS sessions_user_idx ON sessions (user_id);
    `);
  } finally {
    db.close();
  }
}
