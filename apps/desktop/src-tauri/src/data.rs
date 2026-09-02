//! On-device DAL adapter + the content-API sync engine for the Tauri desktop shell.
//!
//! The shared content model — the types, mutations, nutrition CTE, visibility policy, and the
//! viewer-scoped reads — lives in `vegify-core`, consumed by both this desktop shell and the server.
//! This module is the DESKTOP ADAPTER: the `#[procedures]` IPC trait (thin wrappers over vegify-core,
//! threading the signed-in viewer + the locked connection), plus the desktop-only concerns vegify-core
//! deliberately excludes — sign-in over HTTPS, the OS keychain, the `_outbox` push queue, and the
//! content-API sync engine. `sync_now` (push the outbox to the content API, then pull/reconcile) runs
//! on sign-in, after writes (debounced), and periodically. Client ids are authoritative ULIDs.

use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use ttipc::procedures;

// The shared DAL, re-exported so this module's IPC trait AND the test module (`use super::*`) see the
// content types, mutations, and reads as if local. vegify-core glob-exports no `Result`/`Error` alias,
// so std `Result` here is unaffected; `vegify_core::Error` is referenced fully-qualified below.
pub use vegify_core::*;

/// The IPC-facing error. Carries ttipc's `Error` derive (the `{type, message}` wire shape + the
/// `ErrorSet` binding descriptor) — which can't live in vegify-core because ttipc pulls in Tauri.
/// `From<vegify_core::Error>` adapts the shared DAL's error so `?` flows through the trait methods.
#[derive(Debug, ttipc::Error)]
pub enum DataError {
    /// SQLite failure, stringified for the ttipc boundary.
    Db(String),
    /// Auth failure (bad credentials, expired session), stringified.
    Auth(String),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataError::Db(m) => write!(f, "{m}"),
            DataError::Auth(m) => write!(f, "{m}"),
        }
    }
}

impl From<rusqlite::Error> for DataError {
    fn from(e: rusqlite::Error) -> Self {
        DataError::Db(e.to_string())
    }
}

impl From<vegify_core::Error> for DataError {
    fn from(e: vegify_core::Error) -> Self {
        match e {
            vegify_core::Error::Db(m) => DataError::Db(m),
            vegify_core::Error::Auth(m) => DataError::Auth(m),
        }
    }
}

// The API client SDK — the auth/content/messages/notifications transport + the keychain session
// store, EXTRACTED from this module (the app consumes the SDK; applications are leaves). The wire
// types are re-exported under their original names so the IPC trait, the generated bindings, and
// the tests are unchanged.
pub use vegify_client::{
    AuthUser, ConversationSummary, DmNotification, Message, Party, Session, Thread,
};
use vegify_client::{SessionStore, VegifyClient};

impl From<vegify_client::Error> for DataError {
    fn from(e: vegify_client::Error) -> Self {
        match e {
            vegify_client::Error::Auth(m) => DataError::Auth(m),
            vegify_client::Error::Api(m) | vegify_client::Error::Network(m) => DataError::Db(m),
        }
    }
}

// Opaque stable keychain key predating the bundle-identifier unification — renaming it would log
// out every existing install for no functional gain (same rule as the config crate's app-data dir
// name).
const KEYCHAIN_SERVICE: &str = "app.vegify.desktop";
const KEYCHAIN_ACCOUNT: &str = "session";

/// The SDK client against the configured backend (runtime override → build-time bake → placeholder
/// — resolution lives in vegify-config). Stateless; constructed per use.
fn client() -> VegifyClient {
    VegifyClient::new(vegify_config::desktop::server_url())
}

/// This app's keychain slot for the session.
fn session_store() -> SessionStore {
    SessionStore::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
}

