//! Server-side auth — the Rust port of packages/db/src/auth.ts. argon2id password hashing (PHC strings
//! cross-compatible with hash-wasm, so EXISTING live hashes verify unchanged) + opaque server-side
//! sessions (sha256-hashed 32-byte tokens, 30-day TTL). Operates on a `&Connection` from the pool;
//! timestamps are Unix-ms integers (Drizzle's `timestamp_ms` mode), so the format matches the TS rows.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::handles;
pub use vegify_api_types::User;

const SESSION_TTL_MS: i64 = 1000 * 60 * 60 * 24 * 30; // 30 days
const RESET_TTL_MS: i64 = 1000 * 60 * 60; // 1 hour — password-reset links are short-lived
const EMAIL_VERIFY_TTL_MS: i64 = 1000 * 60 * 60 * 24; // 24 hours — verification links live longer than resets

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before 1970")
        .as_millis() as i64
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// N cryptographically-secure random bytes straight from the OS CSPRNG (`getrandom` — the same
/// source `rand`'s `OsRng` wraps, used directly so auth carries no `rand`/`rand_core` version
/// coupling with argon2's pinned `rand_core`). A failure here means the OS entropy source is
/// unavailable, which is unrecoverable for auth — panic rather than mint a weak token/salt.
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("OS CSPRNG unavailable");
    bytes
}

/// OWASP argon2id minimum, matching hash-wasm's params: m=19 MiB (19456 KiB), t=2, p=1, 32-byte output.
fn hasher() -> Argon2<'static> {
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19456, 2, 1, Some(32)).expect("valid argon2 params"),
    )
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    // 16-byte random salt (argon2's default), b64-encoded into the PHC salt — equivalent to what
    // `SaltString::generate` did, but sourced from getrandom so it doesn't need argon2's rand_core.
    let salt = SaltString::encode_b64(&random_bytes::<16>())
        .map_err(|e| AppError::Internal(format!("salt: {e}")))?;
    hasher()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("hash: {e}")))
}

/// Verify a password against a stored PHC hash. argon2 reads the params back out of the PHC string, so
/// hash-wasm-written hashes verify natively (the cutover-compat property). False on any parse/mismatch.
pub fn verify_password(hash: &str, password: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// A throwaway hash, verified on unknown-email logins so response timing doesn't reveal whether an
/// email exists. Computed once (argon2 is intentionally slow).
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash_password("vegify-timing-equalizer").expect("dummy hash"))
}

pub fn create_user(
    conn: &Connection,
    name: &str,
    email: &str,
    password: &str,
) -> Result<User, AppError> {
    let id = vegify_core::new_id();
    let hash = hash_password(password)?;
    let email = normalize_email(email);
    let username = derive_unique_username(conn, name, &email, &id)?;
    let now = now_ms();
    conn.execute(
        "INSERT INTO users(id, name, username, email, password_hash, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, name, username, email, hash, now],
    )?;
    Ok(User {
        id,
        name: name.to_string(),
        username,
        email,
        email_verified: false,
    })
}

/// Assign a unique handle for a new (or backfilled) user: slug the display name, else the email
/// local-part, else the (random) id as an always-valid last resort; then append `-1`, `-2`, … until
/// no `users.username` row collides. Pure derivation can't fail — only the uniqueness probe can.
pub fn derive_unique_username(
    conn: &Connection,
    name: &str,
    email: &str,
    id: &str,
) -> rusqlite::Result<String> {
    let mut base = handles::slugify(name);
    if handles::validate_username(&base).is_err() {
        base = handles::slugify(email.split('@').next().unwrap_or(""));
    }
    if handles::validate_username(&base).is_err() {
        base = handles::slugify(id); // a ULID slug is always a valid handle
    }
    // Leave room for a `-<n>` suffix inside MAX_LEN.
    base.truncate(handles::MAX_LEN - 5);
    while base.ends_with('-') {
        base.pop();
    }
    for n in 0..10_000 {
        let cand = if n == 0 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        // A suffix can re-trip validation (length/reserved) — skip those.
        let Ok(normalized) = handles::validate_username(&cand) else {
            continue;
        };
        let taken: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE username = ?1)",
            [&normalized],
            |r| r.get(0),
        )?;
        if !taken {
            return Ok(normalized);
        }
    }
    Ok(handles::slugify(id)) // unreachable in practice — the id slug is unique
}

