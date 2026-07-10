//! vegify-client-rs — the vegify HTTP API's client SDK: a blocking ureq transport for the auth,
//! content-sync, messages, and notifications routes, plus an OS-keychain [`SessionStore`].
//!
//! Extracted from the desktop app (step 2 of the SDK plan) — the desktop consumes THIS crate, never
//! the other way around (applications are leaves). The wire types below are the hand mirrors moved
//! verbatim (names preserved so the desktop's generated IPC bindings are byte-identical); step 3
//! swaps this module to types GENERATED from `vegify-api-types`' TypeCollection via specta-rust,
//! which is what makes drift between this client and the server impossible by construction.
//!
//! Transport is deliberately blocking: the desktop's sync engine runs sequential calls on its own
//! thread — no async runtime in the SDK. The `specta` feature adds `Type` derives to the wire types
//! so a ttipc consumer can register them; plain consumers get no specta in their tree.

use serde::{Deserialize, Serialize};

/// The SDK error. `Auth` = credential/identity failures (sign-in rejections, missing session);
/// `Api` = the server's own JSON `error` message on a non-2xx; `Network` = transport failures
/// (already formatted "Network error: …" — consumers surface these strings as-is).
#[derive(Debug)]
pub enum Error {
    /// Authentication failure (bad credentials, expired session).
    Auth(String),
    /// Server-reported error (a 4xx/5xx `{error}` body), stringified.
    Api(String),
    /// Transport failure before any server answer.
    Network(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Auth(m) | Error::Api(m) | Error::Network(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Deserialize)]
struct ApiErrorBody {
    error: String,
}

/// A ureq transport failure → [`Error::Network`]. The agent is built with
/// `http_status_as_error(false)`, so a non-2xx status is NOT an `Err` here — it comes back as
/// `Ok(response)` and is handled by [`read_json`]/[`expect_ok`]. So a ureq error is always a genuine
/// transport/network problem.
fn net(e: ureq::Error) -> Error {
    Error::Network(format!("Network error: {e}"))
}

/// The server's JSON `{error}` message from a non-2xx response, or a generic fallback.
fn status_error(resp: &mut ureq::http::Response<ureq::Body>) -> String {
    resp.body_mut()
        .read_json::<ApiErrorBody>()
        .map(|b| b.error)
        .unwrap_or_else(|_| "Request failed.".to_string())
}

/// Read a typed body from a 2xx response; a non-2xx becomes [`Error::Api`] carrying the server's
/// JSON `error` message (or a generic fallback if the body isn't the expected shape).
fn read_json<T: serde::de::DeserializeOwned>(
    mut resp: ureq::http::Response<ureq::Body>,
) -> Result<T, Error> {
    if resp.status().is_success() {
        resp.body_mut()
            .read_json::<T>()
            .map_err(|e| Error::Api(e.to_string()))
    } else {
        Err(Error::Api(status_error(&mut resp)))
    }
}

/// Assert a 2xx (body discarded), else [`Error::Api`] carrying the server's message. For the
/// fire-and-forget writes that don't read a response body but must still surface a rejection.
fn expect_ok(mut resp: ureq::http::Response<ureq::Body>) -> Result<(), Error> {
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(Error::Api(status_error(&mut resp)))
    }
}

// ---- wire types ----------------------------------------------------------------------------------
// Moved verbatim from the desktop's hand mirrors (names/fields preserved → its IPC bindings are
// byte-identical). Timestamps/counts are f64 so specta emits `number`, not bigint.

/// The signed-in user, as the auth routes return it.
#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    /// User id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Public handle backing `/<username>`.
    pub username: String,
    /// Login/notification address (the account's own view).
    pub email: String,
    /// Whether the account's email is verified (serialized `emailVerified`; the auth response's
    /// snake_case `email_verified` is mapped in [`VegifyClient::sign_in`]/[`sign_up`]).
    pub email_verified: bool,
}

