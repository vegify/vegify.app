//! In-app notifications (server-owned, like messages — online-only). A notification is a row per
//! event per recipient: `kind` names the event, `payload` is an opaque JSON blob the clients render
//! by kind (unknown kinds fall back gracefully, so new event types never require a lockstep client
//! release). v1 has one producer — a DM send notifies the recipient (messages.rs calls [`notify`]) —
//! and the read model is bell-standard: opening the notifications page reads everything
//! ([`mark_all_read`]); opening a DM thread also reads that sender's message notifications
//! ([`mark_message_thread_read`]), so the bell never nags about a conversation you've already seen.
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;

use crate::error::AppError;

/// Bell dropdown depth — the page shows the recent window, not an infinite archive.
const LIST_LIMIT: i64 = 50;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// The module's table — called from the server's `ensure_schema` (additive, idempotent).
pub fn ensure_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notifications (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            read_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS notifications_user_idx ON notifications(user_id, created_at);
        CREATE INDEX IF NOT EXISTS notifications_user_unread_idx ON notifications(user_id, read_at);",
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    pub kind: String,
    /// Parsed payload — shape is per-kind (kind "message": `{from: {id,name,username}, preview}`).
    pub payload: Value,
    pub created_at: i64,
    pub read: bool,
}

/// Record an event for `user_id`. Callers pass a serializable payload; failures are the caller's to
/// surface (v1's one caller treats the notification as part of the send).
pub fn notify(conn: &Connection, user_id: &str, kind: &str, payload: &Value) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO notifications (id, user_id, kind, payload, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![vegify_core::new_id(), user_id, kind, payload.to_string(), now_ms()],
    )?;
    Ok(())
}

/// The viewer's recent notifications, newest first.
pub fn list(conn: &Connection, me_id: &str) -> Result<Vec<Notification>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, payload, created_at, read_at IS NOT NULL FROM notifications
          WHERE user_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![me_id, LIST_LIMIT], |r| {
            let payload: String = r.get(2)?;
            Ok(Notification {
                id: r.get(0)?,
                kind: r.get(1)?,
                payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
                created_at: r.get(3)?,
                read: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Unread count — the bell badge number.
pub fn unread_count(conn: &Connection, me_id: &str) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND read_at IS NULL",
        [me_id],
        |r| r.get(0),
    )?)
}

/// Read everything (the notifications page was opened — bell-standard semantics).
pub fn mark_all_read(conn: &Connection, me_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE notifications SET read_at = ?1 WHERE user_id = ?2 AND read_at IS NULL",
        params![now_ms(), me_id],
    )?;
    Ok(())
}

/// Read the message notifications from one sender — called when their DM thread opens, so the bell
/// and the thread drain together. Matches on the payload's `from.id` (kind "message" only).
pub fn mark_message_thread_read(conn: &Connection, me_id: &str, from_user_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE notifications SET read_at = ?1
          WHERE user_id = ?2 AND kind = 'message' AND read_at IS NULL
            AND json_extract(payload, '$.from.id') = ?3",
        params![now_ms(), me_id, from_user_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL, email TEXT NOT NULL, username TEXT);
             INSERT INTO users (id, name, email, username) VALUES ('u1','Ada','a@x','ada'), ('u2','Grace','g@x','grace');",
        )
        .unwrap();
        ensure_tables(&conn).unwrap();
        conn
    }

    #[test]
    fn notify_list_and_read_all_round_trip() {
        let conn = test_conn();
        notify(&conn, "u2", "message", &json!({"from": {"id": "u1"}, "preview": "hi"})).unwrap();
        notify(&conn, "u2", "message", &json!({"from": {"id": "u1"}, "preview": "again"})).unwrap();
        assert_eq!(unread_count(&conn, "u2").unwrap(), 2);
        assert_eq!(unread_count(&conn, "u1").unwrap(), 0, "sender gets nothing");
        let rows = list(&conn, "u2").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].payload["preview"], "again", "newest first");
        assert!(!rows[0].read);
        mark_all_read(&conn, "u2").unwrap();
        assert_eq!(unread_count(&conn, "u2").unwrap(), 0);
        assert!(list(&conn, "u2").unwrap().iter().all(|n| n.read));
    }

    #[test]
    fn thread_open_reads_only_that_senders_message_notifications() {
        let conn = test_conn();
        notify(&conn, "u2", "message", &json!({"from": {"id": "u1"}, "preview": "from ada"})).unwrap();
        notify(&conn, "u2", "message", &json!({"from": {"id": "u9"}, "preview": "from other"})).unwrap();
        notify(&conn, "u2", "system", &json!({"note": "unrelated"})).unwrap();
        mark_message_thread_read(&conn, "u2", "u1").unwrap();
        assert_eq!(unread_count(&conn, "u2").unwrap(), 2, "only ada's message notification drained");
    }
}
