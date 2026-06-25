//! Server-side auth — the Rust port of packages/db/src/auth.ts. argon2id password hashing (PHC strings
//! cross-compatible with hash-wasm, so EXISTING live hashes verify unchanged) + opaque server-side
//! sessions (sha256-hashed 32-byte tokens, 30-day TTL). Operates on a `&Connection` from the pool;
//! timestamps are Unix-ms integers (Drizzle's `timestamp_ms` mode), so the format matches the TS rows.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::AppError;

const SESSION_TTL_MS: i64 = 1000 * 60 * 60 * 24 * 30; // 30 days

/// The signed-in user — the `{id, name, email}` the auth routes return, and the viewer the content
/// gates scope to. Serializes with bare field names (matching the web's response).
#[derive(Serialize, Clone)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
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
    let salt = SaltString::generate(&mut OsRng);
    hasher()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("hash: {e}")))
}

/// Verify a password against a stored PHC hash. argon2 reads the params back out of the PHC string, so
/// hash-wasm-written hashes verify natively (the cutover-compat property). False on any parse/mismatch.
pub fn verify_password(hash: &str, password: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

/// A throwaway hash, verified on unknown-email logins so response timing doesn't reveal whether an
/// email exists. Computed once (argon2 is intentionally slow).
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash_password("vegify-timing-equalizer").expect("dummy hash"))
}

pub fn create_user(conn: &Connection, name: &str, email: &str, password: &str) -> Result<User, AppError> {
    let id = vegify_core::new_id();
    let hash = hash_password(password)?;
    let email = normalize_email(email);
    let now = now_ms();
    conn.execute(
        "INSERT INTO users(id, name, email, password_hash, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, name, email, hash, now],
    )?;
    Ok(User { id, name: name.to_string(), email })
}

/// Verify credentials. Returns the user on success, None otherwise. Timing-equalized: exactly one
/// argon2 verify per path (the real hash for a known account, a dummy otherwise).
pub fn authenticate(conn: &Connection, email: &str, password: &str) -> Result<Option<User>, AppError> {
    let email = normalize_email(email);
    let row: Option<(String, String, String, Option<String>)> = conn
        .query_row(
            "SELECT id, name, email, password_hash FROM users WHERE email = ?1",
            [&email],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    match row {
        Some((id, name, email, Some(hash))) => {
            Ok(verify_password(&hash, password).then_some(User { id, name, email }))
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
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let now = now_ms();
    conn.execute(
        "INSERT INTO sessions(id, user_id, hashed_token, expires_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![vegify_core::new_id(), user_id, token_hash(&token), now + SESSION_TTL_MS, now],
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
            "SELECT id, name, email FROM users WHERE id = ?1",
            [&uid],
            |r| Ok(User { id: r.get(0)?, name: r.get(1)?, email: r.get(2)? }),
        )
        .optional()?;
    Ok(user)
}

pub fn invalidate_session(conn: &Connection, token: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM sessions WHERE hashed_token = ?1", [token_hash(token)])?;
    Ok(())
}

/// Whether an account already exists for this email (normalized) — for signup's 409.
pub fn email_exists(conn: &Connection, email: &str) -> Result<bool, AppError> {
    Ok(conn
        .query_row("SELECT 1 FROM users WHERE email = ?1", [normalize_email(email)], |_| Ok(()))
        .optional()?
        .is_some())
}

/// Set an INITIAL password for an account that has none (NULL hash). Refuses to change an existing
/// one — claims a pre-provisioned/seeded account. NotFound when there's no such account (the web's
/// 404), Conflict when a password is already set.
pub fn set_initial_password(conn: &Connection, email: &str, password: &str) -> Result<(), AppError> {
    let email = normalize_email(email);
    let existing: Option<(String, Option<String>)> = conn
        .query_row("SELECT id, password_hash FROM users WHERE email = ?1", [&email], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
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

/// Pull the bearer token out of an Authorization header (case-insensitive `Bearer `, then trimmed).
pub fn bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let h = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let t = h.trim_start();
    let prefix = t.get(..7)?;
    prefix.eq_ignore_ascii_case("bearer ").then(|| t[7..].trim().to_string())
}