/// Verify credentials. Returns the user on success, None otherwise. Timing-equalized: exactly one
/// argon2 verify per path (the real hash for a known account, a dummy otherwise).
/// users row for credential checks: (id, name, username, email,
/// password_hash, email_verified_at).
type UserAuthRow = (String, String, String, String, Option<String>, Option<i64>);
pub fn authenticate(
    conn: &Connection,
    identifier: &str,
    password: &str,
) -> Result<Option<User>, AppError> {
    // Sign in with email OR username. Both are stored lower-cased, so trim+lowercase lets a single bound
    // parameter match either column. (A username can't contain '@' and an email must, so no ambiguity.)
    let identifier = identifier.trim().to_lowercase();
    let row: Option<UserAuthRow> = conn
        .query_row(
            "SELECT id, name, username, email, password_hash, email_verified_at
             FROM users WHERE email = ?1 OR username = ?1",
            [&identifier],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?;
    match row {
        Some((id, name, username, email, Some(hash), verified_at)) => {
            Ok(verify_password(&hash, password).then_some(User {
                id,
                name,
                username,
                email,
                email_verified: verified_at.is_some(),
            }))
        }
        _ => {
            // unknown email or an account with no password: burn the same time, reveal nothing
            verify_password(dummy_hash(), password);
            Ok(None)
        }
    }
}

fn token_hash(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}

/// Mint a session; returns the raw token (only its sha256 hash is persisted).
pub fn create_session(conn: &Connection, user_id: &str) -> Result<String, AppError> {
    let bytes = random_bytes::<32>();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let now = now_ms();
    conn.execute(
        "INSERT INTO sessions(id, user_id, hashed_token, expires_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![
            vegify_core::new_id(),
            user_id,
            token_hash(&token),
            now + SESSION_TTL_MS,
            now
        ],
    )?;
    Ok(token)
}

/// Resolve a raw session token to its user, or None if missing/expired.
pub fn validate_session(conn: &Connection, token: &str) -> Result<Option<User>, AppError> {
    let uid: Option<String> = conn
        .query_row(
            "SELECT user_id FROM sessions WHERE hashed_token = ?1 AND expires_at > ?2",
            params![token_hash(token), now_ms()],
            |r| r.get(0),
        )
        .optional()?;
    let Some(uid) = uid else { return Ok(None) };
    let user = conn
        .query_row(
            "SELECT id, name, username, email, email_verified_at FROM users WHERE id = ?1",
            [&uid],
            |r| {
                Ok(User {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    username: r.get(2)?,
                    email: r.get(3)?,
                    email_verified: r.get::<_, Option<i64>>(4)?.is_some(),
                })
            },
        )
        .optional()?;
    Ok(user)
}

/// The viewer id for an OPTIONALLY-authed read: a valid bearer identifies the viewer (so they also see
/// their own non-public rows); an absent or invalid token yields `None`, and the caller (vegify-core)
/// then scopes the read to public-only. Never errors — anonymous reads are allowed.
pub fn optional_viewer(conn: &Connection, token: Option<String>) -> Option<String> {
    token
        .and_then(|t| validate_session(conn, &t).ok().flatten())
        .map(|u| u.id)
}

pub fn invalidate_session(conn: &Connection, token: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM sessions WHERE hashed_token = ?1",
        [token_hash(token)],
    )?;
    Ok(())
}

/// Whether an account already exists for this email (normalized) — for signup's 409.
/// Whether this user is an admin (email in the VEGIFY_ADMIN_EMAILS allowlist) — the gate for
/// inviting new accounts while public signups are closed.
pub fn is_admin(user: &User) -> bool {
    vegify_config::server::admin_emails().contains(&user.email.trim().to_lowercase())
}

pub fn email_exists(conn: &Connection, email: &str) -> Result<bool, AppError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM users WHERE email = ?1",
            [normalize_email(email)],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Set an INITIAL password for an account that has none (NULL hash). Refuses to change an existing
/// one — claims a pre-provisioned/seeded account. NotFound when there's no such account (the web's
/// 404), Conflict when a password is already set.
pub fn set_initial_password(
    conn: &Connection,
    email: &str,
    password: &str,
) -> Result<(), AppError> {
    let email = normalize_email(email);
    let existing: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, password_hash FROM users WHERE email = ?1",
            [&email],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match existing {
        None => Err(AppError::NotFound("No such account.".into())),
        Some((_, Some(_))) => Err(AppError::Conflict("Account already has a password.".into())),
        Some((id, None)) => {
            conn.execute(
                "UPDATE users SET password_hash = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, hash_password(password)?, now_ms()],
            )?;
            Ok(())
        }
    }
}