// ---- auth (desktop sign-in over HTTPS → token in the OS keychain) ----

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// Sign-in form payload.
pub struct SignInInput {
    /// Login email.
    pub email: String,
    /// Plaintext password (sent to the server, never stored).
    pub password: String,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// Sign-up form payload.
pub struct SignUpInput {
    /// Display name.
    pub name: String,
    /// Login email.
    pub email: String,
    /// Plaintext password (sent to the server, never stored).
    pub password: String,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// Password-reset request payload.
pub struct ResetRequestInput {
    /// The account email to send the reset link to.
    pub email: String,
}

#[derive(Deserialize, Type)]
/// Send-DM payload.
pub struct SendMessageInput {
    /// Recipient username.
    pub to: String,
    /// Message body (plain text).
    pub body: String,
}

/// Background realtime-push loop: connect to the server's `/ws`, and on every change frame emit a
/// `server-content-changed` Tauri event so the frontend pulls immediately — the realtime replacement for
/// the 60s poll. Reconnects with capped exponential backoff; re-reads the session token from the keychain
/// each attempt, so a sign-in / sign-out is picked up (no token → wait, then retry). The auth token rides
/// the WS handshake as a Bearer header (kept out of the URL, so it never lands in request logs). Runs on
/// its own current-thread tokio runtime (spawned from `main`'s setup) — independent of the sync ureq path.
pub async fn run_ws_push(app: tauri::AppHandle) {
    use futures_util::StreamExt;
    use tauri::Emitter;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;
    use tokio_tungstenite::tungstenite::Message;

    let mut backoff_secs = 1u64;
    loop {
        // Re-read each attempt: None → not signed in yet (or signed out). Nothing to subscribe as; wait.
        let Some(token) = session_store().load().map(|s| s.token) else {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            continue;
        };

        let req = client().ws_url().into_client_request().map(|mut req| {
            if let Ok(v) = HeaderValue::from_str(&format!("Bearer {token}")) {
                req.headers_mut().insert("authorization", v);
            }
            req
        });
        match req {
            Ok(req) => match tokio_tungstenite::connect_async(req).await {
                Ok((mut stream, _resp)) => {
                    backoff_secs = 1; // connected — reset the reconnect backoff
                    tracing::info!("ws push connected");
                    while let Some(frame) = stream.next().await {
                        match frame {
                            // Any change frame → pull now (the frontend listener calls scheduleSync(0)).
                            // Notification frames ALSO get their own event: the frontend fires a native
                            // toast + refetches the bell off it (the generic event only means "re-pull").
                            Ok(Message::Text(payload)) => {
                                tracing::info!(change = %payload, "ws push: change received");
                                let _ = app.emit("server-content-changed", ());
                                let is_notification =
                                    serde_json::from_str::<serde_json::Value>(&payload)
                                        .ok()
                                        .and_then(|v| {
                                            v.get("changed")
                                                .and_then(|c| c.as_str().map(String::from))
                                        })
                                        .is_some_and(|kind| kind == "notification");
                                if is_notification {
                                    let _ = app.emit("server-notification", ());
                                }
                            }
                            Ok(Message::Close(_)) | Err(_) => break,
                            Ok(_) => {} // ping/pong handled by tungstenite; ignore other frames
                        }
                    }
                    tracing::info!("ws push disconnected; reconnecting");
                }
                Err(e) => tracing::warn!(error = %e, "ws push connect failed"),
            },
            Err(e) => tracing::warn!(error = %e, "ws push bad url"),
        }

        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
        backoff_secs = backoff_secs.saturating_mul(2).min(30);
    }
}

/// Extract the `id` from a delete outbox payload (`{ "id": "…" }`).
fn payload_id(p: &serde_json::Value) -> Result<&str, DataError> {
    p.get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| DataError::Db("outbox delete payload missing id".into()))
}

/// Reconcile the local content cache to a server pull. Inside ONE transaction (FK already disabled by
/// the caller — PRAGMA foreign_keys is a no-op mid-transaction), clear the content tables — keeping
/// the `nutrients` name catalog and the meta tables — then re-apply every pulled row via
/// vegify-core's do_save_* stamped with its REAL owner (so per-viewer gates mirror the server).
/// Pruning falls out: anything the pull no longer returns simply isn't recreated. The caller pushes
/// first, so no unpushed local create is lost. Atomic: any error rolls back, leaving the cache intact.
///
/// `users` is reconciled rather than wiped: pull-owned rows are the payload's creators (public
/// identity; synthetic email = the user id, which never contains '@', marking them pull-owned under
/// the cache's NOT NULL + UNIQUE email), replaced wholesale each pull so creator handles and
/// `/<username>` profiles resolve on-device — logged out included. Auth-owned rows (a real email:
/// the signed-in user via ensure_user_local, dev seeds) only ever get their public fields refreshed,
/// never their email.
fn apply_pull(
    conn: &mut Connection,
    payload: &vegify_client::PullPayload,
) -> Result<(), DataError> {
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DELETE FROM ingredient_in_recipe;
         DELETE FROM ingredient_nutrient;
         DELETE FROM recipes;
         DELETE FROM ingredients;
         DELETE FROM amounts;",
    )?;
    tx.execute("DELETE FROM users WHERE email NOT LIKE '%@%'", [])?;
    for u in &payload.users {
        tx.execute(
            "INSERT INTO users(id, name, username, avatar_key, email) VALUES (?1, ?2, ?3, ?4, ?1)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, username = excluded.username,
                                           avatar_key = excluded.avatar_key",
            params![u.id, u.name, u.username, u.avatarKey],
        )?;
    }
    for ing in &payload.ingredients {
        let input = SaveIngredientInput {
            id: Some(ing.id.clone()),
            visibility: Some(ing.visibility.clone().into()),
            name: ing.name.clone(),
            description: ing.description.clone(),
            price: ing.price,
            calories_per_100g: ing.caloriesPer100g,
            serving_grams: ing.servingGrams,
            serving_unit: ing.servingUnit.clone(),
            package_grams: ing.packageGrams,
            nutrients: ing
                .nutrients
                .iter()
                .map(|n| IngredientNutrientInput {
                    name: n.name.clone(),
                    amount_per_100g: n.amountPer100g,
                    unit: n.unit.clone(),
                })
                .collect(),
            slug: ing.slug.clone(), // server-authoritative; store verbatim, don't regenerate
        };
        do_save_ingredient(&tx, &input, ing.userId.as_deref())?;
        // The tombstone rides OUTSIDE the mutation shape (user edits must never touch it) — stamp it
        // after the save, exactly as the server pull reported it.
        if let Some(ts) = ing.deletedAt {
            tx.execute(
                "UPDATE ingredients SET deleted_at = ?1 WHERE id = ?2",
                params![ts, ing.id],
            )?;
        }
    }
    for r in &payload.recipes {
        let input = SaveRecipeInput {
            id: Some(r.id.clone()),
            as_ingredient_id: Some(r.asIngredientId.clone()),
            visibility: Some(r.visibility.clone().into()),
            name: r.name.clone(),
            subtitle: r.subtitle.clone(),
            directions: r.directions.clone(),
            serving_grams: r.servingGrams,
            batch_grams: r.batchGrams,
            items: r
                .items
                .iter()
                .map(|it| RecipeItemInput {
                    ingredient_id: it.ingredientId.clone(),
                    grams: it.grams,
                    unit: it.unit.clone(),
                    amount: it.amount,
                })
                .collect(),
            slug: r.slug.clone(), // server-authoritative
        };
        do_save_recipe(&tx, &input, r.userId.as_deref())?;
    }
    tx.commit()?;
    Ok(())
}

/// The desktop-local `_outbox` push queue, created on open: one semantic mutation `{op, payload}` per
/// local content write, drained FIFO by the sync engine to the content API. `seq` AUTOINCREMENT gives
/// deterministic order (ULIDs aren't monotonic within a millisecond) and is never reused after a
/// drained row is deleted. Local-only — the server is the source of truth, not a synced changeset.
fn init_meta_tables(conn: &Connection) -> Result<(), DataError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _outbox(seq INTEGER PRIMARY KEY AUTOINCREMENT, op TEXT NOT NULL, payload TEXT NOT NULL);",
    )?;
    Ok(())
}

