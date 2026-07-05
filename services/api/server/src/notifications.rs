//! In-app notifications (server-owned, like messages — online-only). A notification is a row per
//! event per recipient: `kind` names the event, `payload` is an opaque JSON blob the clients render
//! by kind (unknown kinds fall back gracefully, so new event types never require a lockstep client
//! release).
//!
//! THE BELL IS RESERVED for events that affect someone personally (John, 2026-07-04) — DMs do NOT
//! ring it (they have their own surface: the Mail badge). v1's one producer is the communal-catalog
//! event: an ingredient someone USES in their recipes was updated by its owner
//! ([`notify_ingredient_updated`], called from the save path). Per-recipient collapse keeps rapid
//! successive edits to ONE unread row per ingredient (the payload/timestamp refresh instead); a row
//! read and then re-triggered notifies again — you saw the old edit, this is a new one. The read
//! model is bell-standard: opening the notifications page reads everything ([`mark_all_read`]).
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::error::AppError;
pub use vegify_api_types::{Notification};

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


/// Record an event for `user_id`. Callers pass a serializable payload. The generic producer —
/// future event kinds enter here (today only tests use it directly; the ingredient producer below
/// carries its own collapse logic).
#[allow(dead_code)]
pub fn notify(conn: &Connection, user_id: &str, kind: &str, payload: &Value) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO notifications (id, user_id, kind, payload, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![vegify_core::new_id(), user_id, kind, payload.to_string(), now_ms()],
    )?;
    Ok(())
}

