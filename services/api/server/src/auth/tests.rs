//! Tests for the auth module. Kept in their own file so code scanning can skip test
//! fixtures (see `.github/codeql/codeql-config.yml`).

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

// Generated under argon2 0.5.3 by this module's own `hash_password` (m=19456, t=2, p=1, 32-byte
// output), before the 0.6 bump. It stands in for the PHC strings already sitting in the live
// `users.password_hash` column: they must keep verifying across the crate upgrade.
const LEGACY_PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$8DHMW9L3hxkVUb7y0uiKrA$JS6iH/E02TYuZB5vKiI8g0FiFmdn+n0cvB12woAMXts";
const LEGACY_PASSWORD: &str = "correct-horse-battery-staple";

#[test]
fn verifies_hashes_written_by_the_previous_argon2() {
    // A stored 0.5-era hash still accepts its plaintext and still rejects anything else.
    assert!(verify_password(LEGACY_PHC, LEGACY_PASSWORD));
    assert!(!verify_password(LEGACY_PHC, "wrong-password"));

    // A hash minted by the current crate round-trips, and carries its own random salt.
    let fresh = hash_password(LEGACY_PASSWORD).unwrap();
    assert_ne!(fresh, LEGACY_PHC, "every hash gets a fresh salt");
    assert!(verify_password(&fresh, LEGACY_PASSWORD));
    assert!(!verify_password(&fresh, "wrong-password"));

    // Garbage in the column is a failed verification, never a panic.
    assert!(!verify_password("not-a-phc-string", LEGACY_PASSWORD));
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