/// The content tables the local cache holds (users/ingredients/recipes/amounts/ingredient_in_recipe/
/// ingredient_nutrient/nutrients/…). Generated from the Drizzle dev DB into `schema.sql` and run
/// idempotently on every open — every statement is `IF NOT EXISTS`, so it's a no-op once the tables
/// exist — mirroring the server's own boot-time `ensure_schema`. Dev's `.data/vegify.db` already
/// carries this schema (from `pnpm db:push`); the load-bearing case is a SHIPPED build's FRESH
/// app-data DB, where WITHOUT this the first sign-in pull (`apply_pull` → vegify-core `do_save_*`)
/// fails with "no such table". `schema_sql_matches_drizzle_dev_db` (below) guards `schema.sql` against
/// drifting from Drizzle.
fn ensure_content_schema(conn: &Connection) -> Result<(), DataError> {
    conn.execute_batch(include_str!("../schema.sql"))?;
    // `users.username` (creator handles) postdates the original cache schema. schema.sql's
    // `CREATE TABLE IF NOT EXISTS` can't alter an existing table, so add the column idempotently here
    // (mirroring the server's ensure_schema); the next pull/sign-in refills it from the server.
    let has_username: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('users') WHERE name = 'username'",
        [],
        |r| r.get(0),
    )?;
    if has_username == 0 {
        conn.execute("ALTER TABLE users ADD COLUMN username TEXT", [])?;
    }
    let has_avatar: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('users') WHERE name = 'avatar_key'",
        [],
        |r| r.get(0),
    )?;
    if has_avatar == 0 {
        conn.execute("ALTER TABLE users ADD COLUMN avatar_key TEXT", [])?;
    }
    // Same story for the two ingredient columns that postdate shipped caches: provenance (`source`)
    // and the soft-delete tombstone (`deleted_at`). Fresh DBs get them from schema.sql's CREATE.
    for (col, ddl) in [
        ("source", "ALTER TABLE ingredients ADD COLUMN source TEXT"),
        (
            "deleted_at",
            "ALTER TABLE ingredients ADD COLUMN deleted_at INTEGER",
        ),
    ] {
        let present: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('ingredients') WHERE name = ?1",
            [col],
            |r| r.get(0),
        )?;
        if present == 0 {
            conn.execute(ddl, [])?;
        }
    }
    // `ingredients.slug` (SEO URL segment) postdates the original cache schema too. schema.sql adds it
    // to fresh caches + creates slug_history; for an existing cache add the column idempotently here.
    // The desktop is a cache: the next pull refills slugs from the server (no local backfill needed).
    let has_slug: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('ingredients') WHERE name = 'slug'",
        [],
        |r| r.get(0),
    )?;
    if has_slug == 0 {
        conn.execute("ALTER TABLE ingredients ADD COLUMN slug TEXT", [])?;
    }
    // FTS5 unified search index over ingredient/recipe names (P2.4) — shared setup with the server's
    // `ensure_schema`, so both sides can never drift (see vegify_core::ensure_search_index).
    vegify_core::ensure_search_index(conn)?;
    Ok(())
}

/// Serialize a mutation input to its content-API JSON body (camelCase). Used to build an outbox payload.
fn to_json<T: Serialize>(v: &T) -> Result<serde_json::Value, DataError> {
    serde_json::to_value(v).map_err(|e| DataError::Db(e.to_string()))
}

/// The local SQLite database plus the in-memory session slot — the
/// desktop's single data handle, shared behind tauri state.
pub struct Db {
    conn: Mutex<Connection>,
    auth: Mutex<Option<Session>>,
}