/// A session as the auth routes mint it (and as [`SessionStore`] persists it): the opaque Bearer
/// token + the user profile (cached so `current_user` works offline).
#[derive(Serialize, Deserialize, Clone)]
pub struct Session {
    /// Bearer token for subsequent calls.
    pub token: String,
    /// The signed-in user.
    pub user: AuthUser,
}

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
/// The other party of a DM, as lists and headers show them.
pub struct DmParty {
    /// User id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Public handle (`/<username>`).
    pub username: String,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
/// One conversation row of the DM list.
pub struct DmConversation {
    /// Conversation id.
    pub id: String,
    /// The other party.
    pub with: DmParty,
    /// Body of the newest message (the preview line).
    pub last_body: String,
    /// Newest message timestamp, ms epoch (f64 on this wire).
    pub last_at: f64,
    /// True when the newest message is the viewer's own.
    pub last_is_mine: bool,
    /// Count of unread messages (f64 on this wire).
    pub unread: f64,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
/// One DM as the thread renders it.
pub struct DmMessage {
    /// Message id.
    pub id: String,
    /// Message body (plain text).
    pub body: String,
    /// Send timestamp, ms epoch (f64 on this wire).
    pub created_at: f64,
    /// True when the viewer sent it.
    pub mine: bool,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
/// A DM thread: the other party plus the messages, oldest first.
pub struct DmThread {
    /// The other party (resolved even for an empty thread).
    pub with: DmParty,
    /// The messages, oldest first.
    pub messages: Vec<DmMessage>,
}

/// One bell row. `payload` is the server's per-kind JSON as a RAW STRING (specta has no stable JSON
/// type on this line) — consumers parse it themselves.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DmNotification {
    /// Notification id.
    pub id: String,
    /// Kind tag (e.g. "ingredient-updated"); selects the payload shape.
    pub kind: String,
    /// Raw per-kind JSON payload; the desktop parses it by `kind`.
    pub payload: String,
    /// Creation timestamp, ms epoch (f64 on this wire).
    pub created_at: f64,
    /// Whether the viewer has opened it.
    pub read: bool,
}

// The /api/content/pull payload, in mutation shape + each row's owner. The desktop's sync engine
// maps these onto vegify-core's Save*Input for the local re-apply.
#[derive(Deserialize)]
/// One full sync pull — every server row visible to this device's user.
pub struct PullPayload {
    /// All visible recipes, with their lines.
    pub recipes: Vec<PullRecipe>,
    /// All visible ingredients, with their nutrient rows.
    pub ingredients: Vec<PullIngredient>,
    /// The creators of the rows above (public identity only) — mirrored into the local `users`
    /// cache so creator handles and profiles resolve on-device. Absent on pre-users servers.
    #[serde(default)]
    pub users: Vec<PullUser>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
/// One creator as pulled — the public identity surface for content in this payload.
pub struct PullUser {
    /// User id (what PullRecipe/PullIngredient `user_id` points at).
    pub id: String,
    /// Profile handle (the `/<username>` URL segment).
    pub username: String,
    /// Display name.
    pub name: String,
    /// Media key of the profile avatar; clients compose `<api base>/<key>`.
    #[serde(default)]
    pub avatar_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
/// One recipe row as pulled (mirrored verbatim into the local DB).
pub struct PullRecipe {
    /// Recipe id (stable cross-replica).
    pub id: String,
    /// The recipe's as-ingredient pair id; must survive the mirror so
    /// consuming items' FKs stay intact.
    pub as_ingredient_id: String,
    /// Owner id; None = ownerless seed content.
    pub user_id: Option<String>,
    /// Visibility as stored.
    pub visibility: vegify_core::Visibility,
    /// Recipe title.
    pub name: String,
    /// Optional subtitle.
    pub subtitle: Option<String>,
    /// Free-text directions.
    pub directions: Option<String>,
    /// Serving size in grams, when declared.
    pub serving_grams: Option<f64>,
    /// Total batch mass in grams, when declared.
    pub batch_grams: Option<f64>,
    /// The recipe's lines, in order.
    pub items: Vec<PullItem>,
    #[serde(default)]
    /// Current slug, mirrored verbatim so local links match the server.
    pub slug: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
/// One recipe line as pulled.
pub struct PullItem {
    /// The ingredient the line references.
    pub ingredient_id: String,
    /// Line quantity in grams (canonical).
    pub grams: f64,
    /// Display unit the author picked; None = grams.
    pub unit: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
/// One ingredient row as pulled (mirrored verbatim into the local DB).
pub struct PullIngredient {
    /// Ingredient id (stable cross-replica).
    pub id: String,
    /// Owner id; None = the communal catalog.
    pub user_id: Option<String>,
    /// Visibility as stored.
    pub visibility: vegify_core::Visibility,
    /// Ingredient name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Cents (USD) — i32 end-to-end (the write path's width; see vegify-api-types).
    pub price: Option<i32>,
    /// Calories per 100 g, when known.
    pub calories_per_100g: Option<f64>,
    /// Serving size in grams, when declared.
    pub serving_grams: Option<f64>,
    /// Package mass in grams, when declared.
    pub package_grams: Option<f64>,
    /// Per-100 g nutrient rows.
    pub nutrients: Vec<PullReading>,
    #[serde(default)]
    /// Current slug, mirrored verbatim so local links match the server.
    pub slug: Option<String>,
    /// Soft-delete tombstone (ms) — mirrored verbatim so local filtering matches the server.
    #[serde(default)]
    pub deleted_at: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
/// One nutrient reading as pulled, per 100 g.
pub struct PullReading {
    /// Nutrient name.
    pub name: String,
    /// Quantity per 100 g.
    pub amount_per_100g: f64,
    /// Unit for the quantity (g, mg, µg).
    pub unit: String,
}

// ---- session store (OS keychain) -----------------------------------------------------------------

/// keyring-core installs no default store, so the right one is chosen here, once, before the
/// first Entry. Test and debug builds get the in-memory mock — dev/test stay hermetic and
/// prompt-free (a debug binary is re-signed ad hoc on every build, so a macOS "Always Allow"
/// grant never sticks; the mock doesn't persist across a process restart). Release builds get
/// the platform store: the macOS legacy keychain (same item class + service/account attributes
/// keyring 3 wrote, so existing sessions survive the upgrade — probe-verified) or the iOS
/// protected-data store. On other targets no store is installed and Entry::new reports it,
/// which only surfaces as the signed-out state.
static KEYCHAIN_STORE_INIT: std::sync::Once = std::sync::Once::new();

fn init_keychain_store() {
    KEYCHAIN_STORE_INIT.call_once(|| {
        #[cfg(any(test, debug_assertions))]
        {
            let store = keyring_core::mock::Store::new().expect("mock keychain store");
            keyring_core::set_default_store(store);
        }
        #[cfg(all(not(any(test, debug_assertions)), target_os = "macos"))]
        {
            if let Ok(store) = apple_native_keyring_store::keychain::Store::new() {
                keyring_core::set_default_store(store);
            }
        }
        #[cfg(all(not(any(test, debug_assertions)), target_os = "ios"))]
        {
            if let Ok(store) = apple_native_keyring_store::protected::Store::new() {
                keyring_core::set_default_store(store);
            }
        }
    });
}

/// The OS-keychain session store: one keyring entry (service + account supplied by the consumer —
/// the app owns its identity) holding the JSON-serialized [`Session`]. The Entry is created once
/// per store and reused (cheap, and it keeps the access pattern identical across the mock and
/// the real stores).
pub struct SessionStore {
    service: String,
    account: String,
    entry: std::sync::OnceLock<keyring_core::Entry>,
}

impl SessionStore {
    /// A store addressing one keychain entry (`service` + `account` are the
    /// consumer's identity — the app owns its naming).
    pub fn new(service: impl Into<String>, account: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            account: account.into(),
            entry: std::sync::OnceLock::new(),
        }
    }

