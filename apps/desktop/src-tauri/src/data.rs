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

    /// One content-API sync pass: push local writes, THEN pull/reconcile — push-first so the pull's
    /// prune can't drop an unpushed local create. The bootstrap-on-sign-in, the debounced auto-sync,
    /// and the manual Sync button all call it.
    fn sync_now(&self) -> Result<(), DataError> {
        self.push()?;
        self.pull()
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
#[allow(clippy::unwrap_used, clippy::panic, missing_docs)] // test code: unwrap/panic ARE the assertion
mod tests {
    use super::*;
    use std::fs;

    fn recipe_id_by_name(db: &Db, needle: &str) -> String {
        VegifyData::list_recipes(db, Page::default())
            .expect("list")
            .into_iter()
            .find(|c| c.name.contains(needle))
            .unwrap_or_else(|| panic!("no recipe matching {needle:?}"))
            .id
    }

    /// Stamp the in-memory session as `id` (writes get owned by it; reads scope to it).
    fn set_auth(db: &Db, id: &str, name: &str) {
        *db.auth.lock().unwrap() = Some(Session {
            token: "t".into(),
            user: AuthUser {
                id: id.into(),
                name: name.into(),
                username: name.to_lowercase(),
                email: format!("{name}@x"),
                email_verified: false,
            },
        });
    }

    /// Sign in as the seed user (owns all seed content), returning its id.
    fn sign_in_seed(db: &Db) -> String {
        let uid: String = db
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT id FROM users LIMIT 1", [], |r| r.get(0))
            .expect("a seed user exists");
        set_auth(db, &uid, "Seed");
        uid
    }

    #[test]
    fn recipe_nutrition_on_device() {
        let db = Db::open(&crate::db_path()).expect("open");
        let id = recipe_id_by_name(&db, "Complete Shake");
        let r = VegifyData::recipe(&db, id)
            .expect("query ok")
            .expect("recipe exists");
        let cal100 = r.nutrition.calories_per_100g.expect("has calories");
        let grams = r.serving.as_ref().expect("has serving").grams;
        let per_serving = cal100 * grams / 100.0;
        eprintln!(
            "recipe {:?}: {:.1} cal/serving (ULID {})",
            r.name, per_serving, r.id
        );
        assert!((per_serving - 307.5).abs() < 0.5, "got {per_serving:.2}");
    }

    // The exact path the write UI drives: search an ingredient → save a recipe USING it as an item
    // → read it back with items + aggregated nutrition. (Covers do_save_recipe's item INSERTs,
    // which the empty-items sync test does not.)
    #[test]
    fn save_recipe_with_items_via_search_flow() {
        let db_path = std::env::temp_dir().join("vegify-uiflow.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");
        sign_in_seed(&db); // own the saved recipe so the owner-gated edit-load returns it

        let hits = VegifyData::search_ingredients(&db, "Flour".into()).expect("search");
        let flour = hits
            .into_iter()
            .find(|h| h.name.contains("Flour"))
            .expect("a flour exists");

        let id = VegifyData::save_recipe(
            &db,
            SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: None,
                name: "UI Flow Bread".into(),
                subtitle: None,
                directions: Some("mix".into()),
                serving_grams: Some(100.0),
                batch_grams: Some(500.0),
                items: vec![RecipeItemInput {
                    ingredient_id: flour.id.clone(),
                    grams: 500.0,
                    unit: None,
                }],
                slug: None,
            },
        )
        .expect("save recipe with an item");

        let r = VegifyData::recipe(&db, id.clone())
            .expect("read")
            .expect("exists");
        assert_eq!(r.name, "UI Flow Bread");
        assert_eq!(
            r.items.len(),
            1,
            "the searched ingredient is attached as an item"
        );
        assert_eq!(r.items[0].name, flour.name);
        assert!(
            r.nutrition.calories_per_100g.is_some(),
            "flour has calories → recipe aggregates them"
        );
        eprintln!(
            "UI-flow recipe: {} item(s), {:?} cal/100g",
            r.items.len(),
            r.nutrition.calories_per_100g
        );

        // edit-with-defaults: per-item nutrition + servings (batch/serving = 500/100 = 5).
        let edit = VegifyData::recipe_for_edit(&db, id)
            .expect("edit")
            .expect("exists");
        assert_eq!(edit.servings, Some(5.0));
        assert_eq!(edit.items.len(), 1);
        assert_eq!(edit.items[0].calories_per_100g, Some(364.0));
    }

    // Ingredient browser + edit: a saved ingredient is listed (leaf only, not recipe
    // as-ingredients) and its edit defaults (per-100g + own nutrients) round-trip.
    #[test]
    fn ingredient_browser_and_edit_round_trip() {
        let db_path = std::env::temp_dir().join("vegify-ing.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");
        sign_in_seed(&db); // own the saved ingredient so the owner-gated edit-load returns it

        let id = VegifyData::save_ingredient(
            &db,
            SaveIngredientInput {
                id: None,
                visibility: None,
                name: "Test Tofu".into(),
                description: Some("firm".into()),
                price: Some(250),
                calories_per_100g: Some(144.0),
                serving_grams: Some(85.0),
                package_grams: Some(340.0),
                nutrients: vec![IngredientNutrientInput {
                    name: "Protein".into(),
                    amount_per_100g: 17.3,
                    unit: "g".into(),
                }],
                slug: None,
            },
        )
        .expect("save ingredient");

        let cards = VegifyData::list_ingredients(&db, Page::default()).expect("list");
        assert!(
            cards.iter().any(|c| c.name == "Test Tofu"),
            "browser shows the new ingredient"
        );
        assert!(
            !cards.iter().any(|c| c.name.contains("Complete Shake")),
            "browser excludes recipe as-ingredients"
        );

        let e = VegifyData::ingredient_for_edit(&db, id)
            .expect("edit")
            .expect("exists");
        assert_eq!(e.name, "Test Tofu");
        assert_eq!(e.serving_grams, Some(85.0));
        assert_eq!(e.calories_per_100g, Some(144.0));
        assert_eq!(e.nutrients.len(), 1);
        assert_eq!(e.nutrients[0].name, "Protein");
        eprintln!(
            "ingredient edit: {} nutrient(s), serving {:?}g",
            e.nutrients.len(),
            e.serving_grams
        );
    }

    // A signed-in user's writes are stamped with their id. foreign_keys=ON requires that user to
    // exist locally — which ensure_user_local guarantees in the real flow; here we use a seed user.
    #[test]
    fn writes_stamp_signed_in_user() {
        let db_path = std::env::temp_dir().join("vegify-stamp.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");

        let uid = sign_in_seed(&db);

        let rid = VegifyData::save_recipe(
            &db,
            SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: None,
                name: "Owned Recipe".into(),
                subtitle: None,
                directions: None,
                serving_grams: Some(100.0),
                batch_grams: Some(200.0),
                items: vec![],
                slug: None,
            },
        )
        .expect("save");

        let owner: Option<String> = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT i.user_id FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id WHERE r.id = ?1",
                [&rid],
                |r| r.get(0),
            )
            .expect("query owner");
        assert_eq!(
            owner.as_deref(),
            Some(uid.as_str()),
            "recipe is stamped with the signed-in user"
        );
    }

    // Full sign-in path against a running web shell (real network + OS keychain). #[ignore]'d so the
    // default suite needs neither. Run with the web build served on $VEGIFY_AUTH_URL:
    //   VEGIFY_AUTH_URL=http://localhost:39008 \
    //     cargo test --lib sign_in_round_trip -- --ignored --nocapture
    #[test]
    #[ignore]
    fn sign_in_round_trip_against_web() {
        let db_path = std::env::temp_dir().join("vegify-signin.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");
        let _ = VegifyData::sign_out(&db); // start from a clean keychain slot

        let user = VegifyData::sign_in(
            &db,
            SignInInput {
                email: "dev@example.com".into(),
                password: "dev-password".into(),
            },
        )
        .expect("sign in");
        assert_eq!(user.email, "dev@example.com");
        eprintln!("signed in as {} ({})", user.name, user.id);

        // A fresh Db (simulating relaunch) restores the session from the keychain — offline-capable.
        let db2 = Db::open(db_path.to_str().unwrap()).expect("reopen");
        let restored = VegifyData::current_user(&db2)
            .expect("current")
            .expect("restored from keychain");
        assert_eq!(
            restored.id, user.id,
            "session persisted to the keychain across reopen"
        );

        // Wrong password is rejected (the server's message surfaces as DataError::Auth).
        let bad = VegifyData::sign_in(
            &db,
            SignInInput {
                email: "dev@example.com".into(),
                password: "nope".into(),
            },
        );
        assert!(bad.is_err(), "wrong password should be rejected");

        VegifyData::sign_out(&db).expect("sign out");
        assert!(
            VegifyData::current_user(&db).expect("current").is_none(),
            "cleared after sign out"
        );
    }

    // A4 visibility model (mirrors the web's 2-user test): public content is shared, private is
    // owner-only, and edit/delete are owner-gated. Two accounts over one local DB exercise the
    // per-viewer policy directly (the desktop is single-user-local until A3, but the DAL gates are
    // per-viewer regardless). Covers BOTH entities: a recipe IS an ingredient at the data level, but
    // the recipe read procedures have their own SQL gates, so they're asserted separately below.
    #[test]
    fn visibility_scopes_reads_and_guards_writes() {
        let db_path = std::env::temp_dir().join("vegify-vis.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");

        // Two accounts: john (the seed owner) + Bob (upserted locally, as sign-in would guarantee).
        let john = sign_in_seed(&db);
        let bob = new_id();
        db.ensure_user_local(&AuthUser {
            id: bob.clone(),
            name: "Bob".into(),
            username: "bob".into(),
            email: "bob@x".into(),
            email_verified: false,
        })
        .expect("create bob");

        // John creates a PRIVATE and a PUBLIC ingredient.
        let mk = |name: &str, vis: Visibility| {
            VegifyData::save_ingredient(
                &db,
                SaveIngredientInput {
                    id: None,
                    visibility: Some(vis),
                    name: name.into(),
                    description: None,
                    price: None,
                    calories_per_100g: Some(50.0),
                    serving_grams: Some(100.0),
                    package_grams: None,
                    nutrients: vec![],
                    slug: None,
                },
            )
            .expect("john saves")
        };
        let secret = mk("John Secret Sauce", Visibility::Private);
        let public = mk("John Public Sauce", Visibility::Public);

        // John also creates a PRIVATE recipe — the recipe read procs apply their own SQL gates.
        let secret_recipe = VegifyData::save_recipe(
            &db,
            SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: Some(Visibility::Private),
                name: "John Secret Recipe".into(),
                subtitle: None,
                directions: None,
                serving_grams: Some(100.0),
                batch_grams: Some(200.0),
                items: vec![],
                slug: None,
            },
        )
        .expect("john saves recipe");

        // --- As Bob (a different account) ---
        set_auth(&db, &bob, "Bob");

        // Lists + search (isListed): Bob sees John's PUBLIC, not John's private.
        let listed: Vec<String> = VegifyData::list_ingredients(&db, Page::default())
            .expect("bob list")
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert!(
            listed.contains(&"John Public Sauce".to_string()),
            "bob sees john's public"
        );
        assert!(
            !listed.contains(&"John Secret Sauce".to_string()),
            "bob must NOT see john's private"
        );
        let found: Vec<String> = VegifyData::search_ingredients(&db, "John".into())
            .expect("bob search")
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert!(
            found.contains(&"John Public Sauce".to_string()),
            "search returns public"
        );
        assert!(
            !found.contains(&"John Secret Sauce".to_string()),
            "search hides private"
        );

        // Detail (canView): public viewable; private 404s (None). can_edit is false for the non-owner.
        let bob_view = VegifyData::ingredient(&db, public.clone())
            .expect("ok")
            .expect("public viewable");
        assert!(
            !bob_view.can_edit,
            "non-owner sees no edit affordance on public content"
        );
        assert!(
            VegifyData::ingredient(&db, secret.clone())
                .expect("ok")
                .is_none(),
            "private hidden"
        );

        // Edit-load (isOwner): Bob can't load John's public ingredient for editing.
        assert!(
            VegifyData::ingredient_for_edit(&db, public.clone())
                .expect("ok")
                .is_none(),
            "non-owner can't edit-load"
        );

        // Mutations owner-guarded: Bob can neither edit nor delete John's ingredient.
        let hijack = VegifyData::save_ingredient(
            &db,
            SaveIngredientInput {
                id: Some(public.clone()),
                visibility: Some(Visibility::Private),
                name: "Hijacked".into(),
                description: None,
                price: None,
                calories_per_100g: None,
                serving_grams: None,
                package_grams: None,
                nutrients: vec![],
                slug: None,
            },
        );
        assert!(hijack.is_err(), "bob can't edit john's ingredient");
        assert!(
            VegifyData::delete_ingredient(&db, public.clone()).is_err(),
            "bob can't delete john's"
        );

        // Recipe gates (separate SQL from the ingredient path): John's private recipe is hidden,
        // unviewable, un-editable, and un-deletable by Bob.
        let recipe_names: Vec<String> = VegifyData::list_recipes(&db, Page::default())
            .expect("bob recipes")
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert!(
            !recipe_names.contains(&"John Secret Recipe".to_string()),
            "private recipe not listed"
        );
        assert!(
            VegifyData::recipe(&db, secret_recipe.clone())
                .expect("ok")
                .is_none(),
            "private recipe 404s"
        );
        assert!(
            VegifyData::recipe_for_edit(&db, secret_recipe.clone())
                .expect("ok")
                .is_none(),
            "non-owner can't edit-load the recipe"
        );
        assert!(
            VegifyData::delete_recipe(&db, secret_recipe.clone()).is_err(),
            "bob can't delete the recipe"
        );

        // --- Back as John (the owner) ---
        set_auth(&db, &john, "John");
        let own_recipe = VegifyData::recipe(&db, secret_recipe.clone())
            .expect("ok")
            .expect("owner views recipe");
        assert!(own_recipe.can_edit, "owner sees the recipe edit affordance");
        assert!(
            VegifyData::recipe_for_edit(&db, secret_recipe)
                .expect("ok")
                .is_some(),
            "owner can edit-load their recipe"
        );
        let e = VegifyData::ingredient_for_edit(&db, secret.clone())
            .expect("ok")
            .expect("owner edit-load");
        assert_eq!(
            e.visibility,
            Visibility::Private,
            "edit defaults carry the stored visibility"
        );
        assert!(e.can_edit, "owner edit-load is editable");
        // Owner can edit (change visibility) then delete own content.
        VegifyData::save_ingredient(
            &db,
            SaveIngredientInput {
                id: Some(secret),
                visibility: Some(Visibility::Unlisted),
                name: "John Secret Sauce".into(),
                description: None,
                price: None,
                calories_per_100g: Some(50.0),
                serving_grams: Some(100.0),
                package_grams: None,
                nutrients: vec![],
                slug: None,
            },
        )
        .expect("owner edits own");
        let own_ing = VegifyData::ingredient(&db, public.clone())
            .expect("ok")
            .expect("owner views own");
        assert!(
            own_ing.can_edit,
            "owner sees the edit affordance on their own ingredient"
        );
        VegifyData::delete_ingredient(&db, public).expect("owner deletes own");
        eprintln!(
            "visibility: public shared, private hidden from non-owner, edit/delete owner-gated"
        );
    }

    #[test]
    fn writes_require_sign_in() {
        let db_path = std::env::temp_dir().join("vegify-anon-write.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");
        let _ = VegifyData::sign_out(&db); // ensure logged-out regardless of any stored session

        // Reads work anonymously (public content from the local cache)…
        assert!(
            VegifyData::list_recipes(&db, Page::default()).is_ok(),
            "anonymous reads work"
        );
        // …but writes are refused up front, so nothing is stamped NULL-owner or queued to the outbox.
        let r = VegifyData::save_recipe(
            &db,
            SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: Some(Visibility::Public),
                name: "Anon Loaf".into(),
                subtitle: None,
                directions: None,
                serving_grams: None,
                batch_grams: None,
                items: vec![],
                slug: None,
            },
        );
        assert!(
            matches!(r, Err(DataError::Auth(_))),
            "anonymous recipe create is refused"
        );
        let i = VegifyData::save_ingredient(
            &db,
            SaveIngredientInput {
                id: None,
                visibility: Some(Visibility::Public),
                name: "Anon Salt".into(),
                description: None,
                price: None,
                calories_per_100g: None,
                serving_grams: None,
                package_grams: None,
                nutrients: vec![],
                slug: None,
            },
        );
        assert!(
            matches!(i, Err(DataError::Auth(_))),
            "anonymous ingredient create is refused"
        );
        assert!(
            matches!(
                VegifyData::delete_recipe(&db, new_id()),
                Err(DataError::Auth(_))
            ),
            "anonymous delete is refused"
        );
    }

    #[test]
    fn list_recipes_keyset_paginates_newest_first() {
        let db_path = std::env::temp_dir().join("vegify-keyset-page.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");
        let _ = VegifyData::sign_out(&db); // public catalog, logged out

        // The full list (default page) is newest-first: ids (ULIDs) strictly descending.
        let all = VegifyData::list_recipes(&db, Page::default()).expect("all");
        assert!(
            all.len() >= 3,
            "seed has several public recipes (got {})",
            all.len()
        );
        assert!(
            all.windows(2).all(|w| w[0].id > w[1].id),
            "ordered newest-first by id"
        );

        // Page 1 (no cursor) is the newest `limit` of that list.
        let p1 = VegifyData::list_recipes(
            &db,
            Page {
                limit: Some(2),
                ..Default::default()
            },
        )
        .expect("page 1");
        assert_eq!(p1.len(), 2, "limit caps the page");
        assert_eq!(
            p1.iter().map(|c| &c.id).collect::<Vec<_>>(),
            all[..2].iter().map(|c| &c.id).collect::<Vec<_>>(),
            "page 1 = the newest two"
        );

        // Page 2 (cursor = page 1's last id) continues with NO overlap and NO skip.
        let p2 = VegifyData::list_recipes(
            &db,
            Page {
                cursor: Some(p1[1].id.clone()),
                limit: Some(2),
                ..Default::default()
            },
        )
        .expect("page 2");
        assert_eq!(
            p2.first().map(|c| &c.id),
            all.get(2).map(|c| &c.id),
            "page 2 starts right after page 1"
        );
        assert!(
            p2.iter().all(|c| p1.iter().all(|x| x.id != c.id)),
            "pages do not overlap"
        );
    }

    #[test]
    fn list_recipes_honors_each_sort() {
        let db_path = std::env::temp_dir().join("vegify-sort.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");
        let _ = VegifyData::sign_out(&db);

        let list = |sort| {
            VegifyData::list_recipes(
                &db,
                Page {
                    sort,
                    ..Default::default()
                },
            )
            .expect("list")
        };
        let ids = |cards: &[RecipeCard]| cards.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
        let names = |cards: &[RecipeCard]| cards.iter().map(|c| c.name.clone()).collect::<Vec<_>>();

        let (newest, oldest, az, za) = (
            list(Sort::Newest),
            list(Sort::Oldest),
            list(Sort::NameAsc),
            list(Sort::NameDesc),
        );
        assert!(newest.len() >= 3, "seed has several public recipes");

        // Recency: newest is oldest reversed (same id keyset, opposite direction).
        let mut oldest_rev = ids(&oldest);
        oldest_rev.reverse();
        assert_eq!(ids(&newest), oldest_rev, "newest == oldest reversed");

        // Name: A→Z is name-ascending; Z→A is its reverse.
        let mut sorted = names(&az);
        sorted.sort();
        assert_eq!(names(&az), sorted, "A→Z is name-ascending");
        let mut az_rev = names(&az);
        az_rev.reverse();
        assert_eq!(names(&za), az_rev, "Z→A == A→Z reversed");

        // The composite (name, id) keyset for A→Z paginates without overlap or gaps.
        let p1 = VegifyData::list_recipes(
            &db,
            Page {
                sort: Sort::NameAsc,
                limit: Some(2),
                ..Default::default()
            },
        )
        .expect("az page 1");
        assert_eq!(p1.len(), 2);
        let p2 = VegifyData::list_recipes(
            &db,
            Page {
                sort: Sort::NameAsc,
                cursor: Some(p1[1].id.clone()),
                cursor_name: Some(p1[1].name.clone()),
                limit: Some(2),
            },
        )
        .expect("az page 2");
        assert_eq!(
            p2.first().map(|c| &c.id),
            az.get(2).map(|c| &c.id),
            "A→Z page 2 continues after page 1"
        );
        assert!(
            p2.iter().all(|c| p1.iter().all(|x| x.id != c.id)),
            "A→Z pages do not overlap"
        );
    }

    // Upsert-by-id + as-ingredient-id threading (step 1 of the sync engine). A supplied-but-absent id
    // must CREATE the row WITH that id (not silently no-op an UPDATE), and the recipe's as-ingredient
    // id must be honorable so a nested recipe consumed by another (a Biga inside a Dough) keeps a
    // stable cross-replica id — the exact shape the sync pull applies. Re-applying is idempotent.
    #[test]
    fn upsert_by_id_honors_supplied_ids_for_pull() {
        let db_path = std::env::temp_dir().join("vegify-upsert.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");
        sign_in_seed(&db); // own the rows so the owner-gated edit-load returns them

        // --- ingredient: a supplied-but-absent id is created WITH that id, then updated in place ---
        let ing_id = new_id();
        let returned = VegifyData::save_ingredient(
            &db,
            SaveIngredientInput {
                id: Some(ing_id.clone()),
                visibility: None,
                name: "Pulled Ingredient".into(),
                description: None,
                price: None,
                calories_per_100g: Some(42.0),
                serving_grams: Some(100.0),
                package_grams: None,
                nutrients: vec![],
                slug: None,
            },
        )
        .expect("save with a supplied id");
        assert_eq!(returned, ing_id, "the supplied id is honored, not minted");
        let loaded = VegifyData::ingredient_for_edit(&db, ing_id.clone())
            .expect("ok")
            .expect("row was created WITH the supplied id (not a no-op UPDATE)");
        assert_eq!(loaded.name, "Pulled Ingredient");

        // re-apply the SAME id with a changed field → updates in place (idempotent, no duplicate row)
        VegifyData::save_ingredient(
            &db,
            SaveIngredientInput {
                id: Some(ing_id.clone()),
                visibility: None,
                name: "Pulled Ingredient v2".into(),
                description: None,
                price: None,
                calories_per_100g: Some(43.0),
                serving_grams: Some(100.0),
                package_grams: None,
                nutrients: vec![],
                slug: None,
            },
        )
        .expect("re-apply updates");
        let pulled: Vec<String> = VegifyData::list_ingredients(&db, Page::default())
            .expect("list")
            .into_iter()
            .filter(|c| c.name.starts_with("Pulled Ingredient"))
            .map(|c| c.name)
            .collect();
        assert_eq!(
            pulled,
            vec!["Pulled Ingredient v2".to_string()],
            "one row, updated in place"
        );

        // --- recipe + nesting: a Biga (its as-ingredient id supplied) consumed by a Dough as an item ---
        let biga_rid = new_id();
        let biga_aiid = new_id();
        let returned_biga = VegifyData::save_recipe(
            &db,
            SaveRecipeInput {
                id: Some(biga_rid.clone()),
                as_ingredient_id: Some(biga_aiid.clone()),
                visibility: None,
                name: "Pulled Biga".into(),
                subtitle: None,
                directions: None,
                serving_grams: Some(100.0),
                batch_grams: Some(300.0),
                items: vec![],
                slug: None,
            },
        )
        .expect("apply biga with supplied ids");
        assert_eq!(returned_biga, biga_rid, "the supplied recipe id is honored");

        let dough_rid = new_id();
        let dough_aiid = new_id();
        // The Dough item references the BIGA's as-ingredient id — the nested-recipe FK that orphans
        // cross-replica unless the as-ingredient id is threaded and stable. Built fresh per apply so
        // the same pull can be replayed (the input type isn't Clone).
        let build_dough = || SaveRecipeInput {
            id: Some(dough_rid.clone()),
            as_ingredient_id: Some(dough_aiid.clone()),
            visibility: None,
            name: "Pulled Dough".into(),
            subtitle: None,
            directions: None,
            serving_grams: Some(250.0),
            batch_grams: Some(500.0),
            items: vec![RecipeItemInput {
                ingredient_id: biga_aiid.clone(),
                grams: 300.0,
                unit: None,
            }],
            slug: None,
        };
        let returned_dough =
            VegifyData::save_recipe(&db, build_dough()).expect("apply dough consuming the biga");
        assert_eq!(
            returned_dough, dough_rid,
            "the supplied recipe id is honored"
        );

        let view = VegifyData::recipe(&db, dough_rid.clone())
            .expect("read")
            .expect("exists");
        assert_eq!(
            view.items.len(),
            1,
            "the nested biga item resolved (FK intact via threaded as-ing id)"
        );
        assert_eq!(
            view.items[0].name, "Pulled Biga",
            "the item resolves to the biga's as-ingredient"
        );

        // re-apply the SAME dough (an idempotent pull) → still ONE dough with that id, ONE item
        VegifyData::save_recipe(&db, build_dough()).expect("re-apply dough");
        let doughs: Vec<String> = VegifyData::list_recipes(&db, Page::default())
            .expect("list")
            .into_iter()
            .filter(|c| c.name == "Pulled Dough")
            .map(|c| c.id)
            .collect();
        assert_eq!(
            doughs,
            vec![dough_rid.clone()],
            "idempotent: one dough with the supplied id"
        );
        let again = VegifyData::recipe(&db, dough_rid.clone())
            .expect("read")
            .expect("exists");
        assert_eq!(again.items.len(), 1, "re-apply did not duplicate the item");
        eprintln!(
            "upsert-by-id: supplied ids honored; nested biga/dough FK stable across re-apply"
        );
    }

    // Step 3: every local content write records a semantic mutation in the _outbox push queue, FIFO,
    // with the resolved client id (a create's minted id, captured up front). A recipe's payload also
    // carries the as-ingredient id matching the LOCAL row, so a later push creates the server row with
    // the same id (cross-replica stability). The server stamps userId from the session, so it's absent.
    #[test]
    fn writes_record_semantic_outbox() {
        let db_path = std::env::temp_dir().join("vegify-outbox.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");
        sign_in_seed(&db);

        let outbox = |db: &Db| -> Vec<(String, serde_json::Value)> {
            let conn = db.conn.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT op, payload FROM _outbox ORDER BY seq")
                .unwrap();
            let v = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .unwrap()
                .map(|row| {
                    let (op, p) = row.unwrap();
                    (op, serde_json::from_str(&p).unwrap())
                })
                .collect::<Vec<_>>();
            v
        };

        let rid = VegifyData::save_recipe(
            &db,
            SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: None,
                name: "Outbox Recipe".into(),
                subtitle: None,
                directions: None,
                serving_grams: Some(100.0),
                batch_grams: Some(200.0),
                items: vec![],
                slug: None,
            },
        )
        .expect("save recipe");
        // capture the local as-ingredient id BEFORE the delete removes the row
        let local_ai: String = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT as_ingredient_id FROM recipes WHERE id = ?1",
                [&rid],
                |r| r.get(0),
            )
            .expect("local recipe row");

        let iid = VegifyData::save_ingredient(
            &db,
            SaveIngredientInput {
                id: None,
                visibility: Some(Visibility::Private),
                name: "Outbox Ingredient".into(),
                description: None,
                price: None,
                calories_per_100g: Some(10.0),
                serving_grams: Some(50.0),
                package_grams: None,
                nutrients: vec![],
                slug: None,
            },
        )
        .expect("save ingredient");
        VegifyData::delete_recipe(&db, rid.clone()).expect("delete recipe");

        let rows = outbox(&db);
        let ops: Vec<&str> = rows.iter().map(|(op, _)| op.as_str()).collect();
        assert_eq!(
            ops,
            ["saveRecipe", "saveIngredient", "deleteRecipe"],
            "one FIFO entry per write"
        );

        // saveRecipe payload = the content-API body: resolved recipe id, camelCase fields, NO userId.
        let recipe_payload = &rows[0].1;
        assert_eq!(
            recipe_payload["id"],
            serde_json::json!(rid),
            "carries the resolved recipe id"
        );
        assert_eq!(recipe_payload["name"], "Outbox Recipe");
        assert!(
            recipe_payload.get("userId").is_none(),
            "userId omitted — the server stamps it"
        );
        assert_eq!(
            recipe_payload["asIngredientId"],
            serde_json::json!(local_ai),
            "outbox as-ingredient id == the local row's (so a push keeps it stable cross-replica)"
        );

        // saveIngredient payload carries its resolved id + the chosen visibility (serialized lowercase).
        let ing_payload = &rows[1].1;
        assert_eq!(ing_payload["id"], serde_json::json!(iid));
        assert_eq!(ing_payload["visibility"], "private");

        // deleteRecipe payload = { id } (drives DELETE /api/content/recipes?id=…).
        assert_eq!(rows[2].1["id"], serde_json::json!(rid));
        eprintln!("outbox FIFO {ops:?}; recipe payload's as-ingredient id matches the local row");
    }

    // Step 4: the content-API HTTP client against a running web shell (real network + Bearer auth).
    // #[ignore]'d so the default suite needs no server. Run with the web build served on $VEGIFY_AUTH_URL:
    //   VEGIFY_AUTH_URL=http://localhost:39008 \
    //     cargo test --lib content_client_round_trip -- --ignored --nocapture
    #[test]
    #[ignore]
    fn content_client_round_trip_against_web() {
        let db_path = std::env::temp_dir().join("vegify-client.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");
        let _ = VegifyData::sign_out(&db); // clean slot
        VegifyData::sign_in(
            &db,
            SignInInput {
                email: "dev@example.com".into(),
                password: "dev-password".into(),
            },
        )
        .expect("sign in");
        let token = db.current_token().expect("token after sign in");

        // pull: the seed world comes back in mutation shape (recipes carry their as-ingredient id).
        let p = client().content_pull(Some(&token)).expect("pull");
        eprintln!(
            "pull: {} recipes, {} ingredients",
            p.recipes.len(),
            p.ingredients.len()
        );
        assert!(
            !p.recipes.is_empty() && !p.ingredients.is_empty(),
            "seed content pulled"
        );
        assert!(
            p.recipes.iter().all(|r| !r.asIngredientId.is_empty()),
            "recipes carry as-ingredient id"
        );

        // post a recipe via the client → it appears in a fresh pull.
        let rid = new_id();
        let body = serde_json::json!({
            "id": rid, "asIngredientId": new_id(), "name": "Client Posted Loaf", "visibility": "public",
            "servingGrams": 100.0, "batchGrams": 200.0, "items": []
        });
        client()
            .content_post(&token, "recipes", &body)
            .expect("post recipe");
        let p2 = client().content_pull(Some(&token)).expect("pull2");
        assert!(
            p2.recipes
                .iter()
                .any(|r| r.id == rid && r.name == "Client Posted Loaf"),
            "posted recipe appears in the pull"
        );

        // delete via the client → gone from the next pull.
        client()
            .content_delete(&token, "recipes", &rid)
            .expect("delete recipe");
        let p3 = client().content_pull(Some(&token)).expect("pull3");
        assert!(
            !p3.recipes.iter().any(|r| r.id == rid),
            "deleted recipe is gone"
        );
        VegifyData::sign_out(&db).ok();
        eprintln!("content client round-trip OK: pull → post → pull → delete → pull");
    }

    // Step 5/9: the full sync engine across TWO replicas of one account, against a running web shell.
    // A writes a nested Biga/Dough and syncs (push); B syncs (pull) and must converge to A's content
    // with the nested item FK intact — the end-to-end proof the whole arc was built for. #[ignore]'d
    // (needs the bun web build on $VEGIFY_AUTH_URL):
    //   VEGIFY_AUTH_URL=http://localhost:39008 \
    //     cargo test --lib two_replica_round_trip -- --ignored --nocapture
    #[test]
    #[ignore]
    fn two_replica_round_trip_against_web() {
        let tmp = std::env::temp_dir();
        let mk = |tag: &str| {
            let db = tmp.join(format!("vegify-2rep-{tag}.db"));
            let _ = fs::remove_file(&db);
            fs::copy(crate::db_path(), &db).expect("seed");
            Db::open(db.to_str().unwrap()).expect("open")
        };
        let a = mk("A");
        let b = mk("B");
        for dev in [&a, &b] {
            let _ = VegifyData::sign_out(dev);
            VegifyData::sign_in(
                dev,
                SignInInput {
                    email: "dev@example.com".into(),
                    password: "dev-password".into(),
                },
            )
            .expect("sign in");
        }

        // A creates a nested pair: a Biga, and a Dough that consumes the Biga's as-ingredient as an
        // item. Names carry a unique run tag so repeated runs against one served copy don't collide.
        let uniq = &new_id()[..10];
        let biga_name = format!("2rep Biga {uniq}");
        let dough_name = format!("2rep Dough {uniq}");
        let biga_rid = VegifyData::save_recipe(
            &a,
            SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: Some(Visibility::Public),
                name: biga_name.clone(),
                subtitle: None,
                directions: None,
                serving_grams: Some(100.0),
                batch_grams: Some(300.0),
                items: vec![],
                slug: None,
            },
        )
        .expect("A saves biga");
        let biga_ai: String = a
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT as_ingredient_id FROM recipes WHERE id = ?1",
                [&biga_rid],
                |r| r.get(0),
            )
            .expect("biga as-ingredient");
        VegifyData::save_recipe(
            &a,
            SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: Some(Visibility::Public),
                name: dough_name.clone(),
                subtitle: None,
                directions: None,
                serving_grams: Some(250.0),
                batch_grams: Some(500.0),
                items: vec![RecipeItemInput {
                    ingredient_id: biga_ai.clone(),
                    grams: 300.0,
                    unit: None,
                }],
                slug: None,
            },
        )
        .expect("A saves dough consuming the biga");

        // A pushes to the server; B pulls and reconciles.
        a.sync_now().expect("A sync (push + pull)");
        b.sync_now().expect("B sync (push-empty + pull)");

        let names: Vec<String> = VegifyData::list_recipes(&b, Page::default())
            .expect("B list")
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert!(names.contains(&biga_name), "B converged on A's Biga");
        assert!(names.contains(&dough_name), "B converged on A's Dough");

        // the nested FK survived the cross-replica round-trip: B's Dough item resolves to A's Biga.
        let dough = VegifyData::list_recipes(&b, Page::default())
            .expect("list")
            .into_iter()
            .find(|c| c.name == dough_name)
            .expect("dough on B");
        let view = VegifyData::recipe(&b, dough.id)
            .expect("B recipe")
            .expect("exists");
        assert_eq!(
            view.items.len(),
            1,
            "B's Dough kept its item through push→pull"
        );
        assert_eq!(
            view.items[0].name, biga_name,
            "the item resolves to A's Biga as-ingredient — nested FK stable cross-replica"
        );
        eprintln!(
            "two-replica round-trip OK: A wrote nested Biga/Dough → pushed → B pulled + converged"
        );
    }

    // Regression for the email-collision the GUI sign-in surfaced (P5 identity reconciliation): a
    // separately-seeded cache already holds this email under a DIFFERENT id (the dev seed's john vs
    // the server's john). ensure_user_local must adopt the server id + re-point the stale user's
    // content, NOT trip `UNIQUE users.email`.
    #[test]
    fn signin_reconciles_email_collision() {
        let db_path = std::env::temp_dir().join("vegify-reconcile.db");
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap()).expect("open");

        let (seed_id, email): (String, String) = db
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT id, email FROM users LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .expect("a seed user");
        let owned_before: i64 = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM ingredients WHERE user_id = ?1",
                [&seed_id],
                |r| r.get(0),
            )
            .expect("count");
        assert!(owned_before > 0, "the seed user owns some content");

        // sign in as the SAME email under a different (server) id — must reconcile, not collide.
        let server_id = new_id();
        db.ensure_user_local(&AuthUser {
            id: server_id.clone(),
            name: "John".into(),
            username: "john".into(),
            email: email.clone(),
            email_verified: false,
        })
        .expect("reconcile, not UNIQUE users.email");

        let conn = db.conn.lock().unwrap();
        let id_for_email: String = conn
            .query_row("SELECT id FROM users WHERE email = ?1", [&email], |r| {
                r.get(0)
            })
            .expect("one user");
        assert_eq!(id_for_email, server_id, "the cache adopted the server id");
        let stale_rows: i64 = conn
            .query_row(
                "SELECT count(*) FROM users WHERE id = ?1",
                [&seed_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stale_rows, 0, "the stale seed user row is gone");
        let owned_after: i64 = conn
            .query_row(
                "SELECT count(*) FROM ingredients WHERE user_id = ?1",
                [&server_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            owned_after, owned_before,
            "the seed user's content now belongs to the server id"
        );
        eprintln!(
            "reconcile OK: email collision resolved, content re-pointed {seed_id} → {server_id}"
        );
    }

    /// Drift guard: Drizzle (`.data/vegify.db` via `pnpm db:push`) is the content-schema source of
    /// truth. Assert the embedded `schema.sql` is a faithful SUPERSET — every (table, column) the dev
    /// DB has, a fresh DB built from `schema.sql` also has — so vegify-core's mutations (proven against
    /// the dev DB by the tests above) hold on a shipped fresh DB too. Fails if the Drizzle schema
    /// changes without `schema.sql` being regenerated.
    #[test]
    fn schema_sql_matches_drizzle_dev_db() {
        use std::collections::BTreeSet;
        fn table_columns(conn: &Connection) -> BTreeSet<String> {
            let names: Vec<String> = conn
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type='table' \
                     AND name NOT LIKE 'sqlite_%' AND name <> '_outbox'",
                )
                .unwrap()
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            let mut cols = BTreeSet::new();
            for t in names {
                let mut q = conn
                    .prepare(&format!("SELECT name FROM pragma_table_info('{t}')"))
                    .unwrap();
                for c in q.query_map([], |r| r.get::<_, String>(0)).unwrap() {
                    cols.insert(format!("{t}.{}", c.unwrap()));
                }
            }
            cols
        }

        // Truth = the Drizzle dev DB. Copy to a temp file so the test never mutates the shared one.
        let truth_path =
            std::env::temp_dir().join(format!("vegify-schema-truth-{}.db", std::process::id()));
        let _ = fs::remove_file(&truth_path);
        fs::copy(crate::db_path(), &truth_path).expect("dev DB present — run `pnpm db:push`");
        let truth_cols = table_columns(&Connection::open(&truth_path).unwrap());

        // Candidate = a fresh in-memory DB built ONLY from the embedded schema.sql.
        let fresh = Connection::open_in_memory().unwrap();
        ensure_content_schema(&fresh).unwrap();
        let fresh_cols = table_columns(&fresh);

        let missing: Vec<_> = truth_cols.difference(&fresh_cols).collect();
        assert!(
            missing.is_empty(),
            "schema.sql drifted from the Drizzle dev DB — regenerate it; missing: {missing:?}"
        );
        let _ = fs::remove_file(&truth_path);
        eprintln!("schema.sql covers all {} dev-DB columns", truth_cols.len());
    }

    /// Reproduces + fixes the v0.2.x "can't be opened / no such table" crash: a SHIPPED fresh install
    /// opens a brand-new app-data DB that never saw `pnpm db:push`. `Db::open` must create the content
    /// schema so the first pull's per-row `do_save_ingredient` (exactly what `apply_pull` replays)
    /// succeeds instead of erroring "no such table: ingredients".
    #[test]
    fn fresh_db_accepts_content_writes_without_dev_seed() {
        let path = std::env::temp_dir().join(format!("vegify-fresh-{}.db", std::process::id()));
        let _ = fs::remove_file(&path);
        let db = Db::open(path.to_str().unwrap()).expect("open fresh app-data DB");
        let conn = db.conn.lock().unwrap();
        let input = SaveIngredientInput {
            id: None,
            visibility: Some(Visibility::Public),
            name: "Rolled Oats".into(),
            description: None,
            price: None,
            calories_per_100g: Some(389.0),
            serving_grams: Some(40.0),
            package_grams: Some(1000.0),
            nutrients: vec![IngredientNutrientInput {
                name: "Protein".into(),
                amount_per_100g: 16.9,
                unit: "g".into(),
            }],
            slug: None,
        };
        let id =
            do_save_ingredient(&conn, &input, None).expect("save into the freshly-created schema");
        let name: String = conn
            .query_row("SELECT name FROM ingredients WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(name, "Rolled Oats");
        // The on-demand catalog/amount rows landed too (find_or_create_nutrient / upsert_amount).
        let nutrients: i64 = conn
            .query_row("SELECT count(*) FROM nutrients", [], |r| r.get(0))
            .unwrap();
        let amounts: i64 = conn
            .query_row("SELECT count(*) FROM amounts", [], |r| r.get(0))
            .unwrap();
        assert!(
            nutrients >= 1 && amounts >= 1,
            "nutrient + amount rows created on demand"
        );
        drop(conn);
        let _ = fs::remove_file(&path);
        eprintln!("fresh DB accepted a content write: ingredient {id}");
    }

    /// The logged-out cache regression behind "@user" breadcrumbs + "No one goes by that handle":
    /// the pull now carries creators, apply_pull mirrors them into `users` (placeholder email = the
    /// id), so creator handles and profiles resolve on-device without a session — while an
    /// auth-owned row (real email) keeps its email and stale placeholders vanish on the next pull.
    #[test]
    fn apply_pull_users_resolve_creators_and_profiles() {
        let path = std::env::temp_dir().join(format!("vegify-pullusers-{}.db", std::process::id()));
        let _ = fs::remove_file(&path);
        let db = Db::open(path.to_str().unwrap()).expect("open fresh app-data DB");
        let mut conn = db.conn.lock().unwrap();
        // An auth-owned row, as ensure_user_local writes it on sign-in (real email).
        conn.execute(
            "INSERT INTO users(id, name, username, email) VALUES ('u-self', 'Old Name', 'self', 'self@example.com')",
            [],
        )
        .unwrap();

        let payload = vegify_client::PullPayload {
            recipes: vec![vegify_client::PullRecipe {
                id: "r1".into(),
                asIngredientId: "r1-as".into(),
                userId: Some("u-ada".into()),
                visibility: vegify_client::Visibility::public,
                name: "Ada's Porridge".into(),
                subtitle: None,
                directions: None,
                servingGrams: None,
                batchGrams: None,
                items: vec![],
                slug: None,
            }],
            ingredients: vec![vegify_client::PullIngredient {
                id: "i1".into(),
                userId: Some("u-ada".into()),
                visibility: vegify_client::Visibility::public,
                name: "Ada's Oats".into(),
                description: None,
                price: None,
                caloriesPer100g: Some(389.0),
                servingGrams: None,
                packageGrams: None,
                nutrients: vec![],
                slug: None,
                deletedAt: None,
            }],
            users: vec![
                vegify_client::PullUser {
                    id: "u-ada".into(),
                    username: "ada".into(),
                    name: "Ada".into(),
                    avatarKey: Some("media/ada.jpg".into()),
                },
                // The signed-in user also appears in a pull (they own content) — public fields
                // refresh, the auth-owned email must survive.
                vegify_client::PullUser {
                    id: "u-self".into(),
                    username: "self".into(),
                    name: "New Name".into(),
                    avatarKey: None,
                },
            ],
        };
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        apply_pull(&mut conn, &payload).expect("apply pull with users");

        // Creator handle resolves on the recipe view (the "@user" breadcrumb regression).
        let view = vegify_core::recipe(&conn, "r1".into(), None)
            .unwrap()
            .expect("pulled recipe readable");
        assert_eq!(view.creator.as_deref(), Some("ada"));

        // The creator's profile resolves logged-out, with both shelves + avatar.
        let profile = vegify_core::get_profile(&conn, "ada", None)
            .unwrap()
            .expect("creator profile resolves from the cache");
        assert_eq!(profile.name, "Ada");
        assert_eq!(profile.avatar_key.as_deref(), Some("media/ada.jpg"));
        assert_eq!(profile.recipes.len(), 1);
        assert_eq!(profile.ingredients.len(), 1);

        // Placeholder rows carry the synthetic id-email; auth-owned rows keep theirs (public
        // fields refreshed).
        let ada_email: String = conn
            .query_row("SELECT email FROM users WHERE id = 'u-ada'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(ada_email, "u-ada");
        let (self_email, self_name): (String, String) = conn
            .query_row(
                "SELECT email, name FROM users WHERE id = 'u-self'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(self_email, "self@example.com");
        assert_eq!(self_name, "New Name");

        // A later pull that no longer lists ada prunes the placeholder; the auth-owned row stays.
        let empty = vegify_client::PullPayload {
            recipes: vec![],
            ingredients: vec![],
            users: vec![],
        };
        apply_pull(&mut conn, &empty).expect("apply empty pull");
        let ada_left: i64 = conn
            .query_row("SELECT COUNT(*) FROM users WHERE id = 'u-ada'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(ada_left, 0, "stale placeholder pruned");
        let self_left: i64 = conn
            .query_row("SELECT COUNT(*) FROM users WHERE id = 'u-self'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(self_left, 1, "auth-owned row survives");

        drop(conn);
        let _ = fs::remove_file(&path);
    }

    /// Slug generation: kebab from name, per-scope uniqueness (global for leaf ingredients, per-user
    /// for recipes), rename regenerates + logs the old slug to slug_history, and a pull-provided slug
    /// is stored verbatim (server-authoritative).
    #[test]
    fn slugs_generate_dedup_and_record_rename_history() {
        let path = std::env::temp_dir().join(format!("vegify-slug-{}.db", std::process::id()));
        let _ = fs::remove_file(&path);
        let db = Db::open(path.to_str().unwrap()).expect("open");
        let conn = db.conn.lock().unwrap();

        let ing = |name: &str, slug: Option<&str>| SaveIngredientInput {
            id: None,
            visibility: Some(Visibility::Public),
            name: name.into(),
            description: None,
            price: None,
            calories_per_100g: None,
            serving_grams: None,
            package_grams: None,
            nutrients: vec![],
            slug: slug.map(str::to_string),
        };
        let slug_of = |conn: &Connection, id: &str| -> String {
            conn.query_row("SELECT slug FROM ingredients WHERE id=?1", [id], |r| {
                r.get::<_, Option<String>>(0)
            })
            .unwrap()
            .unwrap_or_default()
        };

        // Leaf ingredients: kebab + global dedup.
        let a = do_save_ingredient(&conn, &ing("Rolled Oats", None), None).unwrap();
        let b = do_save_ingredient(&conn, &ing("Rolled Oats", None), None).unwrap();
        assert_eq!(slug_of(&conn, &a), "rolled-oats");
        assert_eq!(
            slug_of(&conn, &b),
            "rolled-oats-2",
            "global collision gets a numeric suffix"
        );

        // A pull-provided slug is stored verbatim (no generation).
        let c = do_save_ingredient(&conn, &ing("Rolled Oats", Some("server-picked-slug")), None)
            .unwrap();
        assert_eq!(slug_of(&conn, &c), "server-picked-slug");

        // Recipes: slug unique PER USER (same name under different users may coincide). Real user
        // rows first — ingredients.user_id has an FK and foreign_keys is ON.
        conn.execute(
            "INSERT INTO users(id, name, email, password_hash, username) VALUES
             ('user-1','U1','u1@example.com','h','u1'), ('user-2','U2','u2@example.com','h','u2')",
            [],
        )
        .unwrap();
        let mk_recipe = |name: &str| SaveRecipeInput {
            id: None,
            as_ingredient_id: None,
            visibility: Some(Visibility::Public),
            name: name.into(),
            subtitle: None,
            directions: None,
            serving_grams: None,
            batch_grams: None,
            items: vec![],
            slug: None,
        };
        let r_u1a = do_save_recipe(&conn, &mk_recipe("Biga"), Some("user-1")).unwrap();
        let r_u1b = do_save_recipe(&conn, &mk_recipe("Biga"), Some("user-1")).unwrap();
        let r_u2 = do_save_recipe(&conn, &mk_recipe("Biga"), Some("user-2")).unwrap();
        let recipe_slug = |rid: &str| -> String {
            let as_ing: String = conn
                .query_row(
                    "SELECT as_ingredient_id FROM recipes WHERE id=?1",
                    [rid],
                    |r| r.get(0),
                )
                .unwrap();
            slug_of(&conn, &as_ing)
        };
        assert_eq!(recipe_slug(&r_u1a), "biga");
        assert_eq!(
            recipe_slug(&r_u1b),
            "biga-2",
            "same user, same name → suffix"
        );
        assert_eq!(
            recipe_slug(&r_u2),
            "biga",
            "different user reuses the slug (per-user scope)"
        );

        // Rename a recipe → new slug, old one logged to slug_history for a 301.
        let as_ing_u1a: String = conn
            .query_row(
                "SELECT as_ingredient_id FROM recipes WHERE id=?1",
                [&r_u1a],
                |r| r.get(0),
            )
            .unwrap();
        do_save_recipe(
            &conn,
            &SaveRecipeInput {
                id: Some(r_u1a.clone()),
                as_ingredient_id: Some(as_ing_u1a),
                name: "Poolish".into(),
                ..mk_recipe("Poolish")
            },
            Some("user-1"),
        )
        .unwrap();
        assert_eq!(
            recipe_slug(&r_u1a),
            "poolish",
            "rename regenerates the slug"
        );
        let history: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM slug_history WHERE scope='user-1' AND slug='biga'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(history, 1, "the old slug is recorded for redirects");

        drop(conn);
        let _ = fs::remove_file(&path);
    }

    /// The sitemap is crawler-facing: it must list ONLY public, slugged, indexable URLs — never a
    /// private/unlisted row, and never a recipe whose owner has no handle (there'd be no canonical URL).
    #[test]
    fn public_sitemap_excludes_private_and_headless() {
        let path = std::env::temp_dir().join(format!("vegify-sm-{}.db", std::process::id()));
        let _ = fs::remove_file(&path);
        let db = Db::open(path.to_str().unwrap()).expect("open");
        let conn = db.conn.lock().unwrap();

        // A handled user + a headless one (NULL username → no canonical recipe URL).
        conn.execute(
            "INSERT INTO users(id, name, email, password_hash, username) VALUES
             ('u1','U1','u1@example.com','h','chef'), ('u0','U0','u0@example.com','h',NULL)",
            [],
        )
        .unwrap();

        let ing = |name: &str, vis: Visibility| SaveIngredientInput {
            id: None,
            visibility: Some(vis),
            name: name.into(),
            description: None,
            price: None,
            calories_per_100g: None,
            serving_grams: None,
            package_grams: None,
            nutrients: vec![],
            slug: None,
        };
        do_save_ingredient(&conn, &ing("Tofu", Visibility::Public), None).unwrap();
        do_save_ingredient(&conn, &ing("Secret Sauce", Visibility::Private), None).unwrap();
        // An OWNED leaf — canonical under its creator (/<username>/ingredients/<slug>).
        do_save_ingredient(&conn, &ing("Chef Paste", Visibility::Public), Some("u1")).unwrap();

        let rec = |name: &str, vis: Visibility| SaveRecipeInput {
            id: None,
            as_ingredient_id: None,
            visibility: Some(vis),
            name: name.into(),
            subtitle: None,
            directions: None,
            serving_grams: None,
            batch_grams: None,
            items: vec![],
            slug: None,
        };
        do_save_recipe(&conn, &rec("Public Stew", Visibility::Public), Some("u1")).unwrap();
        do_save_recipe(&conn, &rec("Private Stew", Visibility::Private), Some("u1")).unwrap();
        do_save_recipe(&conn, &rec("Headless Stew", Visibility::Public), Some("u0")).unwrap();

        let sm = vegify_core::public_sitemap(&conn).unwrap();
        let ings: Vec<&str> = sm.ingredients.iter().map(|i| i.slug.as_str()).collect();
        assert!(ings.contains(&"tofu"), "public leaf ingredient listed");
        assert!(
            !ings.contains(&"secret-sauce"),
            "private ingredient excluded"
        );
        assert!(
            sm.ingredients
                .iter()
                .any(|i| i.slug == "tofu" && i.username.is_none()),
            "unowned leaf = the catalog namespace (/ingredients/<slug>)"
        );
        assert!(
            sm.ingredients
                .iter()
                .any(|i| i.slug == "chef-paste" && i.username.as_deref() == Some("chef")),
            "owned leaf carries its owner handle (canonical /<username>/ingredients/<slug>)"
        );

        let recs: Vec<&str> = sm.recipes.iter().map(|r| r.slug.as_str()).collect();
        assert!(recs.contains(&"public-stew"), "public recipe listed");
        assert!(!recs.contains(&"private-stew"), "private recipe excluded");
        assert!(
            !recs.contains(&"headless-stew"),
            "recipe under a handle-less user excluded"
        );
        assert!(
            sm.recipes
                .iter()
                .any(|r| r.username == "chef" && r.slug == "public-stew"),
            "listed recipe carries its owner handle"
        );

        drop(conn);
        let _ = fs::remove_file(&path);
    }
}
