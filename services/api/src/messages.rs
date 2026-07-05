//! 1:1 direct messages (server-owned — messaging is online-only, so the tables live in the server's
//! `ensure_schema`, NOT the shared vegify-core/desktop schema; both clients call these endpoints
//! live, exactly like auth). A conversation is an unordered user pair, normalized so `user_a < user_b`
//! (one row per pair, enforced by a unique index). Read state rides the message row (`read_at`) — in a
//! 1:1 thread the recipient is unambiguous, so no per-participant cursor table is needed.
//!
//! Addressing is by public handle (`users.username`): the UI flows from profiles, and handles are the
//! stable public identifier ([[docs/usernames.md]]). Sends fan out over the existing WS push as
//! `{"changed":"message"}` — a content-free nudge; clients refetch their own auth-scoped state.
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::AppError;

/// Body cap — generous for a DM, small enough that a hostile client can't stuff megabytes a row.
const MAX_BODY_CHARS: usize = 5_000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// The module's tables — called from the server's `ensure_schema` (additive, idempotent).
pub fn ensure_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            user_a TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            user_b TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at INTEGER,
            updated_at INTEGER
        );
        CREATE UNIQUE INDEX IF NOT EXISTS conversations_pair_uq ON conversations(user_a, user_b);
        CREATE INDEX IF NOT EXISTS conversations_user_b_idx ON conversations(user_b);
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            sender_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            body TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            read_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS messages_conversation_idx ON messages(conversation_id, created_at);
        CREATE INDEX IF NOT EXISTS messages_unread_idx ON messages(conversation_id, sender_id, read_at);",
    )
}

/// The other party, as the conversation list + thread header shows them.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Party {
    pub id: String,
    pub name: String,
    pub username: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub with: Party,
    pub last_body: String,
    pub last_at: i64,
    /// True when the last message is the viewer's own (the list renders "You: …").
    pub last_is_mine: bool,
    pub unread: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub body: String,
    pub created_at: i64,
    /// True when the viewer sent it (clients render alignment off this, not off raw ids).
    pub mine: bool,
}

/// A thread as the thread screen consumes it: the other party (resolved even before any message
/// exists, so a profile's "Message" button lands on an empty composer) + the messages, oldest first.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub with: Party,
    pub messages: Vec<Message>,
}

fn party_by_username(conn: &Connection, username: &str) -> Result<Party, AppError> {
    conn.query_row(
        "SELECT id, name, username FROM users WHERE username = ?1",
        [username],
        |r| {
            Ok(Party {
                id: r.get(0)?,
                name: r.get(1)?,
                username: r.get(2)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound("No such user.".into()))
}

/// The pair's conversation id, if one exists. Pair order is normalized (`user_a < user_b`).
fn conversation_id(conn: &Connection, me: &str, other: &str) -> Result<Option<String>, AppError> {
    let (a, b) = if me < other { (me, other) } else { (other, me) };
    Ok(conn
        .query_row(
            "SELECT id FROM conversations WHERE user_a = ?1 AND user_b = ?2",
            [a, b],
            |r| r.get(0),
        )
        .optional()?)
}

fn get_or_create_conversation(conn: &Connection, me: &str, other: &str) -> Result<String, AppError> {
    if let Some(id) = conversation_id(conn, me, other)? {
        return Ok(id);
    }
    let (a, b) = if me < other { (me, other) } else { (other, me) };
    let id = vegify_core::new_id();
    let now = now_ms();
    // A concurrent first-message race can beat us to the unique index — treat that as "it exists".
    match conn.execute(
        "INSERT INTO conversations (id, user_a, user_b, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
        params![id, a, b, now],
    ) {
        Ok(_) => Ok(id),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            conversation_id(conn, me, other)?.ok_or_else(|| AppError::Internal("conversation race".into()))
        }
        Err(e) => Err(e.into()),
    }
}

/// Send `body` to `to_username`. Creates the conversation on first contact. Returns the stored
/// message. Deliberately does NOT ring the bell: DMs have their own surface (the Mail badge) — the
/// bell is reserved for personal-impact events (see notifications.rs).
pub fn send(conn: &Connection, me_id: &str, to_username: &str, body: &str) -> Result<Message, AppError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(AppError::BadRequest("A message can't be empty.".into()));
    }
    if body.chars().count() > MAX_BODY_CHARS {
        return Err(AppError::BadRequest("That message is too long.".into()));
    }
    let to = party_by_username(conn, to_username)?;
    if to.id == me_id {
        return Err(AppError::BadRequest("You can't message yourself.".into()));
    }
    let conversation = get_or_create_conversation(conn, me_id, &to.id)?;
    let id = vegify_core::new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO messages (id, conversation_id, sender_id, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, conversation, me_id, body, now],
    )?;
    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        params![now, conversation],
    )?;
    Ok(Message {
        id,
        body: body.to_string(),
        created_at: now,
        mine: true,
    })
}

