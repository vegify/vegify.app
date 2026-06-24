import { createHash, randomBytes } from "node:crypto";
import { and, eq, gt } from "drizzle-orm";
import { argon2id, argon2Verify } from "hash-wasm";
import { db } from "./index";
import { sessions, users } from "./schema";

// Self-hosted auth core: argon2id password hashing + opaque server-side sessions.
// Server-side only (Node) — used by the web shell's request handlers, the seed, and tests.
// The desktop never calls this directly; it authenticates over HTTP against the web shell,
// which in turn calls these functions.

// OWASP argon2id minimum (m=19 MiB, t=2, p=1). memorySize is in KiB. `encoded` output is a
// self-describing PHC string, so argon2Verify reads the params back out of the stored hash.
const ARGON2 = {
  parallelism: 1,
  iterations: 2,
  memorySize: 19456,
  hashLength: 32,
  outputType: "encoded" as const,
};

const SESSION_TTL_MS = 1000 * 60 * 60 * 24 * 30; // 30 days

export type SessionUser = typeof users.$inferSelect;
export type CreatedSession = { token: string; expiresAt: Date };

const normalizeEmail = (email: string) => email.trim().toLowerCase();

export async function hashPassword(password: string): Promise<string> {
  return argon2id({ password, salt: randomBytes(16), ...ARGON2 });
}

export async function verifyPassword(
  hash: string,
  password: string
): Promise<boolean> {
  try {
    return await argon2Verify({ password, hash });
  } catch {
    return false;
  }
}

// A throwaway hash, verified on unknown-email logins so response timing doesn't reveal whether
// an email exists. Computed once, lazily (argon2 is intentionally slow).
let dummyHash: Promise<string> | null = null;
const getDummyHash = () => (dummyHash ??= hashPassword("vegify-timing-equalizer"));

export async function createUser(input: {
  name: string;
  email: string;
  password: string;
}): Promise<SessionUser> {
  const passwordHash = await hashPassword(input.password);
  const [user] = await db
    .insert(users)
    .values({
      name: input.name,
      email: normalizeEmail(input.email),
      passwordHash,
    })
    .returning();
  return user;
}

/** Verify credentials. Returns the user on success, null otherwise. Timing-equalized. */
export async function authenticate(
  email: string,
  password: string
): Promise<SessionUser | null> {
  const [user] = await db
    .select()
    .from(users)
    .where(eq(users.email, normalizeEmail(email)))
    .limit(1);
  if (!user?.passwordHash) {
    await verifyPassword(await getDummyHash(), password);
    return null;
  }
  return (await verifyPassword(user.passwordHash, password)) ? user : null;
}

const tokenHash = (token: string) =>
  createHash("sha256").update(token).digest("hex");

/** Mint a session; returns the raw token (store client-side) — only its hash is persisted. */
export async function createSession(userId: string): Promise<CreatedSession> {
  const token = randomBytes(32).toString("base64url");
  const expiresAt = new Date(Date.now() + SESSION_TTL_MS);
  await db
    .insert(sessions)
    .values({ userId, hashedToken: tokenHash(token), expiresAt });
  return { token, expiresAt };
}

/** Resolve a raw session token to its user, or null if missing/expired. */
export async function validateSession(
  token: string
): Promise<SessionUser | null> {
  const [row] = await db
    .select()
    .from(sessions)
    .where(
      and(eq(sessions.hashedToken, tokenHash(token)), gt(sessions.expiresAt, new Date()))
    )
    .limit(1);
  if (!row) return null;
  const [user] = await db
    .select()
    .from(users)
    .where(eq(users.id, row.userId))
    .limit(1);
  return user ?? null;
}

export async function invalidateSession(token: string): Promise<void> {
  await db.delete(sessions).where(eq(sessions.hashedToken, tokenHash(token)));
}

export async function invalidateAllSessions(userId: string): Promise<void> {
  await db.delete(sessions).where(eq(sessions.userId, userId));
}