/// Create a single-use password-reset token for the account with this email, if one exists. Returns
/// `(name, raw_token)` for the reset link, or None when no account matches — the caller always responds
/// 200 either way, so the response never reveals whether an email is registered. Only the sha256 hash of
/// the token is stored (like sessions), so a DB leak yields no usable reset links.
pub fn create_password_reset(
    conn: &Connection,
    email: &str,
) -> Result<Option<(String, String)>, AppError> {
    let email = normalize_email(email);
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT id, name FROM users WHERE email = ?1",
            [&email],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((user_id, name)) = row else {
        return Ok(None);
    };
    let bytes = random_bytes::<32>();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let now = now_ms();
    conn.execute(
        "INSERT INTO password_reset_tokens(id, user_id, hashed_token, expires_at, used_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5)",
        params![vegify_core::new_id(), user_id, token_hash(&token), now + RESET_TTL_MS, now],
    )?;
    Ok(Some((name, token)))
}

/// Consume a reset token: set the account's new password and invalidate every existing session (a leaked
/// old session must not outlive the reset). Rejects an unknown, expired, or already-used token with a
/// generic 400. Marks ALL of the user's pending reset tokens used, so the link is strictly single-use.
pub fn consume_password_reset(
    conn: &Connection,
    token: &str,
    new_password: &str,
) -> Result<(), AppError> {
    if new_password.chars().count() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters.".into(),
        ));
    }
    let now = now_ms();
    let user_id: Option<String> = conn
        .query_row(
            "SELECT user_id FROM password_reset_tokens
             WHERE hashed_token = ?1 AND used_at IS NULL AND expires_at > ?2",
            params![token_hash(token), now],
            |r| r.get(0),
        )
        .optional()?;
    let Some(user_id) = user_id else {
        return Err(AppError::BadRequest(
            "This reset link is invalid or has expired.".into(),
        ));
    };
    conn.execute(
        "UPDATE users SET password_hash = ?2, updated_at = ?3 WHERE id = ?1",
        params![user_id, hash_password(new_password)?, now],
    )?;
    conn.execute(
        "UPDATE password_reset_tokens SET used_at = ?2, updated_at = ?2 WHERE user_id = ?1 AND used_at IS NULL",
        params![user_id, now],
    )?;
    conn.execute("DELETE FROM sessions WHERE user_id = ?1", [&user_id])?;
    Ok(())
}