/// The viewer's conversations, most recently active first, with the other party, last message, and
/// their unread count (messages from the other party not yet read).
pub fn list_conversations(conn: &Connection, me_id: &str) -> Result<Vec<ConversationSummary>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT c.id,
                u.id, u.name, u.username,
                m.body, m.created_at, (m.sender_id = ?1),
                (SELECT COUNT(*) FROM messages um
                  WHERE um.conversation_id = c.id AND um.sender_id != ?1 AND um.read_at IS NULL)
           FROM conversations c
           JOIN users u ON u.id = CASE WHEN c.user_a = ?1 THEN c.user_b ELSE c.user_a END
           JOIN messages m ON m.id = (SELECT id FROM messages
                                       WHERE conversation_id = c.id
                                       ORDER BY created_at DESC, rowid DESC LIMIT 1)
          WHERE c.user_a = ?1 OR c.user_b = ?1
          ORDER BY m.created_at DESC, m.rowid DESC",
    )?;
    let rows = stmt
        .query_map([me_id], |r| {
            Ok(ConversationSummary {
                id: r.get(0)?,
                with: Party {
                    id: r.get(1)?,
                    name: r.get(2)?,
                    username: r.get(3)?,
                },
                last_body: r.get(4)?,
                last_at: r.get(5)?,
                last_is_mine: r.get(6)?,
                unread: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The thread with `other_username`, oldest first — and mark their messages read (opening a thread IS
/// reading it; the deliberate GET side-effect saves the clients a round trip). Resolves the party even
/// when no conversation exists yet, so "Message" from a profile lands on an empty composer.
pub fn thread(conn: &Connection, me_id: &str, other_username: &str) -> Result<Thread, AppError> {
    let with = party_by_username(conn, other_username)?;
    if with.id == me_id {
        return Err(AppError::BadRequest("You can't message yourself.".into()));
    }
    let Some(conversation) = conversation_id(conn, me_id, &with.id)? else {
        return Ok(Thread { with, messages: vec![] });
    };
    conn.execute(
        "UPDATE messages SET read_at = ?1 WHERE conversation_id = ?2 AND sender_id != ?3 AND read_at IS NULL",
        params![now_ms(), conversation, me_id],
    )?;
    // rowid tie-break: ULIDs don't order within one millisecond, but insertion order (rowid) always does.
    let mut stmt = conn.prepare(
        "SELECT id, body, created_at, (sender_id = ?2) FROM messages
          WHERE conversation_id = ?1 ORDER BY created_at ASC, rowid ASC",
    )?;
    let messages = stmt
        .query_map(params![conversation, me_id], |r| {
            Ok(Message {
                id: r.get(0)?,
                body: r.get(1)?,
                created_at: r.get(2)?,
                mine: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Thread { with, messages })
}

/// Total unread messages across the viewer's conversations — the chrome badge number.
pub fn unread_count(conn: &Connection, me_id: &str) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM messages m
           JOIN conversations c ON c.id = m.conversation_id
          WHERE (c.user_a = ?1 OR c.user_b = ?1) AND m.sender_id != ?1 AND m.read_at IS NULL",
        [me_id],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL, email TEXT NOT NULL, username TEXT);
             INSERT INTO users (id, name, email, username) VALUES
               ('u1', 'Ada', 'ada@x', 'ada'), ('u2', 'Grace', 'grace@x', 'grace'), ('u3', 'Alan', 'alan@x', 'alan');",
        )
        .unwrap();
        ensure_tables(&conn).unwrap();
        conn
    }

    #[test]
    fn send_creates_one_conversation_per_pair_regardless_of_direction() {
        let conn = test_conn();
        send(&conn, "u1", "grace", "hi").unwrap();
        send(&conn, "u2", "ada", "hi back").unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "both directions share the normalized pair row");
    }

    #[test]
    fn thread_marks_only_their_messages_read_and_unread_counts_track() {
        let conn = test_conn();
        send(&conn, "u1", "grace", "one").unwrap();
        send(&conn, "u1", "grace", "two").unwrap();
        send(&conn, "u2", "ada", "reply").unwrap();
        assert_eq!(unread_count(&conn, "u2").unwrap(), 2);
        assert_eq!(unread_count(&conn, "u1").unwrap(), 1);
        // Grace opens the thread: her unread drains; Ada's reply stays unread for Ada until SHE opens.
        let t = thread(&conn, "u2", "ada").unwrap();
        assert_eq!(t.messages.len(), 3);
        assert_eq!(unread_count(&conn, "u2").unwrap(), 0);
        assert_eq!(unread_count(&conn, "u1").unwrap(), 1);
        let mine: Vec<bool> = t.messages.iter().map(|m| m.mine).collect();
        assert_eq!(mine, vec![false, false, true], "alignment flags are viewer-relative");
    }

    #[test]
    fn conversation_list_shows_last_message_and_unread_per_other_party() {
        let conn = test_conn();
        send(&conn, "u2", "ada", "from grace").unwrap();
        send(&conn, "u3", "ada", "from alan").unwrap();
        let list = list_conversations(&conn, "u1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].with.username, "alan", "most recently active first");
        assert_eq!(list[0].last_body, "from alan");
        assert!(!list[0].last_is_mine);
        assert_eq!(list[0].unread, 1);
    }

    #[test]
    fn guards_empty_self_unknown_and_oversized() {
        let conn = test_conn();
        assert!(matches!(send(&conn, "u1", "ada", "hi"), Err(AppError::BadRequest(_))), "self-send refused");
        assert!(matches!(send(&conn, "u1", "nobody", "hi"), Err(AppError::NotFound(_))));
        assert!(matches!(send(&conn, "u1", "grace", "   "), Err(AppError::BadRequest(_))));
        let big = "x".repeat(MAX_BODY_CHARS + 1);
        assert!(matches!(send(&conn, "u1", "grace", &big), Err(AppError::BadRequest(_))));
        // A thread with a stranger (no conversation yet) resolves the party with zero messages.
        let t = thread(&conn, "u1", "grace").unwrap();
        assert_eq!(t.with.username, "grace");
        assert!(t.messages.is_empty());
    }
}