/// The v1 producer: `editor` updated an ingredient — notify every OTHER user whose recipes use it
/// (it changes their recipes' nutrition). Collapsed per recipient: an existing UNREAD row for this
/// ingredient is refreshed in place instead of stacking; read rows stay read (a later edit inserts
/// fresh — you saw the old one, this is news). Returns how many users were notified (0 → the caller
/// can skip the WS nudge).
pub fn notify_ingredient_updated(
    conn: &Connection,
    editor: &crate::auth::User,
    ingredient_id: &str,
) -> Result<usize, AppError> {
    let Some((name, slug)) = conn
        .query_row(
            "SELECT name, slug FROM ingredients WHERE id = ?1",
            [ingredient_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()?
    else {
        return Ok(0);
    };
    // Recipe ownership lives on the recipe's AS-INGREDIENT row (a recipe IS an ingredient —
    // recipes.as_ingredient_id → ingredients.user_id); the recipes table itself has no user_id.
    let recipients: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT ai.user_id FROM ingredient_in_recipe iir
               JOIN recipes r ON r.id = iir.recipe_id
               JOIN ingredients ai ON ai.id = r.as_ingredient_id
              WHERE iir.ingredient_id = ?1 AND ai.user_id IS NOT NULL AND ai.user_id != ?2",
        )?;
        let v = stmt
            .query_map(params![ingredient_id, editor.id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    let payload = serde_json::json!({
        "ingredient": { "id": ingredient_id, "name": name, "slug": slug },
        "by": { "name": editor.name, "username": editor.username },
    })
    .to_string();
    let now = now_ms();
    for user_id in &recipients {
        let collapsed = conn.execute(
            "UPDATE notifications SET payload = ?1, created_at = ?2
              WHERE user_id = ?3 AND kind = 'ingredient-updated' AND read_at IS NULL
                AND json_extract(payload, '$.ingredient.id') = ?4",
            params![payload, now, user_id, ingredient_id],
        )?;
        if collapsed == 0 {
            conn.execute(
                "INSERT INTO notifications (id, user_id, kind, payload, created_at) VALUES (?1, ?2, 'ingredient-updated', ?3, ?4)",
                params![vegify_core::new_id(), user_id, payload, now],
            )?;
        }
    }
    Ok(recipients.len())
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


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL, email TEXT NOT NULL, username TEXT);
             INSERT INTO users (id, name, email, username) VALUES
               ('u1','Ada','a@x','ada'), ('u2','Grace','g@x','grace'), ('u3','Alan','al@x','alan');
             -- Mirrors the REAL shape: recipes carry NO user_id — ownership lives on the recipe's
             -- as-ingredient row (a recipe IS an ingredient). The wrong assumption (recipes.user_id)
             -- once made these tests vacuously green while the live path 500'd.
             CREATE TABLE ingredients (id TEXT PRIMARY KEY, name TEXT NOT NULL, slug TEXT, user_id TEXT);
             CREATE TABLE recipes (id TEXT PRIMARY KEY, as_ingredient_id TEXT NOT NULL);
             CREATE TABLE ingredient_in_recipe (id TEXT PRIMARY KEY, recipe_id TEXT NOT NULL, ingredient_id TEXT NOT NULL);
             -- Ada owns the flour; Grace and Alan use it in their recipes; Ada uses it herself too.
             INSERT INTO ingredients (id, name, slug, user_id) VALUES
               ('i1', 'Caputo 00 Flour', 'caputo-00-flour', 'u1'),
               ('x1', 'Grace Bread', NULL, 'u2'), ('x2', 'Alan Bake', NULL, 'u3'), ('x3', 'Ada Loaf', NULL, 'u1');
             INSERT INTO recipes (id, as_ingredient_id) VALUES
               ('r-grace', 'x1'), ('r-alan', 'x2'), ('r-ada', 'x3');
             INSERT INTO ingredient_in_recipe (id, recipe_id, ingredient_id) VALUES
               ('l1', 'r-grace', 'i1'), ('l2', 'r-alan', 'i1'), ('l3', 'r-ada', 'i1');",
        )
        .unwrap();
        ensure_tables(&conn).unwrap();
        conn
    }

    fn ada() -> crate::auth::User {
        crate::auth::User {
            id: "u1".into(),
            name: "Ada".into(),
            username: "ada".into(),
            email: "a@x".into(),
            email_verified: true,
        }
    }

    #[test]
    fn ingredient_update_notifies_users_of_it_but_never_the_editor() {
        let conn = test_conn();
        let notified = notify_ingredient_updated(&conn, &ada(), "i1").unwrap();
        assert_eq!(notified, 2, "grace + alan use it; ada edited it");
        assert_eq!(unread_count(&conn, "u2").unwrap(), 1);
        assert_eq!(unread_count(&conn, "u3").unwrap(), 1);
        assert_eq!(unread_count(&conn, "u1").unwrap(), 0, "the editor is never notified");
        let rows = list(&conn, "u2").unwrap();
        assert_eq!(rows[0].kind, "ingredient-updated");
        assert_eq!(rows[0].payload["ingredient"]["name"], "Caputo 00 Flour");
        assert_eq!(rows[0].payload["by"]["username"], "ada");
        // An ingredient nobody uses (or an unknown id) notifies no one.
        assert_eq!(notify_ingredient_updated(&conn, &ada(), "missing").unwrap(), 0);
    }

    #[test]
    fn rapid_edits_collapse_to_one_unread_row_but_a_read_row_notifies_fresh() {
        let conn = test_conn();
        notify_ingredient_updated(&conn, &ada(), "i1").unwrap();
        notify_ingredient_updated(&conn, &ada(), "i1").unwrap();
        notify_ingredient_updated(&conn, &ada(), "i1").unwrap();
        assert_eq!(unread_count(&conn, "u2").unwrap(), 1, "collapsed: one unread per ingredient");
        assert_eq!(list(&conn, "u2").unwrap().len(), 1);
        // Grace reads it; a LATER edit is news again — a fresh unread row, the read one preserved.
        mark_all_read(&conn, "u2").unwrap();
        notify_ingredient_updated(&conn, &ada(), "i1").unwrap();
        assert_eq!(unread_count(&conn, "u2").unwrap(), 1);
        assert_eq!(list(&conn, "u2").unwrap().len(), 2);
    }

    #[test]
    fn read_all_round_trip_and_unknown_kinds_survive() {
        let conn = test_conn();
        notify(&conn, "u2", "future-kind", &json!({"whatever": true})).unwrap();
        notify_ingredient_updated(&conn, &ada(), "i1").unwrap();
        assert_eq!(unread_count(&conn, "u2").unwrap(), 2);
        let rows = list(&conn, "u2").unwrap();
        assert_eq!(rows.len(), 2);
        mark_all_read(&conn, "u2").unwrap();
        assert_eq!(unread_count(&conn, "u2").unwrap(), 0);
        assert!(list(&conn, "u2").unwrap().iter().all(|n| n.read));
    }
}