    fn entry(&self) -> Result<&keyring_core::Entry, Error> {
        init_keychain_store();
        if self.entry.get().is_none() {
            let e = keyring_core::Entry::new(&self.service, &self.account)
                .map_err(|e| Error::Auth(e.to_string()))?;
            let _ = self.entry.set(e);
        }
        Ok(self.entry.get().expect("just set"))
    }

    /// The stored session, if one round-trips from the keychain and parses.
    pub fn load(&self) -> Option<Session> {
        let json = self.entry().ok()?.get_password().ok()?;
        serde_json::from_str(&json).ok()
    }

    /// Persist `s` as the keychain entry's value (JSON).
    pub fn store(&self, s: &Session) -> Result<(), Error> {
        let json = serde_json::to_string(s).map_err(|e| Error::Auth(e.to_string()))?;
        self.entry()?
            .set_password(&json)
            .map_err(|e| Error::Auth(e.to_string()))
    }

    /// Delete the stored session; missing entries are fine.
    pub fn clear(&self) {
        if let Ok(e) = self.entry() {
            let _ = e.delete_credential();
        }
    }
}

// ---- the client -----------------------------------------------------------------------------------

/// The vegify API client: a base URL + blocking calls mirroring the server's routes 1:1. Stateless —
/// construct freely; the session token is passed per call (the consumer owns storage, usually via
/// [`SessionStore`]).
pub struct VegifyClient {
    base: String,
    agent: ureq::Agent,
}

impl VegifyClient {
    /// Client for the vegify API at `base_url` (trailing slash tolerated).
    pub fn new(base_url: impl Into<String>) -> Self {
        let base = base_url.into().trim_end_matches('/').to_string();
        // http_status_as_error(false): the server returns JSON `{error}` bodies on 4xx/5xx that the
        // SDK surfaces to the user; ureq 3 otherwise collapses a non-2xx into a bare StatusCode error
        // with no body. So take every response as-is and inspect the status ourselves (see read_json /
        // expect_ok). Default agent config otherwise.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        Self { base, agent }
    }

