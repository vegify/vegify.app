//! UGC safety — the moderation surface the App Store requires for user-generated content with
//! direct messages (App Review Guideline 1.2): a way to REPORT content or users, and a way to
//! BLOCK an abusive user. Server-owned tables (like messages/notifications), never in the shared
//! content schema.
//!
//! - `reports`: an append-only log of user reports (content or a user + a reason). The operator
//!   reviews out-of-band (a follow-up moderation queue can read this table); the App Store
//!   requirement is that the mechanism exists and acts within 24h — the EULA commits to that.
//! - `user_blocks`: a directed block (blocker → blocked). A block (in either direction) hides both
//!   users from each other's DMs (send refused) and drops the blocked user's content from the
//!   blocker's browse/search/profile reads.
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppError;

pub fn ensure_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS reports (
            id TEXT PRIMARY KEY,
            reporter_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            target_type TEXT NOT NULL,          -- 'ingredient' | 'recipe' | 'user' | 'message'
            target_id TEXT NOT NULL,
            reason TEXT NOT NULL,
            note TEXT,
            resolved_at INTEGER,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS reports_open_idx ON reports(resolved_at, created_at);
        CREATE TABLE IF NOT EXISTS user_blocks (
            blocker_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            blocked_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (blocker_id, blocked_id)
        );
        CREATE INDEX IF NOT EXISTS user_blocks_blocked_idx ON user_blocks(blocked_id);",
    )
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const TARGET_TYPES: [&str; 4] = ["ingredient", "recipe", "user", "message"];
const REASONS: [&str; 5] = ["spam", "abuse", "sexual", "violence", "other"];

/// Record a report. Validated but never rejected for a duplicate — a second report is signal too.
pub fn report(
    conn: &Connection,
    reporter_id: &str,
    target_type: &str,
    target_id: &str,
    reason: &str,
    note: Option<&str>,
) -> Result<(), AppError> {
    if !TARGET_TYPES.contains(&target_type) {
        return Err(AppError::BadRequest("Unknown report target.".into()));
    }
    if !REASONS.contains(&reason) {
        return Err(AppError::BadRequest("Unknown report reason.".into()));
    }
    if target_id.trim().is_empty() {
        return Err(AppError::BadRequest("A report needs a target.".into()));
    }
    conn.execute(
        "INSERT INTO reports (id, reporter_id, target_type, target_id, reason, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            vegify_core::new_id(),
            reporter_id,
            target_type,
            target_id.trim(),
            reason,
            note.map(str::trim).filter(|s| !s.is_empty()),
            now_ms()
        ],
    )?;
    tracing::warn!(reporter = %reporter_id, target_type, target_id, reason, "content reported");
    Ok(())
}

/// Resolve a username to its user id (for block-by-handle).
fn user_id_by_username(conn: &Connection, username: &str) -> Result<String, AppError> {
    conn.query_row(
        "SELECT id FROM users WHERE username = ?1",
        [username],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::BadRequest("No such user.".into()))
}

/// Block a user by handle (idempotent). You can't block yourself.
pub fn block(conn: &Connection, blocker_id: &str, blocked_username: &str) -> Result<(), AppError> {
    let blocked_id = user_id_by_username(conn, blocked_username)?;
    if blocked_id == blocker_id {
        return Err(AppError::BadRequest("You can't block yourself.".into()));
    }
    conn.execute(
        "INSERT OR IGNORE INTO user_blocks (blocker_id, blocked_id, created_at) VALUES (?1, ?2, ?3)",
        params![blocker_id, blocked_id, now_ms()],
    )?;
    Ok(())
}

/// Unblock (idempotent).
pub fn unblock(
    conn: &Connection,
    blocker_id: &str,
    blocked_username: &str,
) -> Result<(), AppError> {
    let blocked_id = user_id_by_username(conn, blocked_username)?;
    conn.execute(
        "DELETE FROM user_blocks WHERE blocker_id = ?1 AND blocked_id = ?2",
        params![blocker_id, blocked_id],
    )?;
    Ok(())
}

/// Whether a block exists in EITHER direction between two users — the symmetric gate DMs use.
pub fn either_blocked(conn: &Connection, a: &str, b: &str) -> Result<bool, AppError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM user_blocks
         WHERE (blocker_id = ?1 AND blocked_id = ?2) OR (blocker_id = ?2 AND blocked_id = ?1)",
        params![a, b],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// The set of user ids `me` has blocked. The DM surfaces filter blocks today (send gate +
/// conversation list); wiring this into the shared content reads (hiding a blocked user's public
/// recipes/ingredients from the blocker's browse/search) is the follow-up — it needs the block set
/// threaded through vegify-core, which the desktop shares. Kept + tested as that hook.
#[allow(dead_code)]
pub fn blocked_ids(conn: &Connection, me_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT blocked_id FROM user_blocks WHERE blocker_id = ?1")?;
    let ids = stmt
        .query_map([me_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE users (id TEXT PRIMARY KEY, username TEXT);
             INSERT INTO users VALUES ('u1','ada'), ('u2','bob');",
        )
        .unwrap();
        ensure_tables(&c).unwrap();
        c
    }

    #[test]
    fn report_validates_and_records() {
        let c = conn();
        assert!(report(&c, "u1", "bogus", "x", "spam", None).is_err());
        assert!(report(&c, "u1", "recipe", "r1", "nonsense", None).is_err());
        report(&c, "u1", "recipe", "r1", "abuse", Some("mean")).unwrap();
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM reports", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn block_is_symmetric_for_dms_directional_for_reads() {
        let c = conn();
        assert!(block(&c, "u1", "ada").is_err(), "can't block yourself");
        block(&c, "u1", "bob").unwrap();
        block(&c, "u1", "bob").unwrap(); // idempotent
        assert!(either_blocked(&c, "u1", "u2").unwrap());
        assert!(either_blocked(&c, "u2", "u1").unwrap(), "symmetric for DMs");
        assert_eq!(blocked_ids(&c, "u1").unwrap(), vec!["u2".to_string()]);
        assert!(
            blocked_ids(&c, "u2").unwrap().is_empty(),
            "directional for reads"
        );
        unblock(&c, "u1", "bob").unwrap();
        assert!(!either_blocked(&c, "u1", "u2").unwrap());
    }
}