/// Mint a single-use email-verification token for the account with this email, if one exists and is not
/// already verified. Returns `(name, raw_token)` for the verification link, or None (no such account, or
/// the email is already verified). The request endpoint always 200s, so this never reveals registration
/// state. Only the sha256 hash of the token is stored, like sessions and reset tokens.
pub fn create_email_verification(
    conn: &Connection,
    email: &str,
) -> Result<Option<(String, String)>, AppError> {
    let email = normalize_email(email);
    let row: Option<(String, String, Option<i64>)> = conn
        .query_row(
            "SELECT id, name, email_verified_at FROM users WHERE email = ?1",
            [&email],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((user_id, name, verified_at)) = row else {
        return Ok(None);
    };
    if verified_at.is_some() {
        return Ok(None); // already verified — nothing to send
    }
    let bytes = random_bytes::<32>();
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let now = now_ms();
    conn.execute(
        "INSERT INTO email_verification_tokens(id, user_id, hashed_token, expires_at, used_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5)",
        params![vegify_core::new_id(), user_id, token_hash(&token), now + EMAIL_VERIFY_TTL_MS, now],
    )?;
    Ok(Some((name, token)))
}

/// Consume an email-verification token: stamp `users.email_verified_at` and mark every pending token for
/// the account used (strictly single-use). Rejects an unknown, expired, or already-used token with a
/// generic 400. Unlike a password reset this does NOT touch sessions — verifying shouldn't sign you out.
pub fn consume_email_verification(conn: &Connection, token: &str) -> Result<(), AppError> {
    let now = now_ms();
    let user_id: Option<String> = conn
        .query_row(
            "SELECT user_id FROM email_verification_tokens
             WHERE hashed_token = ?1 AND used_at IS NULL AND expires_at > ?2",
            params![token_hash(token), now],
            |r| r.get(0),
        )
        .optional()?;
    let Some(user_id) = user_id else {
        return Err(AppError::BadRequest(
            "This verification link is invalid or has expired.".into(),
        ));
    };
    conn.execute(
        "UPDATE users SET email_verified_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![user_id, now],
    )?;
    conn.execute(
        "UPDATE email_verification_tokens SET used_at = ?2, updated_at = ?2 WHERE user_id = ?1 AND used_at IS NULL",
        params![user_id, now],
    )?;
    Ok(())
}

/// Delete the signed-in account (App Review 5.1.1(v)), password-reconfirmed. Irreversible.
///
/// The cascade respects the UGC model: the user's RECIPES are deleted; their LEAF INGREDIENTS that
/// are still used by ANOTHER user's recipe are ANONYMIZED to the communal catalog (user_id NULL,
/// live) so those recipes keep working; unreferenced ones are deleted. Everything keyed on the user
/// (sessions, reset/verify tokens, conversations→messages, blocks, reports, notifications) drops via
/// ON DELETE CASCADE when the user row is finally removed.
pub fn delete_account(conn: &Connection, user_id: &str, password: &str) -> Result<(), AppError> {
    let hash: Option<Option<String>> = conn
        .query_row(
            "SELECT password_hash FROM users WHERE id = ?1",
            [user_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(Some(hash)) = hash else {
        return Err(AppError::Unauthorized);
    };
    if !verify_password(&hash, password) {
        return Err(AppError::InvalidCredentials);
    }

    // Recipes first (frees ingredient references + removes each recipe's as-ingredient row).
    let recipe_ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT r.id FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id WHERE i.user_id = ?1",
        )?;
        let v = stmt
            .query_map([user_id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        v
    };
    for id in recipe_ids {
        vegify_core::do_delete_recipe(conn, &id, Some(user_id))
            .map_err(|e| AppError::internal(e.to_string()))?;
    }

    // Leaf ingredients: anonymize the ones still referenced by other users; delete the rest.
    let leaf_ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT id FROM ingredients WHERE user_id = ?1 AND id NOT IN (SELECT as_ingredient_id FROM recipes)",
        )?;
        let v = stmt
            .query_map([user_id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        v
    };
    for id in leaf_ids {
        let referenced: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ingredient_in_recipe WHERE ingredient_id = ?1",
            [&id],
            |r| r.get(0),
        )?;
        if referenced > 0 {
            // Sever ownership, restore to a live communal catalog row (others depend on it).
            conn.execute(
                "UPDATE ingredients SET user_id = NULL, deleted_at = NULL, visibility = 'public' WHERE id = ?1",
                [&id],
            )?;
        } else {
            vegify_core::do_delete_ingredient(conn, &id, Some(user_id))
                .map_err(|e| AppError::internal(e.to_string()))?;
        }
    }

    // The user row — cascades sessions, tokens, conversations→messages, blocks, reports, notifications.
    conn.execute("DELETE FROM users WHERE id = ?1", [user_id])?;
    tracing::info!(user = %user_id, "account deleted");
    Ok(())
}

/// Pull the bearer token out of an Authorization header (case-insensitive `Bearer `, then trimmed).
pub fn bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let h = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let t = h.trim_start();
    let prefix = t.get(..7)?;
    prefix
        .eq_ignore_ascii_case("bearer ")
        .then(|| t[7..].trim().to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod reset_tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL, username TEXT UNIQUE,
                email TEXT NOT NULL UNIQUE, password_hash TEXT, email_verified_at INTEGER,
                created_at INTEGER, updated_at INTEGER);
             CREATE TABLE sessions (id TEXT PRIMARY KEY, user_id TEXT NOT NULL, hashed_token TEXT NOT NULL UNIQUE,
                expires_at INTEGER NOT NULL, created_at INTEGER, updated_at INTEGER);
             CREATE TABLE password_reset_tokens (id TEXT PRIMARY KEY, user_id TEXT NOT NULL,
                hashed_token TEXT NOT NULL UNIQUE, expires_at INTEGER NOT NULL, used_at INTEGER,
                created_at INTEGER, updated_at INTEGER);
             CREATE TABLE email_verification_tokens (id TEXT PRIMARY KEY, user_id TEXT NOT NULL,
                hashed_token TEXT NOT NULL UNIQUE, expires_at INTEGER NOT NULL, used_at INTEGER,
                created_at INTEGER, updated_at INTEGER);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn signs_in_with_email_or_username() {
        let conn = test_conn();
        let user = create_user(&conn, "Test User", "user@example.com", "pw-123456").unwrap();
        assert_eq!(user.username, "test-user"); // derived handle for "Test User"
                                                // email, username, and case/space-insensitive all resolve to the same account
        assert_eq!(
            authenticate(&conn, "user@example.com", "pw-123456")
                .unwrap()
                .unwrap()
                .id,
            user.id
        );
        assert_eq!(
            authenticate(&conn, "test-user", "pw-123456")
                .unwrap()
                .unwrap()
                .id,
            user.id
        );
        assert_eq!(
            authenticate(&conn, "  Test-User  ", "pw-123456")
                .unwrap()
                .unwrap()
                .id,
            user.id
        );
        // wrong password and unknown identifier both fail
        assert!(authenticate(&conn, "test-user", "wrong").unwrap().is_none());
        assert!(authenticate(&conn, "ghost", "pw-123456").unwrap().is_none());
    }

    #[test]
    fn optional_viewer_resolves_only_a_valid_session() {
        let conn = test_conn();
        let user = create_user(&conn, "Test User", "user@example.com", "pw-123456").unwrap();
        let token = create_session(&conn, &user.id).unwrap();
        // A valid bearer identifies the viewer; an absent or garbage token is anonymous (None), never an error.
        assert_eq!(optional_viewer(&conn, Some(token)), Some(user.id));
        assert_eq!(optional_viewer(&conn, None), None);
        assert_eq!(
            optional_viewer(&conn, Some("not-a-real-token".into())),
            None
        );
    }

    #[test]
    fn password_reset_round_trip() {
        let conn = test_conn();
        let user = create_user(&conn, "Test User", "user@example.com", "old-password").unwrap();
        let session = create_session(&conn, &user.id).unwrap();

        // Unknown email reveals nothing — no token, no error (enumeration-safe).
        assert!(create_password_reset(&conn, "nobody@example.com")
            .unwrap()
            .is_none());

        // Known email, case/space-insensitive, mints a token and returns the name for the email.
        let (name, token) = create_password_reset(&conn, "  User@Example.com ")
            .unwrap()
            .unwrap();
        assert_eq!(name, "Test User");

        // Consuming sets the new password, rejects the old one, and kills existing sessions.
        consume_password_reset(&conn, &token, "new-password-123").unwrap();
        assert!(authenticate(&conn, "user@example.com", "new-password-123")
            .unwrap()
            .is_some());
        assert!(authenticate(&conn, "user@example.com", "old-password")
            .unwrap()
            .is_none());
        assert!(
            validate_session(&conn, &session).unwrap().is_none(),
            "reset must invalidate sessions"
        );

        // The link is strictly single-use.
        assert!(consume_password_reset(&conn, &token, "yet-another-123").is_err());

        // A new token still requires an 8+ char password.
        let (_, t2) = create_password_reset(&conn, "user@example.com")
            .unwrap()
            .unwrap();
        assert!(consume_password_reset(&conn, &t2, "short").is_err());
    }

    #[test]
    fn email_verification_round_trip() {
        let conn = test_conn();
        let user = create_user(&conn, "Test User", "user@example.com", "a-password").unwrap();
        assert!(!user.email_verified, "a fresh account starts unverified");

        // Unknown email mints nothing (enumeration-safe).
        assert!(create_email_verification(&conn, "nobody@example.com")
            .unwrap()
            .is_none());

        // Known, unverified email mints a token (case/space-insensitive) and returns the name.
        let (name, token) = create_email_verification(&conn, "  User@Example.com ")
            .unwrap()
            .unwrap();
        assert_eq!(name, "Test User");

        // Consuming it stamps email_verified_at — a fresh session now reports verified.
        consume_email_verification(&conn, &token).unwrap();
        let session = create_session(&conn, &user.id).unwrap();
        assert!(
            validate_session(&conn, &session)
                .unwrap()
                .unwrap()
                .email_verified
        );

        // The link is single-use, and an already-verified account mints no further tokens.
        assert!(consume_email_verification(&conn, &token).is_err());
        assert!(create_email_verification(&conn, "user@example.com")
            .unwrap()
            .is_none());
    }
}