    /// The `/ws` WebSocket URL derived from the base (https→wss, http→ws) — the realtime change feed.
    pub fn ws_url(&self) -> String {
        let ws = if let Some(rest) = self.base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = self.base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            self.base.clone()
        };
        format!("{}/ws", ws.trim_end_matches('/'))
    }

    /// Generic over the builder's body-state (`WithBody`/`WithoutBody`) so it decorates both GETs and
    /// POSTs. `.header()` lives on `impl<Any> RequestBuilder<Any>`, so one signature covers both.
    fn bearer<B>(req: ureq::RequestBuilder<B>, token: &str) -> ureq::RequestBuilder<B> {
        req.header("authorization", &format!("Bearer {token}"))
    }

    // ---- auth ----

    /// POST credentials to a JSON auth route; on success returns the [`Session`] to store. The
    /// backend serializes the user snake_case (`email_verified`) while [`AuthUser`] is camelCase, so
    /// the response parses into a wire-shaped struct and maps over. ALL failures surface as
    /// [`Error::Auth`] (credential rejections carry the server's message).
    fn post_auth(&self, path: &str, body: serde_json::Value) -> Result<Session, Error> {
        #[derive(Deserialize)]
        struct WireUser {
            id: String,
            name: String,
            #[serde(default)]
            username: String,
            email: String,
            #[serde(default)]
            email_verified: bool,
        }
        #[derive(Deserialize)]
        struct WireSession {
            token: String,
            user: WireUser,
        }
        let url = format!("{}/api/auth/{path}", self.base);
        // Auth surfaces ALL failures as Error::Auth (not Api/Network): a transport failure, a
        // credential rejection (the server's message), and a malformed body all read as "couldn't sign
        // you in" to the caller.
        let mut resp = self
            .agent
            .post(&url)
            .send_json(body)
            .map_err(|e| Error::Auth(format!("Network error: {e}")))?;
        if resp.status().is_success() {
            resp.body_mut()
                .read_json::<WireSession>()
                .map(|w| Session {
                    token: w.token,
                    user: AuthUser {
                        id: w.user.id,
                        name: w.user.name,
                        username: w.user.username,
                        email: w.user.email,
                        email_verified: w.user.email_verified,
                    },
                })
                .map_err(|e| Error::Auth(e.to_string()))
        } else {
            Err(Error::Auth(status_error(&mut resp)))
        }
    }

    /// Sign in with email + password, returning the new session.
    pub fn sign_in(&self, email: &str, password: &str) -> Result<Session, Error> {
        self.post_auth(
            "login",
            serde_json::json!({ "email": email, "password": password }),
        )
    }

    /// Create an account and sign in, returning the new session.
    pub fn sign_up(&self, name: &str, email: &str, password: &str) -> Result<Session, Error> {
        self.post_auth(
            "signup",
            serde_json::json!({ "name": name, "email": email, "password": password }),
        )
    }

    /// Best-effort server-side session revoke — errors are swallowed so a local logout always works.
    pub fn logout(&self, token: &str) {
        let _ = Self::bearer(
            self.agent.post(format!("{}/api/auth/logout", self.base)),
            token,
        )
        .send_empty();
    }

    /// Enumeration-safe: the backend always 200s; transport errors are swallowed too, so the caller
    /// shows the same "check your email" result regardless. The reset finishes in the browser.
    pub fn request_password_reset(&self, email: &str) {
        let url = format!("{}/api/auth/password-reset/request", self.base);
        let _ = self
            .agent
            .post(&url)
            .send_json(serde_json::json!({ "email": email }));
    }