impl Db {
    /// Open (creating if missing) the database at `db_path` and run
    /// migrations; restores any keychain session into the slot.
    pub fn open(db_path: &str) -> Result<Self, DataError> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
        )
        .ok();
        init_meta_tables(&conn)?;
        ensure_content_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            auth: Mutex::new(session_store().load()),
        })
    }

    /// Run a write with the connection locked. (Formerly captured a SQLite changeset for the S3 sync
    /// mesh; the server is the source of truth now — content writes propagate via the `_outbox` and
    /// the sync engine, so the write just runs.)
    /// The SQLite handle, recovering from a poisoned lock: a panicked
    /// holder's open transaction has already rolled back (rusqlite's drop
    /// is rollback), so the connection is consistent to reuse.
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The in-memory session slot, recovering from a poisoned lock the same
    /// way (the slot holds plain data; last-written state is valid).
    fn auth_slot(&self) -> std::sync::MutexGuard<'_, Option<Session>> {
        self.auth
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn with_conn<T>(
        &self,
        write: impl FnOnce(&Connection) -> Result<T, DataError>,
    ) -> Result<T, DataError> {
        write(&self.conn())
    }

    /// Append a semantic mutation `{op, payload}` to the local push queue. `_outbox` is device-local
    /// (the server is the source of truth); the sync engine drains it in `seq` order, pushing each to
    /// the content API and deleting the row on success.
    fn enqueue(&self, op: &str, payload: serde_json::Value) -> Result<(), DataError> {
        let json = serde_json::to_string(&payload).map_err(|e| DataError::Db(e.to_string()))?;
        self.conn().execute(
            "INSERT INTO _outbox(op, payload) VALUES (?1, ?2)",
            params![op, json],
        )?;
        Ok(())
    }

    fn current_uid(&self) -> Option<String> {
        self.auth_slot().as_ref().map(|s| s.user.id.clone())
    }

    /// The current opaque session token (for the content API's Bearer auth + server-side logout).
    fn current_token(&self) -> Option<String> {
        self.auth_slot().as_ref().map(|s| s.token.clone())
    }

    /// The signed-in user id, or an auth error. WRITES require a session — you may only create or edit
    /// your OWN content; reads use `current_uid` (an anonymous viewer simply sees public content).
    fn require_uid(&self) -> Result<String, DataError> {
        self.current_uid().ok_or_else(|| {
            DataError::Auth("Sign in to add or edit recipes and ingredients.".into())
        })
    }

    /// The session token, or an auth error — for the online-only endpoints (messages) where an
    /// anonymous fallback makes no sense.
    fn require_token(&self) -> Result<String, DataError> {
        self.current_token()
            .ok_or_else(|| DataError::Auth("Sign in to use messages.".into()))
    }

    /// Push: drain the outbox to the content API in FIFO (`seq`) order, deleting each row on success.
    /// Stops at the first failure — the unpushed tail stays queued, so order holds and a re-push is
    /// idempotent (every payload carries its client id → the server upserts). The connection mutex is
    /// NOT held during the HTTP call. An empty outbox is a no-op (no token required).
    fn push(&self) -> Result<(), DataError> {
        loop {
            let next: Option<(i64, String, String)> = {
                let conn = self.conn();
                conn.query_row(
                    "SELECT seq, op, payload FROM _outbox ORDER BY seq LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?
            };
            let Some((seq, op, payload_json)) = next else {
                tracing::debug!("push: outbox empty");
                return Ok(());
            };
            let token = self
                .current_token()
                .ok_or_else(|| DataError::Auth("Not signed in.".into()))?;
            let payload: serde_json::Value =
                serde_json::from_str(&payload_json).map_err(|e| DataError::Db(e.to_string()))?;
            tracing::info!(seq, op = %op, "push: sending outbox item");
            let client = client();
            match op.as_str() {
                "saveRecipe" => client.content_post(&token, "recipes", &payload)?,
                "saveIngredient" => client.content_post(&token, "ingredients", &payload)?,
                "deleteRecipe" => {
                    client.content_delete(&token, "recipes", payload_id(&payload)?)?
                }
                "deleteIngredient" => {
                    client.content_delete(&token, "ingredients", payload_id(&payload)?)?
                }
                "restoreIngredient" => client.restore_ingredient(&token, payload_id(&payload)?)?,
                "saveLogEntry" => client.log_post(&token, &payload)?,
                "deleteLogEntry" => client.log_delete(&token, payload_id(&payload)?)?,
                "saveProfile" => client.profile_post(&token, &payload)?,
                "saveDaySupplements" => client.day_supplements_post(&token, &payload)?,
                other => return Err(DataError::Db(format!("unknown outbox op: {other}"))),
            }
            self.conn()
                .execute("DELETE FROM _outbox WHERE seq = ?1", params![seq])?;
        }
    }

    /// Pull: replace the local content cache with the server's listed world for this viewer (apply +
    /// prune in one FK-off transaction — see apply_pull). MUST run after a full push, so a local create
    /// sitting in the outbox is already on the server (hence in the pull) before the rebuild.
    fn pull(&self) -> Result<(), DataError> {
        // Anonymous-capable: signed in → public + own; logged out → public only. A logged-out desktop
        // still fills and rebuilds its local cache from the server's public content this way.
        let token = self.current_token();
        let payload = client().content_pull(token.as_deref())?;
        tracing::info!(
            recipes = payload.recipes.len(),
            ingredients = payload.ingredients.len(),
            "pull: rebuilding local cache"
        );
        let mut conn = self.conn();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").ok();
        let res = apply_pull(&mut conn, &payload);
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        res
    }

    /// Pull the viewer's authoritative diary into the local cache — a SEPARATE authed channel from
    /// `pull()` (the anonymous /api/content/pull never carries private log data). Runs only when signed
    /// in. Reconciles via `vegify_core::apply_log_pull` — its OWN rebuild, deliberately NOT folded into
    /// `apply_pull` (which wipes the shared `amounts` table wholesale; the diary references it). MUST run
    /// AFTER `pull()` so a logged food's ingredient is already in the local cache when the diary's
    /// RESTRICT FK resolves. FK off during the rebuild so an entry whose ingredient hasn't synced yet
    /// doesn't block the apply (log_day simply omits it until the next content pull catches up).
    fn pull_diary(&self) -> Result<(), DataError> {
        let Some(token) = self.current_token() else {
            return Ok(()); // signed out: no private diary to pull
        };
        let uid = self.require_uid()?;
        let pull = client().log_pull(&token)?;
        tracing::info!(entries = pull.entries.len(), "pull: rebuilding local diary");
        let conn = self.conn();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").ok();
        let res = vegify_core::apply_log_pull(&conn, &uid, &pull).map_err(Into::into);
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        res
    }

    /// Pull the viewer's authoritative nutrition profile into the local cache — another authed-only
    /// channel (the profile is PRIVATE, like the diary; never in the anonymous content pull). Upserts
    /// the single per-user `profiles` row via the same DAL the local write uses, so the server's value
    /// wins (server is the source of truth). Runs only when signed in; a no-op otherwise. Cheap — one
    /// row — so it rides every sync. The signed-in `users` row already exists (ensure_user_local), so
    /// the profile's FK resolves with foreign keys left ON.
    fn pull_profile(&self) -> Result<(), DataError> {
        let Some(token) = self.current_token() else {
            return Ok(()); // signed out: no private profile to pull
        };
        let uid = self.require_uid()?;
        let profile = client().profile_get(&token)?;
        let conn = self.conn();
        vegify_core::save_nutrition_profile(&conn, &uid, &profile).map_err(Into::into)
    }

    /// Upsert the signed-in user into the local `users` table so write-time foreign keys (and the
    /// recipe `creator`) resolve on-device. Identity is auth state, not synced content.
    ///
    /// Reconcile by email: if a DIFFERENT local id already holds this email — a separately-seeded
    /// cache, e.g. the dev seed's john (`01KVX…`) vs the server's john (`01KVE…`), the id-divergence
    /// bug class — re-point that user's content to the server id and drop the stale row, so the cache
    /// adopts the server's authoritative identity (else the insert trips `UNIQUE users.email`). FK off
    /// so the content reassignment + the PK swap don't trip mid-update; the bootstrap pull then
    /// rebuilds content under the server owner anyway.
    fn ensure_user_local(&self, user: &AuthUser) -> Result<(), DataError> {
        let conn = self.conn();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").ok();
        let res = (|| -> Result<(), DataError> {
            let stale: Option<String> = conn
                .query_row(
                    "SELECT id FROM users WHERE email = ?1 AND id <> ?2",
                    params![user.email, user.id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(stale_id) = stale {
                conn.execute(
                    "UPDATE ingredients SET user_id = ?1 WHERE user_id = ?2",
                    params![user.id, stale_id],
                )?;
                conn.execute("DELETE FROM users WHERE id = ?1", params![stale_id])?;
            }
            conn.execute(
                "INSERT INTO users(id, name, username, email) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, username = excluded.username, email = excluded.email",
                params![user.id, user.name, user.username, user.email],
            )?;
            Ok(())
        })();
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        res
    }
}

#[procedures]
/// The desktop data surface, 1:1 with the ttipc commands: local-first
/// reads and writes against the mirror, sessions, DMs, notifications.
/// Writes enqueue outbox mutations for the sync engine.
pub trait VegifyData {
    /// One catalog page of recipe cards visible to the current user.
    fn list_recipes(&self, page: Page) -> Result<Vec<RecipeCard>, DataError>;
    /// Full recipe detail; None when the id is unknown or not visible.
    fn recipe(&self, id: String) -> Result<Option<RecipeView>, DataError>;
    /// A user's public profile by handle, from the local cache.
    fn get_profile(&self, username: String) -> Result<Option<Profile>, DataError>;
    /// Resolve a recipe slug (current or historical) under an owner handle;
    /// the caller 301s when the hit reports a newer canonical slug.
    fn resolve_recipe_by_slug(
        &self,
        username: String,
        slug: String,
    ) -> Result<Option<RecipeSlugHit>, DataError>;
    /// Resolve an ingredient slug (current or historical); see
    /// `resolve_recipe_by_slug` for the 301 contract.
    fn resolve_ingredient_by_slug(
        &self,
        slug: String,
    ) -> Result<Option<IngredientSlugHit>, DataError>;
    /// Owner-only edit-mode load of a recipe.
    fn recipe_for_edit(&self, id: String) -> Result<Option<RecipeEditData>, DataError>;
    /// One catalog page of leaf-ingredient cards.
    fn list_ingredients(&self, page: Page) -> Result<Vec<IngredientCard>, DataError>;
    /// Ingredient detail (readable rows; viewer-scoped).
    fn ingredient(&self, id: String) -> Result<Option<IngredientEditData>, DataError>;
    /// Owner-only edit-mode load of an ingredient.
    fn ingredient_for_edit(&self, id: String) -> Result<Option<IngredientEditData>, DataError>;
    /// Name search over visible ingredients (the recipe composer's box).
    fn search_ingredients(&self, query: String) -> Result<Vec<IngredientSearchResult>, DataError>;
    /// Unified ranked search over recipes + standalone ingredients — the chrome/global search box
    /// (replaces the old client-side full-catalog filter; P2.4).
    fn search_content(&self, query: String) -> Result<ContentSearchResult, DataError>;
    /// Create or update an ingredient; returns its id. Enqueues the
    /// mutation for sync.
    fn save_ingredient(&self, input: SaveIngredientInput) -> Result<String, DataError>;
    /// Soft-delete an ingredient (tombstone). Enqueues for sync.
    fn delete_ingredient(&self, id: String) -> Result<(), DataError>;
    /// Undo a soft delete (the greyed recipe row's "restore?" affordance). Owner-gated in the DAL.
    fn restore_ingredient(&self, id: String) -> Result<(), DataError>;
    /// Create or update a recipe; returns its id. Enqueues for sync.
    fn save_recipe(&self, input: SaveRecipeInput) -> Result<String, DataError>;
    /// Delete a recipe and its as-ingredient pair. Enqueues for sync.
    fn delete_recipe(&self, id: String) -> Result<(), DataError>;
    /// One diary day (entries + rolled-up nutrient totals) from the local cache. Authed-only — the
    /// diary is PRIVATE, so unlike content reads there is no anonymous fallback.
    fn log_day(&self, date: String) -> Result<DayLog, DataError>;
    /// The viewer's recently-logged ingredients, newest first, for the add-flow. Authed-only.
    fn log_recents(&self, limit: f64) -> Result<Vec<RecentIngredient>, DataError>;
    /// Log or update a diary entry (freezes the nutrition snapshot); returns its id. Enqueues for sync.
    fn save_log_entry(&self, input: SaveLogEntryInput) -> Result<String, DataError>;
    /// Soft-delete a diary entry. Enqueues for sync.
    fn delete_log_entry(&self, id: String) -> Result<(), DataError>;
    /// The viewer's nutrition profile from the local cache (all-null defaults when never set). Authed-only
    /// — the profile is PRIVATE, so unlike content reads there is no anonymous fallback. Drives targets.
    fn get_nutrition_profile(&self) -> Result<NutritionProfile, DataError>;
    /// Upsert the viewer's nutrition profile (age/sex/weight/pregnancy). Enqueues for sync, so the next
    /// `sync_now` pushes it and the server's other devices re-pull. Authed-only.
    fn save_nutrition_profile(&self, input: NutritionProfile) -> Result<(), DataError>;
    /// Upsert the supplements taken on a day (the day's plan). Writes locally + enqueues for sync; the
    /// day's effective supplements come back in `log_day` (carry-forward). Authed-only — private per-day.
    fn save_day_supplements(&self, input: DaySupplementsRecord) -> Result<(), DataError>;
    /// Branded/packaged-food search — an ONLINE-ONLY proxy to `/api/branded/search`. The branded
    /// cache and the third-party API key are server-side by design (gate D1, option 2), so this shell
    /// asks the server rather than USDA. Public: no session needed, exactly like the catalog.
    fn branded_search(&self, query: String) -> Result<Vec<BrandedFood>, DataError>;
    /// Resolve a scanned/typed barcode — an online-only proxy to `/api/branded/barcode`. `None` means
    /// no source knows that GTIN, which is a normal outcome ("type it in yourself"), not an error.
    fn branded_barcode(&self, gtin: String) -> Result<Option<BrandedFood>, DataError>;
    /// **Promote-on-first-use**: join a looked-up branded food into the communal catalog and return
    /// its ingredient id. Authed. Pulls before returning, so the promoted ingredient is in the LOCAL
    /// cache by the time the caller writes a diary entry or a recipe row against that id.
    fn branded_promote(
        &self,
        source: BrandedSource,
        external_id: String,
    ) -> Result<String, DataError>;
    /// One content-API sync pass: push the outbox, then pull/reconcile. The bootstrap-on-sign-in, the
    /// debounced auto-sync, and the manual Sync button all call this.
    fn sync_now(&self) -> Result<(), DataError>;
    /// The signed-in user, if a session is live.
    fn current_user(&self) -> Result<Option<AuthUser>, DataError>;
    /// The backend base URL — the frontend composes media URLs (`<base>/<photoKey>`) from it, since
    /// photos are served from the server's CloudFront, not the local cache.
    fn media_base(&self) -> Result<String, DataError>;
    /// Sign in against the server, persist the session, then pull.
    fn sign_in(&self, input: SignInInput) -> Result<AuthUser, DataError>;
    /// Create an account, persist the session, then pull.
    fn sign_up(&self, input: SignUpInput) -> Result<AuthUser, DataError>;
    /// Clear the session (memory + keychain).
    fn sign_out(&self) -> Result<(), DataError>;
    /// Enumeration-safe: POST the email to the backend's reset-request route and always succeed. The
    /// reset itself is finished in the browser via the email link — no token round-trips to desktop.
    fn request_password_reset(&self, input: ResetRequestInput) -> Result<(), DataError>;
    /// Resend the email-verification link (enumeration-safe; the confirm happens in the browser via the
    /// emailed link, exactly like reset). Always succeeds.
    fn request_email_verification(&self, input: ResetRequestInput) -> Result<(), DataError>;
    /// 1:1 DMs — online-only proxies to /api/messages/* (no local cache; auth required).
    fn message_conversations(&self) -> Result<Vec<ConversationSummary>, DataError>;
    /// The DM thread with `username`, oldest first.
    fn message_thread(&self, username: String) -> Result<Thread, DataError>;
    /// Send a DM; returns the created message.
    fn send_message(&self, input: SendMessageInput) -> Result<Message, DataError>;
    /// Count of unread DMs (f64 mirrors the wire).
    fn messages_unread(&self) -> Result<f64, DataError>;
    /// The bell — online-only proxies to /api/notifications (auth required).
    fn notifications(&self) -> Result<Vec<DmNotification>, DataError>;
    /// Count of unread notifications (f64 mirrors the wire).
    fn notifications_unread(&self) -> Result<f64, DataError>;
    /// Mark every notification read.
    fn notifications_mark_read(&self) -> Result<(), DataError>;
    /// UGC safety (App Review 1.2): report content/users, block/unblock a user.
    fn report_content(
        &self,
        target_type: String,
        target_id: String,
        reason: String,
        note: String,
    ) -> Result<(), DataError>;
    /// Block `username`: hides their content and stops their DMs.
    fn block_user(&self, username: String) -> Result<(), DataError>;
    /// Unblock `username`.
    fn unblock_user(&self, username: String) -> Result<(), DataError>;
    /// Delete the signed-in account (App Review 5.1.1(v)); password-reconfirmed, then signs out locally.
    fn delete_account(&self, password: String) -> Result<(), DataError>;
}

// The trait methods are thin desktop adapters: derive the viewer from the cached session, lock the
// connection, and delegate reads/mutations to vegify-core (one impl shared with the server). Writes
// additionally mint client ids up front and enqueue a semantic mutation for the sync engine.
impl VegifyData for Db {
    fn list_recipes(&self, page: Page) -> Result<Vec<RecipeCard>, DataError> {
        let me = self.current_uid();
        let conn = self.conn();
        vegify_core::list_recipes(&conn, me.as_deref(), &page).map_err(Into::into)
    }

    fn recipe(&self, id: String) -> Result<Option<RecipeView>, DataError> {
        let me = self.current_uid();
        let conn = self.conn();
        vegify_core::recipe(&conn, id, me.as_deref()).map_err(Into::into)
    }

    /// A user's public profile by handle, from the local cache — primarily the signed-in user's own
    /// profile (other users resolve only if their rows were pulled). Mirrors /api/content/profile.
    fn get_profile(&self, username: String) -> Result<Option<Profile>, DataError> {
        let me = self.current_uid();
        let conn = self.conn();
        vegify_core::get_profile(&conn, &username, me.as_deref()).map_err(Into::into)
    }

    /// Resolve `/<username>/<slug>` against the local cache (offline-first). Mirrors the server's
    /// /api/content/recipe-by-slug.
    fn resolve_recipe_by_slug(
        &self,
        username: String,
        slug: String,
    ) -> Result<Option<RecipeSlugHit>, DataError> {
        let conn = self.conn();
        vegify_core::resolve_recipe_by_slug(&conn, &username, &slug).map_err(Into::into)
    }

    fn resolve_ingredient_by_slug(
        &self,
        slug: String,
    ) -> Result<Option<IngredientSlugHit>, DataError> {
        let conn = self.conn();
        vegify_core::resolve_ingredient_by_slug(&conn, &slug).map_err(Into::into)
    }

    fn recipe_for_edit(&self, id: String) -> Result<Option<RecipeEditData>, DataError> {
        let me = self.current_uid();
        let conn = self.conn();
        vegify_core::recipe_for_edit(&conn, id, me.as_deref()).map_err(Into::into)
    }

    fn list_ingredients(&self, page: Page) -> Result<Vec<IngredientCard>, DataError> {
        let me = self.current_uid();
        let conn = self.conn();
        vegify_core::list_ingredients(&conn, me.as_deref(), &page).map_err(Into::into)
    }

    fn ingredient(&self, id: String) -> Result<Option<IngredientEditData>, DataError> {
        let me = self.current_uid();
        let conn = self.conn();
        vegify_core::ingredient(&conn, id, me.as_deref()).map_err(Into::into)
    }

    fn ingredient_for_edit(&self, id: String) -> Result<Option<IngredientEditData>, DataError> {
        let me = self.current_uid();
        let conn = self.conn();
        vegify_core::ingredient_for_edit(&conn, id, me.as_deref()).map_err(Into::into)
    }

    fn search_ingredients(&self, query: String) -> Result<Vec<IngredientSearchResult>, DataError> {
        let me = self.current_uid();
        let conn = self.conn();
        vegify_core::search_ingredients(&conn, query, me.as_deref()).map_err(Into::into)
    }

    fn search_content(&self, query: String) -> Result<ContentSearchResult, DataError> {
        let me = self.current_uid();
        let conn = self.conn();
        vegify_core::search_content(&conn, query, me.as_deref()).map_err(Into::into)
    }

    fn save_ingredient(&self, mut input: SaveIngredientInput) -> Result<String, DataError> {
        let uid = self.require_uid()?;
        // Mint the client id up front for a create so the local row, the outbox entry, and (after
        // push) the server row all share ONE id — the local-first model (client ULIDs authoritative).
        if input.id.is_none() {
            input.id = Some(new_id());
        }
        let id = self.with_conn(|conn| {
            do_save_ingredient(conn, &input, Some(uid.as_str())).map_err(Into::into)
        })?;
        self.enqueue("saveIngredient", to_json(&input)?)?;
        Ok(id)
    }

    fn delete_ingredient(&self, id: String) -> Result<(), DataError> {
        let uid = self.require_uid()?;
        self.with_conn(|conn| {
            do_delete_ingredient(conn, &id, Some(uid.as_str())).map_err(Into::into)
        })?;
        self.enqueue("deleteIngredient", serde_json::json!({ "id": id }))?;
        Ok(())
    }

    fn restore_ingredient(&self, id: String) -> Result<(), DataError> {
        let uid = self.require_uid()?;
        self.with_conn(|conn| {
            do_restore_ingredient(conn, &id, Some(uid.as_str())).map_err(Into::into)
        })?;
        self.enqueue("restoreIngredient", serde_json::json!({ "id": id }))?;
        Ok(())
    }

    fn save_recipe(&self, mut input: SaveRecipeInput) -> Result<String, DataError> {
        let uid = self.require_uid()?;
        // Mint client ids up front for a create (see save_ingredient). A nested recipe also needs its
        // as-ingredient id stable cross-replica, so mint that alongside — else the push would let the
        // server mint a different one and the consuming item's FK would diverge.
        if input.id.is_none() {
            input.id = Some(new_id());
            input.as_ingredient_id = Some(new_id());
        }
        let id = self.with_conn(|conn| {
            do_save_recipe(conn, &input, Some(uid.as_str())).map_err(Into::into)
        })?;
        self.enqueue("saveRecipe", to_json(&input)?)?;
        Ok(id)
    }

    fn delete_recipe(&self, id: String) -> Result<(), DataError> {
        let uid = self.require_uid()?;
        self.with_conn(|conn| do_delete_recipe(conn, &id, Some(uid.as_str())).map_err(Into::into))?;
        self.enqueue("deleteRecipe", serde_json::json!({ "id": id }))?;
        Ok(())
    }

    fn log_day(&self, date: String) -> Result<DayLog, DataError> {
        let me = self.require_uid()?; // diary is authed-only (private)
        let conn = self.conn();
        vegify_core::log_day(&conn, &me, &date).map_err(Into::into)
    }

    fn log_recents(&self, limit: f64) -> Result<Vec<RecentIngredient>, DataError> {
        let me = self.require_uid()?;
        let conn = self.conn();
        vegify_core::log_recents(&conn, &me, limit as i64).map_err(Into::into)
    }

    fn save_log_entry(&self, mut input: SaveLogEntryInput) -> Result<String, DataError> {
        let uid = self.require_uid()?;
        // Mint the client id up front for a create (see save_recipe) so the local row, the outbox entry,
        // and the server row after push all share ONE id.
        if input.id.is_none() {
            input.id = Some(new_id());
        }
        let id =
            self.with_conn(|conn| do_save_log_entry(conn, &input, &uid).map_err(Into::into))?;
        self.enqueue("saveLogEntry", to_json(&input)?)?;
        Ok(id)
    }

    fn delete_log_entry(&self, id: String) -> Result<(), DataError> {
        let uid = self.require_uid()?;
        self.with_conn(|conn| do_delete_log_entry(conn, &id, &uid).map_err(Into::into))?;
        self.enqueue("deleteLogEntry", serde_json::json!({ "id": id }))?;
        Ok(())
    }

    fn get_nutrition_profile(&self) -> Result<NutritionProfile, DataError> {
        let me = self.require_uid()?; // profile is authed-only (private)
        let conn = self.conn();
        vegify_core::get_nutrition_profile(&conn, &me).map_err(Into::into)
    }

    fn save_nutrition_profile(&self, input: NutritionProfile) -> Result<(), DataError> {
        let uid = self.require_uid()?;
        self.with_conn(|conn| {
            vegify_core::save_nutrition_profile(conn, &uid, &input).map_err(Into::into)
        })?;
        // The upsert is a whole-row replace, so a single queued "saveProfile" always carries the latest
        // state — the server's own upsert is idempotent under a re-push.
        self.enqueue("saveProfile", to_json(&input)?)?;
        Ok(())
    }

    fn save_day_supplements(&self, input: DaySupplementsRecord) -> Result<(), DataError> {
        let uid = self.require_uid()?;
        self.with_conn(|conn| {
            vegify_core::save_day_supplements(
                conn,
                &uid,
                &input.date,
                &DaySupplements {
                    b12: input.b12,
                    vit_d: input.vit_d,
                    algae_oil: input.algae_oil,
                },
            )
            .map_err(Into::into)
        })?;
        // One row per (user, date); the upsert is a whole-row replace, so a queued "saveDaySupplements"
        // always carries the date's latest state and re-pushes idempotently.
        self.enqueue("saveDaySupplements", to_json(&input)?)?;
        Ok(())
    }

    // ---- branded foods (P2.1/P2.2, gate D1 option 2) ----
    //
    // ONLINE-ONLY, and deliberately so. Everything else on this shell reads the local mirror first;
    // branded lookups cannot, because the branded cache and the FoodData Central API key both live on
    // the server — a client that talked to USDA directly would have to ship the key. So a lookup is a
    // proxy call, and offline it fails the way any other network read fails; the shared UI treats an
    // empty branded group as "no branded matches", and the catalog underneath it still answers from
    // the local cache as it always did.
    //
    // The flags on these rows are the SERVER's word-aware match (vegify_core::branded_diet_flags),
    // carried on the wire. The desktop never re-derives them — one matcher, one answer, so a phone and
    // a laptop can't disagree about whether a label mentions dairy.

    fn branded_search(&self, query: String) -> Result<Vec<BrandedFood>, DataError> {
        Ok(client().branded_search(&query)?)
    }

    fn branded_barcode(&self, gtin: String) -> Result<Option<BrandedFood>, DataError> {
        Ok(client().branded_barcode(&gtin)?)
    }

    /// Promote, then PULL before returning. The pull is the whole reason this isn't a one-liner: the
    /// promoted row is created on the SERVER, and the local schema puts a real foreign key on
    /// `log_entries.ingredient_id` / `ingredient_in_recipe.ingredient_id`. Hand the id back before the
    /// mirror has that ingredient and the very next write — the log entry the user just asked for —
    /// fails on a FK violation. So the id this returns is always an id the local cache can already
    /// resolve.
    fn branded_promote(
        &self,
        source: BrandedSource,
        external_id: String,
    ) -> Result<String, DataError> {
        let token = self.require_token()?;
        let id = client().branded_promote(&token, source, &external_id)?;
        self.pull()?;
        Ok(id)
    }

    /// One content-API sync pass: push local writes, THEN pull/reconcile — push-first so the pull's
    /// prune can't drop an unpushed local create. The bootstrap-on-sign-in, the debounced auto-sync,
    /// and the manual Sync button all call it.
    fn sync_now(&self) -> Result<(), DataError> {
        self.push()?;
        self.pull()?;
        // Diary before profile, both after the content pull: diary entries reference content ingredients
        // (so that pull must land first); the profile is independent (only the user FK, always present).
        self.pull_diary()?;
        self.pull_profile()
    }

    fn media_base(&self) -> Result<String, DataError> {
        Ok(vegify_config::desktop::server_url())
    }

    fn current_user(&self) -> Result<Option<AuthUser>, DataError> {
        let user = self.auth_slot().as_ref().map(|s| s.user.clone());
        if let Some(u) = &user {
            // Restored from the keychain on launch — make sure the local row exists before any write.
            self.ensure_user_local(u)?;
        }
        Ok(user)
    }

    fn sign_in(&self, input: SignInInput) -> Result<AuthUser, DataError> {
        let session = client().sign_in(&input.email, &input.password)?;
        let user = session.user.clone();
        self.ensure_user_local(&user)?;
        session_store().store(&session)?;
        *self.auth_slot() = Some(session);
        tracing::info!(user = %user.id, "signed in");
        Ok(user)
    }

    fn sign_up(&self, input: SignUpInput) -> Result<AuthUser, DataError> {
        let session = client().sign_up(&input.name, &input.email, &input.password)?;
        let user = session.user.clone();
        self.ensure_user_local(&user)?;
        session_store().store(&session)?;
        *self.auth_slot() = Some(session);
        tracing::info!(user = %user.id, "signed up");
        Ok(user)
    }

    fn sign_out(&self) -> Result<(), DataError> {
        if let Some(token) = self.current_token() {
            // best-effort server-side revoke; the SDK swallows errors so logout always works locally
            client().logout(&token);
        }
        session_store().clear();
        *self.auth_slot() = None;
        Ok(())
    }

    fn request_password_reset(&self, input: ResetRequestInput) -> Result<(), DataError> {
        // Enumeration-safe; the SDK swallows transport errors too, so the UI shows the same
        // "check your email" result regardless. The reset finishes in the browser via the link.
        client().request_password_reset(&input.email);
        Ok(())
    }

    fn request_email_verification(&self, input: ResetRequestInput) -> Result<(), DataError> {
        // Same contract as request_password_reset. The verify link opens the browser; the desktop
        // never holds the token.
        client().request_email_verification(&input.email);
        Ok(())
    }

    fn message_conversations(&self) -> Result<Vec<ConversationSummary>, DataError> {
        let token = self.require_token()?;
        Ok(client().conversations(&token)?)
    }

    fn message_thread(&self, username: String) -> Result<Thread, DataError> {
        let token = self.require_token()?;
        Ok(client().thread(&token, &username)?)
    }

    fn send_message(&self, input: SendMessageInput) -> Result<Message, DataError> {
        let token = self.require_token()?;
        Ok(client().send_message(&token, &input.to, &input.body)?)
    }

    fn messages_unread(&self) -> Result<f64, DataError> {
        let token = self.require_token()?;
        Ok(client().messages_unread(&token)?)
    }

    fn notifications(&self) -> Result<Vec<DmNotification>, DataError> {
        let token = self.require_token()?;
        Ok(client().notifications(&token)?)
    }

    fn notifications_unread(&self) -> Result<f64, DataError> {
        let token = self.require_token()?;
        Ok(client().notifications_unread(&token)?)
    }

    fn notifications_mark_read(&self) -> Result<(), DataError> {
        let token = self.require_token()?;
        Ok(client().notifications_mark_all_read(&token)?)
    }

    fn report_content(
        &self,
        target_type: String,
        target_id: String,
        reason: String,
        note: String,
    ) -> Result<(), DataError> {
        let token = self.require_token()?;
        Ok(client().report(&token, &target_type, &target_id, &reason, &note)?)
    }

    fn block_user(&self, username: String) -> Result<(), DataError> {
        let token = self.require_token()?;
        client().block_user(&token, &username)?;
        self.pull() // reads change (blocked user's content drops out) — rebuild the local cache
    }

    fn unblock_user(&self, username: String) -> Result<(), DataError> {
        let token = self.require_token()?;
        client().unblock_user(&token, &username)?;
        self.pull() // rebuild with the unblocked user's content back
    }

    fn delete_account(&self, password: String) -> Result<(), DataError> {
        let token = self.require_token()?;
        client().delete_account(&token, &password)?;
        // The account is gone — sign out locally (clear the keychain + cached session).
        session_store().clear();
        *self.auth_slot() = None;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, missing_docs)]
// test code: unwrap/panic ARE the assertion
mod tests;
