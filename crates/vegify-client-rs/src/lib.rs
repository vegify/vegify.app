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
    Auth(String),
    Api(String),
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

/// Map a ureq error → [`Error`], surfacing the server's JSON `error` message on non-2xx.
fn err(e: ureq::Error) -> Error {
    match e {
        ureq::Error::Status(_, resp) => {
            let msg = resp
                .into_json::<ApiErrorBody>()
                .map(|b| b.error)
                .unwrap_or_else(|_| "Request failed.".to_string());
            Error::Api(msg)
        }
        e => Error::Network(format!("Network error: {e}")),
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
    pub id: String,
    pub name: String,
    /// Public handle backing `/<username>`.
    pub username: String,
    pub email: String,
    /// Whether the account's email is verified (serialized `emailVerified`; the auth response's
    /// snake_case `email_verified` is mapped in [`VegifyClient::sign_in`]/[`sign_up`]).
    pub email_verified: bool,
}

/// A session as the auth routes mint it (and as [`SessionStore`] persists it): the opaque Bearer
/// token + the user profile (cached so `current_user` works offline).
#[derive(Serialize, Deserialize, Clone)]
pub struct Session {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DmParty {
    pub id: String,
    pub name: String,
    pub username: String,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DmConversation {
    pub id: String,
    pub with: DmParty,
    pub last_body: String,
    pub last_at: f64,
    pub last_is_mine: bool,
    pub unread: f64,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DmMessage {
    pub id: String,
    pub body: String,
    pub created_at: f64,
    pub mine: bool,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DmThread {
    pub with: DmParty,
    pub messages: Vec<DmMessage>,
}

/// One bell row. `payload` is the server's per-kind JSON as a RAW STRING (specta has no stable JSON
/// type on this line) — consumers parse it themselves.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DmNotification {
    pub id: String,
    pub kind: String,
    pub payload: String,
    pub created_at: f64,
    pub read: bool,
}

// The /api/content/pull payload, in mutation shape + each row's owner. The desktop's sync engine
// maps these onto vegify-core's Save*Input for the local re-apply.
#[derive(Deserialize)]
pub struct PullPayload {
    pub recipes: Vec<PullRecipe>,
    pub ingredients: Vec<PullIngredient>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRecipe {
    pub id: String,
    pub as_ingredient_id: String,
    pub user_id: Option<String>,
    pub visibility: vegify_core::Visibility,
    pub name: String,
    pub subtitle: Option<String>,
    pub directions: Option<String>,
    pub serving_grams: Option<f64>,
    pub batch_grams: Option<f64>,
    pub items: Vec<PullItem>,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullItem {
    pub ingredient_id: String,
    pub grams: f64,
    pub unit: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullIngredient {
    pub id: String,
    pub user_id: Option<String>,
    pub visibility: vegify_core::Visibility,
    pub name: String,
    pub description: Option<String>,
    /// Cents (USD) — i32 end-to-end (the write path's width; see vegify-api-types).
    pub price: Option<i32>,
    pub calories_per_100g: Option<f64>,
    pub serving_grams: Option<f64>,
    pub package_grams: Option<f64>,
    pub nutrients: Vec<PullReading>,
    #[serde(default)]
    pub slug: Option<String>,
    /// Soft-delete tombstone (ms) — mirrored verbatim so local filtering matches the server.
    #[serde(default)]
    pub deleted_at: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullReading {
    pub name: String,
    pub amount_per_100g: f64,
    pub unit: String,
}

// ---- session store (OS keychain) -----------------------------------------------------------------

/// In test and debug builds, route ALL keychain access to keyring's in-memory mock store instead of
/// the real OS keychain — dev/test stay hermetic and prompt-free (a debug binary is re-signed ad hoc
/// on every build, so a macOS "Always Allow" grant never sticks). Installed once, before the first
/// Entry. RELEASE builds use the real keychain. Dev tradeoff: the mock doesn't persist across a
/// process restart.
#[cfg(any(test, debug_assertions))]
static MOCK_KEYCHAIN_INIT: std::sync::Once = std::sync::Once::new();

/// The OS-keychain session store: one keyring entry (service + account supplied by the consumer —
/// the app owns its identity) holding the JSON-serialized [`Session`]. The Entry is created once
/// per store and reused — keyring's MOCK store (test/debug) keeps state per Entry INSTANCE, so a
/// fresh Entry per call would make the mock amnesiac even within one process. The real keychain
/// (release) persists regardless.
pub struct SessionStore {
    service: String,
    account: String,
    entry: std::sync::OnceLock<keyring::Entry>,
}

impl SessionStore {
    pub fn new(service: impl Into<String>, account: impl Into<String>) -> Self {
        Self { service: service.into(), account: account.into(), entry: std::sync::OnceLock::new() }
    }

    fn entry(&self) -> Result<&keyring::Entry, Error> {
        #[cfg(any(test, debug_assertions))]
        MOCK_KEYCHAIN_INIT.call_once(|| {
            keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        });
        if self.entry.get().is_none() {
            let e = keyring::Entry::new(&self.service, &self.account)
                .map_err(|e| Error::Auth(e.to_string()))?;
            let _ = self.entry.set(e);
        }
        Ok(self.entry.get().expect("just set"))
    }

    pub fn load(&self) -> Option<Session> {
        let json = self.entry().ok()?.get_password().ok()?;
        serde_json::from_str(&json).ok()
    }

    pub fn store(&self, s: &Session) -> Result<(), Error> {
        let json = serde_json::to_string(s).map_err(|e| Error::Auth(e.to_string()))?;
        self.entry()?.set_password(&json).map_err(|e| Error::Auth(e.to_string()))
    }

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
}

impl VegifyClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let base = base_url.into().trim_end_matches('/').to_string();
        Self { base }
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

    fn bearer(req: ureq::Request, token: &str) -> ureq::Request {
        req.set("authorization", &format!("Bearer {token}"))
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
        match ureq::post(&url).send_json(body) {
            Ok(resp) => resp
                .into_json::<WireSession>()
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
                .map_err(|e| Error::Auth(e.to_string())),
            Err(ureq::Error::Status(_, resp)) => {
                let msg = resp
                    .into_json::<ApiErrorBody>()
                    .map(|e| e.error)
                    .unwrap_or_else(|_| "Authentication failed.".to_string());
                Err(Error::Auth(msg))
            }
            Err(e) => Err(Error::Auth(format!("Network error: {e}"))),
        }
    }

    pub fn sign_in(&self, email: &str, password: &str) -> Result<Session, Error> {
        self.post_auth("login", serde_json::json!({ "email": email, "password": password }))
    }

    pub fn sign_up(&self, name: &str, email: &str, password: &str) -> Result<Session, Error> {
        self.post_auth(
            "signup",
            serde_json::json!({ "name": name, "email": email, "password": password }),
        )
    }

    /// Best-effort server-side session revoke — errors are swallowed so a local logout always works.
    pub fn logout(&self, token: &str) {
        let _ = Self::bearer(ureq::post(&format!("{}/api/auth/logout", self.base)), token).call();
    }

    /// Enumeration-safe: the backend always 200s; transport errors are swallowed too, so the caller
    /// shows the same "check your email" result regardless. The reset finishes in the browser.
    pub fn request_password_reset(&self, email: &str) {
        let url = format!("{}/api/auth/password-reset/request", self.base);
        let _ = ureq::post(&url).send_json(serde_json::json!({ "email": email }));
    }

    /// Same contract as [`request_password_reset`](Self::request_password_reset).
    pub fn request_email_verification(&self, email: &str) {
        let url = format!("{}/api/auth/email-verification/request", self.base);
        let _ = ureq::post(&url).send_json(serde_json::json!({ "email": email }));
    }

    // ---- content sync ----

    fn content_url(&self, path: &str) -> String {
        format!("{}/api/content/{path}", self.base)
    }

    /// GET /api/content/pull → the viewer's listed world in mutation shape. With a token: public +
    /// own. Without one (anonymous): public only.
    pub fn content_pull(&self, token: Option<&str>) -> Result<PullPayload, Error> {
        tracing::debug!("GET /api/content/pull");
        let mut req = ureq::get(&self.content_url("pull"));
        if let Some(token) = token {
            req = Self::bearer(req, token);
        }
        let payload = req
            .call()
            .map_err(err)?
            .into_json::<PullPayload>()
            .map_err(|e| Error::Api(e.to_string()))?;
        tracing::debug!(
            recipes = payload.recipes.len(),
            ingredients = payload.ingredients.len(),
            "pull received"
        );
        Ok(payload)
    }

    /// POST /api/content/{collection} with a save payload (the server upserts by id and stamps the
    /// owner from the session). `collection` ∈ {"recipes", "ingredients"}.
    pub fn content_post(&self, token: &str, collection: &str, body: &serde_json::Value) -> Result<(), Error> {
        tracing::debug!(collection, "POST content");
        Self::bearer(ureq::post(&self.content_url(collection)), token)
            .send_json(body)
            .map_err(err)?;
        Ok(())
    }

    /// DELETE /api/content/{collection}?id=… — idempotent server-side.
    pub fn content_delete(&self, token: &str, collection: &str, id: &str) -> Result<(), Error> {
        tracing::debug!(collection, id, "DELETE content");
        Self::bearer(ureq::delete(&format!("{}?id={id}", self.content_url(collection))), token)
            .call()
            .map_err(err)?;
        Ok(())
    }

    /// Undo a soft delete (POST /api/content/ingredient-restore?id=).
    pub fn restore_ingredient(&self, token: &str, id: &str) -> Result<(), Error> {
        tracing::debug!(id, "POST ingredient-restore");
        Self::bearer(ureq::post(&format!("{}?id={id}", self.content_url("ingredient-restore"))), token)
            .send_json(serde_json::json!({}))
            .map_err(err)?;
        Ok(())
    }

    // ---- messages (1:1 DMs; auth required, viewer-relative shapes) ----

    fn messages_url(&self, path: &str) -> String {
        format!("{}/api/messages/{path}", self.base)
    }

    pub fn conversations(&self, token: &str) -> Result<Vec<DmConversation>, Error> {
        Self::bearer(ureq::get(&self.messages_url("conversations")), token)
            .call()
            .map_err(err)?
            .into_json()
            .map_err(|e| Error::Api(e.to_string()))
    }

    pub fn thread(&self, token: &str, with: &str) -> Result<DmThread, Error> {
        Self::bearer(ureq::get(&self.messages_url("thread")).query("with", with), token)
            .call()
            .map_err(err)?
            .into_json()
            .map_err(|e| Error::Api(e.to_string()))
    }

    pub fn send_message(&self, token: &str, to: &str, body: &str) -> Result<DmMessage, Error> {
        Self::bearer(ureq::post(&self.messages_url("send")), token)
            .send_json(serde_json::json!({ "to": to, "body": body }))
            .map_err(err)?
            .into_json()
            .map_err(|e| Error::Api(e.to_string()))
    }

    pub fn messages_unread(&self, token: &str) -> Result<f64, Error> {
        #[derive(Deserialize)]
        struct Count {
            count: f64,
        }
        let c: Count = Self::bearer(ureq::get(&self.messages_url("unread")), token)
            .call()
            .map_err(err)?
            .into_json()
            .map_err(|e| Error::Api(e.to_string()))?;
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
        let rows: Vec<WireNotification> = Self::bearer(ureq::get(&self.notifications_url("")), token)
            .call()
            .map_err(err)?
            .into_json()
            .map_err(|e| Error::Api(e.to_string()))?;
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

    pub fn notifications_unread(&self, token: &str) -> Result<f64, Error> {
        #[derive(Deserialize)]
        struct Count {
            count: f64,
        }
        let c: Count = Self::bearer(ureq::get(&self.notifications_url("/unread")), token)
            .call()
            .map_err(err)?
            .into_json()
            .map_err(|e| Error::Api(e.to_string()))?;
        Ok(c.count)
    }

    pub fn notifications_mark_all_read(&self, token: &str) -> Result<(), Error> {
        Self::bearer(ureq::post(&self.notifications_url("/read")), token)
            .send_json(serde_json::json!({}))
            .map_err(err)?;
        Ok(())
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
        Self::bearer(ureq::post(&self.content_url("report")), token)
            .send_json(serde_json::json!({
                "targetType": target_type, "targetId": target_id, "reason": reason,
                "note": if note.is_empty() { serde_json::Value::Null } else { serde_json::json!(note) },
            }))
            .map_err(err)?;
        Ok(())
    }

    pub fn block_user(&self, token: &str, username: &str) -> Result<(), Error> {
        Self::bearer(ureq::post(&format!("{}/api/users/block", self.base)), token)
            .send_json(serde_json::json!({ "username": username }))
            .map_err(err)?;
        Ok(())
    }

    pub fn unblock_user(&self, token: &str, username: &str) -> Result<(), Error> {
        Self::bearer(ureq::post(&format!("{}/api/users/unblock", self.base)), token)
            .send_json(serde_json::json!({ "username": username }))
            .map_err(err)?;
        Ok(())
    }

    /// Delete the signed-in account (password-reconfirmed). Irreversible.
    pub fn delete_account(&self, token: &str, password: &str) -> Result<(), Error> {
        Self::bearer(ureq::post(&format!("{}/api/auth/delete-account", self.base)), token)
            .send_json(serde_json::json!({ "password": password }))
            .map_err(err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_url_derives_scheme_and_path() {
        assert_eq!(VegifyClient::new("https://api.vegify.app/").ws_url(), "wss://api.vegify.app/ws");
        assert_eq!(VegifyClient::new("http://localhost:8787").ws_url(), "ws://localhost:8787/ws");
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