    /// Same contract as [`request_password_reset`](Self::request_password_reset).
    pub fn request_email_verification(&self, email: &str) {
        let url = format!("{}/api/auth/email-verification/request", self.base);
        let _ = self
            .agent
            .post(&url)
            .send_json(serde_json::json!({ "email": email }));
    }

    // ---- content sync ----

    fn content_url(&self, path: &str) -> String {
        format!("{}/api/content/{path}", self.base)
    }

    /// GET /api/content/pull → the viewer's listed world in mutation shape. With a token: public +
    /// own. Without one (anonymous): public only.
    pub fn content_pull(&self, token: Option<&str>) -> Result<PullPayload, Error> {
        tracing::debug!("GET /api/content/pull");
        let mut req = self.agent.get(self.content_url("pull"));
        if let Some(token) = token {
            req = Self::bearer(req, token);
        }
        let payload: PullPayload = read_json(req.call().map_err(net)?)?;
        tracing::debug!(
            recipes = payload.recipes.len(),
            ingredients = payload.ingredients.len(),
            "pull received"
        );
        Ok(payload)
    }

    /// POST /api/content/{collection} with a save payload (the server upserts by id and stamps the
    /// owner from the session). `collection` ∈ {"recipes", "ingredients"}.
    pub fn content_post(
        &self,
        token: &str,
        collection: &str,
        body: &serde_json::Value,
    ) -> Result<(), Error> {
        tracing::debug!(collection, "POST content");
        expect_ok(
            Self::bearer(self.agent.post(self.content_url(collection)), token)
                .send_json(body)
                .map_err(net)?,
        )
    }

    /// DELETE /api/content/{collection}?id=… — idempotent server-side.
    pub fn content_delete(&self, token: &str, collection: &str, id: &str) -> Result<(), Error> {
        tracing::debug!(collection, id, "DELETE content");
        expect_ok(
            Self::bearer(
                self.agent
                    .delete(format!("{}?id={id}", self.content_url(collection))),
                token,
            )
            .call()
            .map_err(net)?,
        )
    }

    /// Undo a soft delete (POST /api/content/ingredient-restore?id=).
    pub fn restore_ingredient(&self, token: &str, id: &str) -> Result<(), Error> {
        tracing::debug!(id, "POST ingredient-restore");
        expect_ok(
            Self::bearer(
                self.agent.post(format!(
                    "{}?id={id}",
                    self.content_url("ingredient-restore")
                )),
                token,
            )
            .send_json(serde_json::json!({}))
            .map_err(net)?,
        )
    }

    // ---- messages (1:1 DMs; auth required, viewer-relative shapes) ----

    fn messages_url(&self, path: &str) -> String {
        format!("{}/api/messages/{path}", self.base)
    }

    /// The viewer's conversation list, newest-message first.
    pub fn conversations(&self, token: &str) -> Result<Vec<DmConversation>, Error> {
        read_json(
            Self::bearer(self.agent.get(self.messages_url("conversations")), token)
                .call()
                .map_err(net)?,
        )
    }

    /// The thread with `with` (a username), oldest message first.
    pub fn thread(&self, token: &str, with: &str) -> Result<DmThread, Error> {
        read_json(
            Self::bearer(
                self.agent
                    .get(self.messages_url("thread"))
                    .query("with", with),
                token,
            )
            .call()
            .map_err(net)?,
        )
    }

    /// Send `body` to `to` (a username); returns the created message.
    pub fn send_message(&self, token: &str, to: &str, body: &str) -> Result<DmMessage, Error> {
        read_json(
            Self::bearer(self.agent.post(self.messages_url("send")), token)
                .send_json(serde_json::json!({ "to": to, "body": body }))
                .map_err(net)?,
        )
    }

    /// Count of unread DMs (f64 mirrors the wire).
    pub fn messages_unread(&self, token: &str) -> Result<f64, Error> {
        #[derive(Deserialize)]
        struct Count {
            count: f64,
        }
        let c: Count = read_json(
            Self::bearer(self.agent.get(self.messages_url("unread")), token)
                .call()
                .map_err(net)?,
        )?;
        Ok(c.count)
    }

    // ---- notifications (the bell) ----

    fn notifications_url(&self, path: &str) -> String {
        format!("{}/api/notifications{path}", self.base)
    }

    /// List the bell. The server sends `payload` as parsed JSON; it's re-serialized to a string for
    /// [`DmNotification`] (consumers parse per kind).
    pub fn notifications(&self, token: &str) -> Result<Vec<DmNotification>, Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct WireNotification {
            id: String,
            kind: String,
            payload: serde_json::Value,
            created_at: f64,
            read: bool,
        }
        let rows: Vec<WireNotification> = read_json(
            Self::bearer(self.agent.get(self.notifications_url("")), token)
                .call()
                .map_err(net)?,
        )?;
        Ok(rows
            .into_iter()
            .map(|n| DmNotification {
                id: n.id,
                kind: n.kind,
                payload: n.payload.to_string(),
                created_at: n.created_at,
                read: n.read,
            })
            .collect())
    }

    /// Count of unread notifications (f64 mirrors the wire).
    pub fn notifications_unread(&self, token: &str) -> Result<f64, Error> {
        #[derive(Deserialize)]
        struct Count {
            count: f64,
        }
        let c: Count = read_json(
            Self::bearer(self.agent.get(self.notifications_url("/unread")), token)
                .call()
                .map_err(net)?,
        )?;
        Ok(c.count)
    }

    /// Mark every notification read.
    pub fn notifications_mark_all_read(&self, token: &str) -> Result<(), Error> {
        expect_ok(
            Self::bearer(self.agent.post(self.notifications_url("/read")), token)
                .send_json(serde_json::json!({}))
                .map_err(net)?,
        )
    }

    // ---- UGC safety (App Review 1.2) + account deletion (5.1.1(v)) ----

    /// Report content or a user. `target_type` ∈ {ingredient, recipe, user, message}.
    pub fn report(
        &self,
        token: &str,
        target_type: &str,
        target_id: &str,
        reason: &str,
        note: &str,
    ) -> Result<(), Error> {
        expect_ok(
            Self::bearer(self.agent.post(self.content_url("report")), token)
                .send_json(serde_json::json!({
                    "targetType": target_type, "targetId": target_id, "reason": reason,
                    "note": if note.is_empty() { serde_json::Value::Null } else { serde_json::json!(note) },
                }))
                .map_err(net)?,
        )
    }

    /// Block `username`: hides their content and stops their DMs.
    pub fn block_user(&self, token: &str, username: &str) -> Result<(), Error> {
        expect_ok(
            Self::bearer(
                self.agent.post(format!("{}/api/users/block", self.base)),
                token,
            )
            .send_json(serde_json::json!({ "username": username }))
            .map_err(net)?,
        )
    }

    /// Unblock `username`.
    pub fn unblock_user(&self, token: &str, username: &str) -> Result<(), Error> {
        expect_ok(
            Self::bearer(
                self.agent.post(format!("{}/api/users/unblock", self.base)),
                token,
            )
            .send_json(serde_json::json!({ "username": username }))
            .map_err(net)?,
        )
    }

    /// Delete the signed-in account (password-reconfirmed). Irreversible.
    pub fn delete_account(&self, token: &str, password: &str) -> Result<(), Error> {
        expect_ok(
            Self::bearer(
                self.agent
                    .post(format!("{}/api/auth/delete-account", self.base)),
                token,
            )
            .send_json(serde_json::json!({ "password": password }))
            .map_err(net)?,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod tests {
    use super::*;

    #[test]
    fn ws_url_derives_scheme_and_path() {
        assert_eq!(
            VegifyClient::new("https://api.vegify.app/").ws_url(),
            "wss://api.vegify.app/ws"
        );
        assert_eq!(
            VegifyClient::new("http://localhost:8787").ws_url(),
            "ws://localhost:8787/ws"
        );
    }

    #[test]
    fn session_store_round_trips_on_the_mock_keychain() {
        let store = SessionStore::new("app.vegify.sdk-test", "session");
        assert!(store.load().is_none(), "mock starts empty");
        let s = Session {
            token: "tok".into(),
            user: AuthUser {
                id: "u1".into(),
                name: "Ada".into(),
                username: "ada".into(),
                email: "a@x".into(),
                email_verified: true,
            },
        };
        store.store(&s).unwrap();
        let back = store.load().expect("stored session loads");
        assert_eq!(back.token, "tok");
        assert_eq!(back.user.username, "ada");
        store.clear();
        assert!(store.load().is_none(), "clear removes it");
    }
}
