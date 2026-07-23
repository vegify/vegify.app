//! Vegify standing backend — Axum over SQLite-WAL, serving the SAME `/api/auth/*` + `/api/content/*`
//! contract the clients already speak (so they re-point, not rewrite). Reads/writes go through
//! vegify-core (one shared DAL with the desktop); auth is the Rust port of auth.ts. rusqlite is sync,
//! so every DB touch runs on an r2d2 WAL pool via spawn_blocking (concurrent readers, serialized writer).
//!
//! Run: DATABASE_PATH=<file> PORT=8787 cargo run -p vegify-server

mod auth;
mod blog;
mod content;
mod email;
mod error;
mod messages;
mod notifications;
mod safety;
mod usda;
// Reserved handles + username validation for the future `vegify.app/<username>/<recipe>` URLs. Locked
// now (a claimed handle can't be reclaimed); not yet called — signups are invite-only. `signup` will
// call `handles::validate_username` when usernames launch.
#[allow(dead_code)]
mod handles;
mod media;
mod ratelimit;

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use tower_http::trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::Level;

use crate::auth::{bearer_token, User};
use crate::error::AppError;
use crate::ratelimit::{ClientIp, RateLimiter};

type Pool = r2d2::Pool<SqliteConnectionManager>;

#[derive(Clone)]
struct AppState {
    pool: Pool,
    /// Fans a tiny `{"changed":"<kind>"}` signal to every connected /ws client after a content write
    /// commits — the push that replaces the desktop's poll. Best-effort: a send error just means nobody
    /// is listening. State holds only the Sender; each /ws connection `.subscribe()`s its own Receiver.
    change_tx: tokio::sync::broadcast::Sender<String>,
    /// In-process budgets for the auth surface (single-instance server → in-memory is authoritative).
    rate: Arc<RateLimiter>,
}

impl AppState {
    /// Broadcast a content-change signal (`kind` = "recipe" | "ingredient"). Clients re-pull on any frame.
    fn notify_change(&self, kind: &str) {
        let _ = self.change_tx.send(json!({ "changed": kind }).to_string());
    }
}

/// Run a blocking DB closure on a pooled connection, off the async runtime.
async fn db<T, F>(state: &AppState, f: F) -> Result<T, AppError>
where
    F: FnOnce(&Connection) -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(AppError::internal)?;
        f(&conn)
    })
    .await
    .map_err(AppError::internal)?
}

/// Validate the request's bearer token → the viewer (401 if absent/invalid). Sync; called inside `db`.
fn require_user(conn: &Connection, token: &str) -> Result<User, AppError> {
    auth::validate_session(conn, token)?.ok_or(AppError::Unauthorized)
}

#[derive(Deserialize)]
struct IdQuery {
    id: Option<String>,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

#[derive(Deserialize)]
struct DateQuery {
    date: Option<String>,
}

#[derive(Deserialize)]
struct RecentsQuery {
    limit: Option<u32>,
}

// ---- auth routes (JSON body in, token in the body out; no cookie/CSRF — for native clients) ----

#[derive(Deserialize)]
struct LoginBody {
    email: Option<String>,
    password: Option<String>,
}

async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<LoginBody>,
) -> Result<Json<Value>, AppError> {
    ratelimit::guard(&state.rate, ratelimit::LOGIN_IP, &ip).map_err(AppError::RateLimited)?;
    let (email, password) = match (body.email, body.password) {
        (Some(e), Some(p)) if !e.is_empty() && !p.is_empty() => (e, p),
        _ => {
            return Err(AppError::BadRequest(
                "Email and password are required.".into(),
            ))
        }
    };
    // The per-identifier failure budget is the actual credential-stuffing guard: keyed on the
    // account, not the address, so it holds through the SSR Lambda's egress aggregation and
    // against a distributed attacker. Only FAILURES count; a success clears the bucket.
    let fail_key = email.trim().to_lowercase();
    if let Some(retry) = state.rate.over(ratelimit::LOGIN_FAILS, &fail_key) {
        // Same greppable signal `guard` emits — a locked identifier IS the credential-stuffing tell.
        tracing::warn!(
            limit = ratelimit::LOGIN_FAILS.name,
            retry_after = retry,
            "rate_limited"
        );
        return Err(AppError::RateLimited(retry));
    }
    let out = db(&state, move |conn| {
        match auth::authenticate(conn, &email, &password)? {
            Some(user) => {
                let token = auth::create_session(conn, &user.id)?;
                tracing::info!(user = %user.id, "login ok");
                Ok(json!({ "token": token, "user": user }))
            }
            None => Err(AppError::InvalidCredentials),
        }
    })
    .await;
    match &out {
        Err(AppError::InvalidCredentials) => {
            let _ = state.rate.hit(ratelimit::LOGIN_FAILS, &fail_key);
        }
        Ok(_) => state.rate.clear(ratelimit::LOGIN_FAILS, &fail_key),
        Err(_) => {}
    }
    out.map(Json)
}

#[derive(Deserialize)]
struct SignupBody {
    name: Option<String>,
    email: Option<String>,
    password: Option<String>,
}

async fn signup(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<SignupBody>,
) -> Result<Json<Value>, AppError> {
    // Signups are disabled by default (invite-only while the app isn't open to the public). The server
    // is the authority — set VEGIFY_SIGNUPS_OPEN=1 to re-open (and flip SIGNUPS_ENABLED in @vegify/ui).
    if !vegify_config::server::signups_open() {
        return Err(AppError::Forbidden("Signups are disabled.".into()));
    }
    ratelimit::guard(&state.rate, ratelimit::SIGNUP_IP, &ip).map_err(AppError::RateLimited)?;
    let name = body.name.unwrap_or_default().trim().to_string();
    let email = body.email.unwrap_or_default().trim().to_string();
    let password = body.password.unwrap_or_default();
    if name.is_empty() || email.is_empty() {
        return Err(AppError::BadRequest("Name and email are required.".into()));
    }
    if password.chars().count() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters.".into(),
        ));
    }
    let verify_email = email.clone();
    let out = db(&state, move |conn| {
        if auth::email_exists(conn, &email)? {
            return Err(AppError::Conflict(
                "An account with that email already exists.".into(),
            ));
        }
        let user = auth::create_user(conn, &name, &email, &password)?;
        let token = auth::create_session(conn, &user.id)?;
        tracing::info!(user = %user.id, "signup");
        Ok(json!({ "token": token, "user": user }))
    })
    .await?;
    // New account → send a verification email, best-effort and outside the blocking closure (mirrors the
    // reset send). A second DB hit mints the token; signup already succeeded, so a send failure only logs.
    let to = verify_email.clone();
    if let Some((name, token)) = db(&state, move |conn| {
        auth::create_email_verification(conn, &verify_email)
    })
    .await?
    {
        email::send_email_verification(&to, &name, &token).await;
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
struct InviteBody {
    name: Option<String>,
    email: Option<String>,
    password: Option<String>,
}

/// Invite (create) a new account while public signups stay CLOSED — admin-only (the caller's session
/// must resolve to an email in VEGIFY_ADMIN_EMAILS). This is how invite-only onboarding works: the
/// operator (vegify-admin invite) calls it. Bypasses the signups gate BY DESIGN — the admin check is
/// the gate. Returns the created user + its handle.
async fn invite_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<InviteBody>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let name = body.name.unwrap_or_default().trim().to_string();
    let email = body.email.unwrap_or_default().trim().to_string();
    let password = body.password.unwrap_or_default();
    if name.is_empty() || email.is_empty() {
        return Err(AppError::BadRequest("Name and email are required.".into()));
    }
    if password.chars().count() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters.".into(),
        ));
    }
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        if !auth::is_admin(&me) {
            return Err(AppError::Forbidden("Admins only.".into()));
        }
        if auth::email_exists(conn, &email)? {
            return Err(AppError::Conflict(
                "An account with that email already exists.".into(),
            ));
        }
        let user = auth::create_user(conn, &name, &email, &password)?;
        tracing::info!(by = %me.id, new_user = %user.id, "account invited");
        Ok(json!({ "user": user }))
    })
    .await?;
    Ok(Json(out))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    if let Some(token) = bearer_token(&headers) {
        db(&state, move |conn| auth::invalidate_session(conn, &token)).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct BootstrapBody {
    email: Option<String>,
    password: Option<String>,
}

async fn bootstrap(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<BootstrapBody>,
) -> Result<Json<Value>, AppError> {
    // Claiming an invited (passwordless) account is a guessing surface — small per-IP budget.
    ratelimit::guard(&state.rate, ratelimit::BOOTSTRAP_IP, &ip).map_err(AppError::RateLimited)?;
    let email = body.email.unwrap_or_default().trim().to_string();
    let password = body.password.unwrap_or_default();
    if email.is_empty() || password.chars().count() < 8 {
        return Err(AppError::BadRequest(
            "Email and an 8+ character password are required.".into(),
        ));
    }
    let normalized = email.to_lowercase();
    db(&state, move |conn| {
        auth::set_initial_password(conn, &email, &password)
    })
    .await?;
    Ok(Json(json!({ "ok": true, "email": normalized })))
}

/// Whoami: resolve the bearer token to its user (401 if absent/invalid). The web SSR shell calls this
/// to turn its session cookie into the signed-in user for the auth gate + chrome.
async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<User>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let user = db(&state, move |conn| auth::validate_session(conn, &token)).await?;
    user.map(Json).ok_or(AppError::Unauthorized)
}

#[derive(Deserialize)]
struct ResetRequestBody {
    email: Option<String>,
}

/// Begin a password reset: mint a token (if the email matches an account) and email the link. ALWAYS
/// 200 — the response never reveals whether an email is registered. DB work runs on the blocking pool;
/// the SES send runs on the async side (best-effort, logged on failure).
async fn request_password_reset(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<ResetRequestBody>,
) -> Result<Json<Value>, AppError> {
    // Outbound-email budgets (per IP + per requested address) protect the target inbox and SES
    // reputation. Both fire identically whether or not the address has an account, so the 429
    // carries no registration signal — the 200 below stays enumeration-proof.
    ratelimit::guard(&state.rate, ratelimit::EMAIL_SEND_IP, &ip).map_err(AppError::RateLimited)?;
    let email = body.email.unwrap_or_default().trim().to_string();
    if !email.is_empty() {
        ratelimit::guard(&state.rate, ratelimit::EMAIL_SEND_ID, &email.to_lowercase())
            .map_err(AppError::RateLimited)?;
        let to = email.clone();
        if let Some((name, token)) = db(&state, move |conn| {
            auth::create_password_reset(conn, &email)
        })
        .await?
        {
            email::send_password_reset(&to, &name, &token).await;
        }
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct ResetConfirmBody {
    token: Option<String>,
    password: Option<String>,
}

/// Complete a password reset: set the new password, then invalidate the token + all of the account's
/// sessions. 400 on a missing/invalid/expired/used token or a too-short password.
async fn confirm_password_reset(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<ResetConfirmBody>,
) -> Result<Json<Value>, AppError> {
    ratelimit::guard(&state.rate, ratelimit::TOKEN_CONFIRM_IP, &ip)
        .map_err(AppError::RateLimited)?;
    let token = body.token.unwrap_or_default();
    let password = body.password.unwrap_or_default();
    if token.is_empty() {
        return Err(AppError::BadRequest("A reset token is required.".into()));
    }
    db(&state, move |conn| {
        auth::consume_password_reset(conn, &token, &password)
    })
    .await?;
    tracing::info!("password reset completed");
    Ok(Json(json!({ "ok": true })))
}

/// Begin (or resend) email verification: mint a token if the email matches an unverified account, and
/// email the link. ALWAYS 200 — like the reset request, the response never reveals registration state.
async fn request_email_verification(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<ResetRequestBody>,
) -> Result<Json<Value>, AppError> {
    // Same outbound-email budgets (and the same no-enumeration-signal property) as the reset request.
    ratelimit::guard(&state.rate, ratelimit::EMAIL_SEND_IP, &ip).map_err(AppError::RateLimited)?;
    let email = body.email.unwrap_or_default().trim().to_string();
    if !email.is_empty() {
        ratelimit::guard(&state.rate, ratelimit::EMAIL_SEND_ID, &email.to_lowercase())
            .map_err(AppError::RateLimited)?;
        let to = email.clone();
        if let Some((name, token)) = db(&state, move |conn| {
            auth::create_email_verification(conn, &email)
        })
        .await?
        {
            email::send_email_verification(&to, &name, &token).await;
        }
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct VerifyConfirmBody {
    token: Option<String>,
}

/// Complete email verification: stamp the account verified and burn the token. 400 on a
/// missing/invalid/expired/used token.
async fn confirm_email_verification(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<VerifyConfirmBody>,
) -> Result<Json<Value>, AppError> {
    ratelimit::guard(&state.rate, ratelimit::TOKEN_CONFIRM_IP, &ip)
        .map_err(AppError::RateLimited)?;
    let token = body.token.unwrap_or_default();
    if token.is_empty() {
        return Err(AppError::BadRequest(
            "A verification token is required.".into(),
        ));
    }
    db(&state, move |conn| {
        auth::consume_email_verification(conn, &token)
    })
    .await?;
    tracing::info!("email verified");
    Ok(Json(json!({ "ok": true })))
}

// ---- content routes ----
// READS and the bulk PULL are optionally-authed: a bearer identifies the viewer (who also sees their own
// non-public rows); without one the request is anonymous and vegify-core scopes it to public-only. WRITES
// and edit-loads require a bearer and stamp/guard userId server-side from the session.

/// Clamp the client-supplied page size so an (anonymous) catalog read is never unbounded; absent or
/// oversized → 100. The desktop reads its local cache directly and isn't subject to this. The rest of
/// the page (sort + keyset cursor) deserializes straight into `vegify_core::Page` from the query string.
fn page_limit(requested: Option<u32>) -> u32 {
    requested.unwrap_or(100).clamp(1, 100)
}

async fn list_recipes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut page): Query<vegify_core::Page>,
) -> Result<Json<Vec<vegify_core::RecipeCard>>, AppError> {
    let token = bearer_token(&headers);
    page.limit = Some(page_limit(page.limit));
    let out = db(&state, move |conn| {
        let viewer = auth::optional_viewer(conn, token);
        vegify_core::list_recipes(conn, viewer.as_deref(), &page).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
struct UsernameQuery {
    username: Option<String>,
}

#[derive(Deserialize)]
struct RecipeSlugQuery {
    username: Option<String>,
    slug: Option<String>,
}

/// Resolve `/<username>/<slug>` → { recipeId, canonicalSlug } (or null → 404). No bearer needed
/// (visibility is enforced when the recipe is then loaded by id); the web route 301s if canonical
/// differs from the requested slug, else loads the recipe.
async fn recipe_by_slug(
    State(state): State<AppState>,
    Query(q): Query<RecipeSlugQuery>,
) -> Result<Json<Option<vegify_core::RecipeSlugHit>>, AppError> {
    let (username, slug) = (q.username.unwrap_or_default(), q.slug.unwrap_or_default());
    if username.is_empty() || slug.is_empty() {
        return Err(AppError::BadRequest(
            "username and slug are required.".into(),
        ));
    }
    let out = db(&state, move |conn| {
        vegify_core::resolve_recipe_by_slug(conn, &username, &slug).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
struct SlugQuery {
    slug: Option<String>,
}

/// Resolve /ingredients/<slug> → { ingredientId, canonicalSlug } (or null → 404). The web route 301s
/// if canonical differs, else loads the ingredient by id.
async fn ingredient_by_slug(
    State(state): State<AppState>,
    Query(q): Query<SlugQuery>,
) -> Result<Json<Option<vegify_core::IngredientSlugHit>>, AppError> {
    let slug = q.slug.unwrap_or_default();
    if slug.is_empty() {
        return Err(AppError::BadRequest("slug is required.".into()));
    }
    let out = db(&state, move |conn| {
        vegify_core::resolve_ingredient_by_slug(conn, &slug).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
}

/// All public canonical URLs (recipes + ingredients) for the web's dynamic sitemap. Public data only —
/// no session, safe for a crawler-facing web route to proxy unauthenticated.
async fn sitemap(
    State(state): State<AppState>,
) -> Result<Json<vegify_core::SitemapData>, AppError> {
    let out = db(&state, move |conn| {
        vegify_core::public_sitemap(conn).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
}

/// Blog index (published posts, newest first). Public — the blog is the unauthenticated writing surface.
async fn blog_list(
    State(state): State<AppState>,
) -> Result<Json<Vec<blog::PostSummary>>, AppError> {
    let out = db(&state, |conn| {
        blog::list_posts(conn).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
}

/// One published post by ?slug= (null → 404). Public.
async fn blog_detail(
    State(state): State<AppState>,
    Query(q): Query<SlugQuery>,
) -> Result<Json<Option<blog::PostFull>>, AppError> {
    let slug = q.slug.unwrap_or_default();
    if slug.is_empty() {
        return Err(AppError::BadRequest("slug is required.".into()));
    }
    let out = db(&state, move |conn| {
        blog::get_post(conn, &slug).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
}

/// Public profile by handle. No bearer required; an optional one identifies the viewer so they also
/// see their own non-public recipes when viewing themselves. 404 when the handle has no account.
async fn profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<UsernameQuery>,
) -> Result<Json<Option<vegify_core::Profile>>, AppError> {
    let username = q.username.unwrap_or_default();
    if username.is_empty() {
        return Err(AppError::BadRequest("A username is required.".into()));
    }
    let token = bearer_token(&headers);
    // Null (not 404) for an unknown handle — mirrors recipe_detail; the web route renders not-found.
    let out = db(&state, move |conn| {
        let viewer = auth::optional_viewer(conn, token);
        vegify_core::get_profile(conn, &username, viewer.as_deref()).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
}

// ---- messages routes (1:1 DMs; all authed; addressing by public handle — see messages.rs) ----

#[derive(Deserialize)]
struct ThreadQuery {
    with: Option<String>,
}

#[derive(Deserialize)]
struct SendMessageBody {
    to: Option<String>,
    body: Option<String>,
}

async fn message_conversations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<messages::ConversationSummary>>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        messages::list_conversations(conn, &me.id)
    })
    .await?;
    Ok(Json(out))
}

async fn message_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ThreadQuery>,
) -> Result<Json<messages::Thread>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let with = q.with.unwrap_or_default();
    if with.is_empty() {
        return Err(AppError::BadRequest("A username is required.".into()));
    }
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        messages::thread(conn, &me.id, &with)
    })
    .await?;
    // Opening a thread consumes unread state — nudge the viewer's OTHER clients so badges drop.
    state.notify_change("message");
    Ok(Json(out))
}

async fn message_send(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SendMessageBody>,
) -> Result<Json<messages::Message>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let to = body.to.unwrap_or_default();
    let text = body.body.unwrap_or_default();
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        messages::send(conn, &me.id, &to, &text)
    })
    .await?;
    state.notify_change("message");
    Ok(Json(out))
}

// ---- notifications routes (the bell; all authed — see notifications.rs) ----

async fn notifications_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<notifications::Notification>>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        notifications::list(conn, &me.id)
    })
    .await?;
    Ok(Json(out))
}

async fn notifications_unread(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let count = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        notifications::unread_count(conn, &me.id)
    })
    .await?;
    Ok(Json(json!({ "count": count })))
}

async fn notifications_read_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        notifications::mark_all_read(conn, &me.id)
    })
    .await?;
    // Other clients drop their bell badges immediately.
    state.notify_change("notification");
    Ok(Json(json!({ "ok": true })))
}

async fn message_unread(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let count = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        messages::unread_count(conn, &me.id)
    })
    .await?;
    Ok(Json(json!({ "count": count })))
}

async fn save_recipe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<vegify_core::SaveRecipeInput>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let id = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::do_save_recipe(conn, &input, Some(&me.id)).map_err(AppError::from)
    })
    .await?;
    state.notify_change("recipe");
    tracing::info!(%id, "saved recipe");
    Ok(Json(json!({ "id": id })))
}

async fn delete_recipe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let id =
        q.id.ok_or_else(|| AppError::BadRequest("id is required.".into()))?;
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::do_delete_recipe(conn, &id, Some(&me.id)).map_err(AppError::from)
    })
    .await?;
    state.notify_change("recipe");
    Ok(Json(json!({ "ok": true })))
}

async fn list_ingredients(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(mut page): Query<vegify_core::Page>,
) -> Result<Json<Vec<vegify_core::IngredientCard>>, AppError> {
    let token = bearer_token(&headers);
    page.limit = Some(page_limit(page.limit));
    let out = db(&state, move |conn| {
        let viewer = auth::optional_viewer(conn, token);
        vegify_core::list_ingredients(conn, viewer.as_deref(), &page).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
}

async fn save_ingredient(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<vegify_core::SaveIngredientInput>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let (id, notified) = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        // Pre-existence decides "update" — creating an ingredient affects nobody's recipes yet.
        let existed: bool = input
            .id
            .as_deref()
            .map(|iid| {
                conn.query_row(
                    "SELECT COUNT(*) FROM ingredients WHERE id = ?1",
                    [iid],
                    |r| r.get::<_, i64>(0),
                )
                .map(|c| c > 0)
            })
            .transpose()?
            .unwrap_or(false);
        let id =
            vegify_core::do_save_ingredient(conn, &input, Some(&me.id)).map_err(AppError::from)?;
        // The bell's v1 producer: an UPDATED communal ingredient notifies everyone whose recipes use
        // it (their nutrition just changed) — never the editor, collapsed per recipient.
        let notified = if existed {
            notifications::notify_ingredient_updated(conn, &me, &id)?
        } else {
            0
        };
        Ok((id, notified))
    })
    .await?;
    state.notify_change("ingredient");
    if notified > 0 {
        state.notify_change("notification");
    }
    tracing::info!(%id, notified, "saved ingredient");
    Ok(Json(json!({ "id": id })))
}

async fn delete_ingredient(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let id =
        q.id.ok_or_else(|| AppError::BadRequest("id is required.".into()))?;
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::do_delete_ingredient(conn, &id, Some(&me.id)).map_err(AppError::from)
    })
    .await?;
    state.notify_change("ingredient");
    Ok(Json(json!({ "ok": true })))
}

/// Undo a soft delete (the greyed recipe row's "restore?" affordance). Owner-gated in the DAL.
async fn restore_ingredient(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let id =
        q.id.ok_or_else(|| AppError::BadRequest("id is required.".into()))?;
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::do_restore_ingredient(conn, &id, Some(&me.id)).map_err(AppError::from)
    })
    .await?;
    state.notify_change("ingredient");
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportBody {
    target_type: Option<String>,
    target_id: Option<String>,
    reason: Option<String>,
    note: Option<String>,
}

/// Report content or a user (App Review 1.2). Any signed-in user.
async fn report_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<ReportBody>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let (tt, tid, reason) = (
        b.target_type.unwrap_or_default(),
        b.target_id.unwrap_or_default(),
        b.reason.unwrap_or_default(),
    );
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        safety::report(conn, &me.id, &tt, &tid, &reason, b.note.as_deref())
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct BlockBody {
    username: Option<String>,
}

/// Block a user (App Review 1.2): no DMs either way; their content leaves your reads.
async fn block_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<BlockBody>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let username = b.username.unwrap_or_default();
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        safety::block(conn, &me.id, &username)
    })
    .await?;
    state.notify_change("content"); // reads change (blocked user's content drops out)
    Ok(Json(json!({ "ok": true })))
}

async fn unblock_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<BlockBody>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let username = b.username.unwrap_or_default();
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        safety::unblock(conn, &me.id, &username)
    })
    .await?;
    state.notify_change("content");
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct DeleteAccountBody {
    password: Option<String>,
}

/// Delete the signed-in account (App Review 5.1.1(v)): password-reconfirmed, then a full cascade —
/// the user's content (owned ingredients/recipes), sessions, DMs, blocks, and reports all go via
/// ON DELETE CASCADE when the user row is removed. Irreversible.
async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<DeleteAccountBody>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let password = b.password.unwrap_or_default();
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        auth::delete_account(conn, &me.id, &password)
    })
    .await?;
    state.notify_change("content");
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct UploadUrlBody {
    content_type: Option<String>,
}

/// Mint a presigned PUT for one image (signed-in users; attachment is owner-gated separately).
async fn upload_url(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UploadUrlBody>,
) -> Result<Json<vegify_api_types::UploadTicket>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    db(&state, move |conn| require_user(conn, &token).map(|_| ())).await?;
    let content_type = body.content_type.unwrap_or_default();
    Ok(Json(media::presign_upload(&content_type).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachPhotoBody {
    /// Either directly, or via recipe_id (resolved to the recipe's as-ingredient — a recipe IS an
    /// ingredient, and the photo lives on that row).
    ingredient_id: Option<String>,
    recipe_id: Option<String>,
    key: Option<String>,
    content_type: Option<String>,
}

async fn attach_photo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AttachPhotoBody>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let (ing, rid, key, ct) = (
        body.ingredient_id.unwrap_or_default(),
        body.recipe_id.unwrap_or_default(),
        body.key.unwrap_or_default(),
        body.content_type.unwrap_or_default(),
    );
    if (ing.is_empty() && rid.is_empty()) || key.is_empty() {
        return Err(AppError::BadRequest(
            "ingredientId (or recipeId) and key are required.".into(),
        ));
    }
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        let target = if !ing.is_empty() {
            ing
        } else {
            conn.query_row(
                "SELECT as_ingredient_id FROM recipes WHERE id = ?1",
                [&rid],
                |r| r.get(0),
            )
            .map_err(|_| AppError::BadRequest("No such recipe.".into()))?
        };
        media::attach_photo(conn, &me.id, &target, &key, &ct)
    })
    .await?;
    state.notify_change("recipe");
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachAvatarBody {
    key: Option<String>,
    content_type: Option<String>,
}

async fn attach_avatar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AttachAvatarBody>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let (key, ct) = (
        body.key.unwrap_or_default(),
        body.content_type.unwrap_or_default(),
    );
    if key.is_empty() {
        return Err(AppError::BadRequest("key is required.".into()));
    }
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        media::attach_avatar(conn, &me.id, &key, &ct)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn recipe_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Option<vegify_core::RecipeView>>, AppError> {
    let token = bearer_token(&headers);
    let out = db(&state, move |conn| {
        let viewer = auth::optional_viewer(conn, token);
        match q.id {
            Some(id) => vegify_core::recipe(conn, id, viewer.as_deref()).map_err(AppError::from),
            None => Ok(None),
        }
    })
    .await?;
    Ok(Json(out))
}

async fn recipe_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Option<vegify_core::RecipeEditData>>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        match q.id {
            Some(id) => {
                vegify_core::recipe_for_edit(conn, id, Some(&me.id)).map_err(AppError::from)
            }
            None => Ok(None),
        }
    })
    .await?;
    Ok(Json(out))
}

async fn ingredient_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Option<vegify_core::IngredientEditData>>, AppError> {
    let token = bearer_token(&headers);
    let out = db(&state, move |conn| {
        let viewer = auth::optional_viewer(conn, token);
        match q.id {
            Some(id) => {
                vegify_core::ingredient(conn, id, viewer.as_deref()).map_err(AppError::from)
            }
            None => Ok(None),
        }
    })
    .await?;
    Ok(Json(out))
}

async fn ingredient_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Option<vegify_core::IngredientEditData>>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        match q.id {
            Some(id) => {
                vegify_core::ingredient_for_edit(conn, id, Some(&me.id)).map_err(AppError::from)
            }
            None => Ok(None),
        }
    })
    .await?;
    Ok(Json(out))
}

async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<vegify_core::IngredientSearchResult>>, AppError> {
    let token = bearer_token(&headers);
    let q = query.q.unwrap_or_default();
    let out = db(&state, move |conn| {
        let viewer = auth::optional_viewer(conn, token);
        vegify_core::search_ingredients(conn, q, viewer.as_deref()).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
}

async fn pull(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<content::PullPayload>, AppError> {
    let token = bearer_token(&headers);
    let out = db(&state, move |conn| {
        let viewer = auth::optional_viewer(conn, token);
        content::pull(conn, viewer.as_deref())
    })
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
struct WsAuth {
    token: Option<String>,
}

/// WebSocket push (`GET /ws`): a client connects, authenticates (Bearer header — the desktop tungstenite
/// client — or `?token=` for browsers, which can't set WS handshake headers), then receives a tiny
/// `{"changed":"<kind>"}` text frame whenever any content write commits. This replaces the desktop's 60s
/// poll — a push arrives, the client pulls immediately. Auth runs BEFORE the upgrade so a bad token gets a
/// clean 401 instead of a dangling socket.
async fn ws(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<WsAuth>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, AppError> {
    let token = bearer_token(&headers)
        .or(q.token)
        .ok_or(AppError::Unauthorized)?;
    db(&state, move |conn| require_user(conn, &token).map(|_| ())).await?;
    let rx = state.change_tx.subscribe();
    Ok(upgrade.on_upgrade(move |socket| ws_loop(socket, rx)))
}

/// Forward broadcast change signals to one connected client; ping every 30s to keep the CloudFront WS
/// tunnel alive and detect a dead peer; exit on client close/error. A lagged receiver (the client fell
/// further than the buffer behind) gets a single "pull everything" nudge so it self-heals.
async fn ws_loop(mut socket: WebSocket, mut rx: tokio::sync::broadcast::Receiver<String>) {
    use tokio::sync::broadcast::error::RecvError;
    let mut keepalive = tokio::time::interval(std::time::Duration::from_secs(30));
    keepalive.tick().await; // the first tick fires immediately — consume it so the real cadence is 30s
    loop {
        tokio::select! {
            change = rx.recv() => match change {
                Ok(text) => if socket.send(Message::Text(text.into())).await.is_err() { break },
                Err(RecvError::Lagged(_)) => {
                    if socket.send(Message::Text("{\"changed\":\"all\"}".into())).await.is_err() { break }
                }
                Err(RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Ok(Message::Close(_))) | Some(Err(_)) => break,
                Some(Ok(_)) => {} // ignore client→server frames (pongs etc.)
            },
            _ = keepalive.tick() => if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break },
        }
    }
}

// ---- diary (log_entries): the PRIVATE food log. Every endpoint is hard-authed to the owner and the
// diary is NEVER in the anonymous content pull or the sitemap; authed device sync is GET /api/log/pull.
// Writes fan a WS "diary" change so the owner's other signed-in devices re-pull (the /ws connection is
// per-user, so this only nudges the owner — no one else learns anything). ----

/// POST/PATCH /api/log/entries — create or update a diary entry (upsert-by-id; owner-gated on update).
async fn save_log_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<vegify_core::SaveLogEntryInput>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let id = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::do_save_log_entry(conn, &input, &me.id).map_err(AppError::from)
    })
    .await?;
    state.notify_change("diary");
    Ok(Json(json!({ "id": id })))
}

/// DELETE /api/log/entries?id= — soft-delete a diary entry (owner-gated).
async fn delete_log_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let id =
        q.id.ok_or_else(|| AppError::BadRequest("id is required.".into()))?;
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::do_delete_log_entry(conn, &id, &me.id).map_err(AppError::from)
    })
    .await?;
    state.notify_change("diary");
    Ok(Json(json!({ "ok": true })))
}

/// GET /api/log/day?date=YYYY-MM-DD — the day's entries + server-computed nutrient totals.
async fn log_day(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<DateQuery>,
) -> Result<Json<vegify_core::DayLog>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let date = q
        .date
        .ok_or_else(|| AppError::BadRequest("date is required.".into()))?;
    let day = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::log_day(conn, &me.id, &date).map_err(AppError::from)
    })
    .await?;
    Ok(Json(day))
}

/// GET /api/log/recents?limit= — distinct recently-logged ingredients (default 20, capped 100).
async fn log_recents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RecentsQuery>,
) -> Result<Json<Vec<vegify_core::RecentIngredient>>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let limit = q.limit.unwrap_or(20).min(100) as i64;
    let recents = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::log_recents(conn, &me.id, limit).map_err(AppError::from)
    })
    .await?;
    Ok(Json(recents))
}

/// GET /api/log/pull — the viewer's ENTIRE diary (each entry with its frozen snapshot) for authed
/// device sync. The desktop reconciles this into its local-first cache; a separate channel from the
/// anonymous /api/content/pull, which never carries private log data.
async fn log_pull(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<vegify_core::LogPull>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let pull = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::log_pull(conn, &me.id).map_err(AppError::from)
    })
    .await?;
    Ok(Json(pull))
}

/// GET /api/profile — the viewer's nutrition profile (all-null defaults when never set). PRIVATE, like
/// the diary: it drives the personalized targets in the day view and is never public.
async fn get_nutrition_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<vegify_core::NutritionProfile>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let profile = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::get_nutrition_profile(conn, &me.id).map_err(AppError::from)
    })
    .await?;
    Ok(Json(profile))
}

/// POST /api/profile — upsert the viewer's nutrition profile (age/sex/weight/pregnancy/supplements).
/// Changes personalized targets on the next `GET /api/log/day`. PRIVATE, owner-scoped. Fans a WS
/// "profile" change (like the diary writes) so the owner's other signed-in devices re-pull and their
/// targets re-personalize without a manual sync.
async fn save_nutrition_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<vegify_core::NutritionProfile>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::save_nutrition_profile(conn, &me.id, &input).map_err(AppError::from)
    })
    .await?;
    state.notify_change("profile");
    Ok(Json(json!({ "ok": true })))
}

/// POST /api/day-supplements — upsert the supplements taken on a specific date (the day's plan). Body =
/// `{ date, b12, vitD, algaeOil }`. Changes that day's supplement coverage (and, by carry-forward, later
/// days without their own record) on the next `GET /api/log/day`. PRIVATE, owner-scoped. Fans a WS
/// "diary" change (like the log writes) so the owner's other devices re-pull.
async fn save_day_supplements(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<vegify_core::DaySupplementsRecord>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::save_day_supplements(
            conn,
            &me.id,
            &input.date,
            &vegify_core::DaySupplements {
                b12: input.b12,
                vit_d: input.vit_d,
                algae_oil: input.algae_oil,
            },
        )
        .map_err(AppError::from)
    })
    .await?;
    state.notify_change("diary");
    Ok(Json(json!({ "ok": true })))
}

/// Idempotent additive migration applied on boot. The live EBS DB predates the password-reset table
/// and the `users.email_verified_at` column; the server is the sole writer, so this is the migration
/// path (the server analogue of the web's old `ensure-schema.mjs`). Safe to re-run every boot.
fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS password_reset_tokens (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            hashed_token TEXT NOT NULL UNIQUE,
            expires_at INTEGER NOT NULL,
            used_at INTEGER,
            created_at INTEGER,
            updated_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS password_reset_tokens_user_idx ON password_reset_tokens(user_id);
        CREATE TABLE IF NOT EXISTS email_verification_tokens (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            hashed_token TEXT NOT NULL UNIQUE,
            expires_at INTEGER NOT NULL,
            used_at INTEGER,
            created_at INTEGER,
            updated_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS email_verification_tokens_user_idx ON email_verification_tokens(user_id);
        CREATE TABLE IF NOT EXISTS slug_history (
            id TEXT PRIMARY KEY,
            slug TEXT NOT NULL,
            scope TEXT,
            target_id TEXT NOT NULL REFERENCES ingredients(id) ON DELETE CASCADE,
            created_at INTEGER,
            updated_at INTEGER
        );
        CREATE UNIQUE INDEX IF NOT EXISTS slug_history_scope_slug_uq ON slug_history(scope, slug);
        CREATE TABLE IF NOT EXISTS posts (
            id TEXT PRIMARY KEY,
            slug TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            published_at TEXT NOT NULL,
            date_display TEXT NOT NULL,
            body TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'published',
            created_at INTEGER,
            updated_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS posts_status_published_idx ON posts(status, published_at);
        CREATE TABLE IF NOT EXISTS log_entries (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            date TEXT NOT NULL,
            slot TEXT,
            ingredient_id TEXT NOT NULL REFERENCES ingredients(id) ON DELETE RESTRICT,
            amount_id TEXT NOT NULL REFERENCES amounts(id) ON DELETE CASCADE,
            calories_per_100g REAL,
            logged_at INTEGER,
            deleted_at INTEGER,
            created_at INTEGER,
            updated_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS log_entries_user_date_idx ON log_entries(user_id, date);
        CREATE INDEX IF NOT EXISTS log_entries_user_logged_idx ON log_entries(user_id, logged_at);
        CREATE TABLE IF NOT EXISTS log_entry_nutrient (
            id TEXT PRIMARY KEY,
            log_entry_id TEXT NOT NULL REFERENCES log_entries(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            amount_per_100g REAL NOT NULL,
            unit TEXT NOT NULL,
            created_at INTEGER,
            updated_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS log_entry_nutrient_entry_idx ON log_entry_nutrient(log_entry_id);
        CREATE TABLE IF NOT EXISTS profiles (
            user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            birth_year INTEGER,
            dri_sex TEXT,
            weight_kg REAL,
            pregnancy INTEGER,
            lactation INTEGER,
            created_at INTEGER,
            updated_at INTEGER
        );
        CREATE TABLE IF NOT EXISTS day_supplements (
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            date TEXT NOT NULL,
            b12 INTEGER,
            vit_d INTEGER,
            algae_oil INTEGER,
            created_at INTEGER,
            updated_at INTEGER,
            PRIMARY KEY (user_id, date)
        );",
    )?;
    // 1:1 direct messages + the notification feed (server-owned, like posts — online-only, never in
    // the shared content schema).
    messages::ensure_tables(conn)?;
    notifications::ensure_tables(conn)?;
    safety::ensure_tables(conn)?;
    // SQLite has no `ADD COLUMN IF NOT EXISTS` — guard the ALTER with a pragma check so re-runs don't error.
    let has_col: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('users') WHERE name = 'email_verified_at'",
        [],
        |r| r.get(0),
    )?;
    if has_col == 0 {
        conn.execute("ALTER TABLE users ADD COLUMN email_verified_at INTEGER", [])?;
    }
    // Public handles (`users.username`). SQLite can't ADD a UNIQUE NOT NULL column to a populated
    // table, so add it nullable, backfill every existing row with a derived unique handle, then
    // enforce uniqueness with an index. New signups always set it (auth::create_user).
    let has_username: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('users') WHERE name = 'username'",
        [],
        |r| r.get(0),
    )?;
    if has_username == 0 {
        conn.execute("ALTER TABLE users ADD COLUMN username TEXT", [])?;
        let rows: Vec<(String, String, String)> = {
            let mut stmt = conn.prepare("SELECT id, name, email FROM users")?;
            let v = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        for (id, name, email) in rows {
            // Sequential so each derive sees handles assigned earlier this pass (dedup holds).
            let handle = auth::derive_unique_username(conn, &name, &email, &id)?;
            conn.execute(
                "UPDATE users SET username = ?1 WHERE id = ?2",
                [&handle, &id],
            )?;
        }
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS users_username_uq ON users(username)",
            [],
        )?;
    }
    // `ingredients.slug` (SEO URL segment). Guard on the table existing (the migration test sets up
    // only `users`) and on the column being absent. Backfill of existing rows runs after ensure_schema
    // in main (it needs vegify_core's scoped slug generation, not just DDL).
    let has_ingredients: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'ingredients'",
        [],
        |r| r.get(0),
    )?;
    if has_ingredients > 0 {
        let has_slug: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('ingredients') WHERE name = 'slug'",
            [],
            |r| r.get(0),
        )?;
        if has_slug == 0 {
            conn.execute("ALTER TABLE ingredients ADD COLUMN slug TEXT", [])?;
        }
        // Provenance for imported reference data (the USDA catalog) — NULL for user content.
        let has_source: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('ingredients') WHERE name = 'source'",
            [],
            |r| r.get(0),
        )?;
        if has_source == 0 {
            conn.execute("ALTER TABLE ingredients ADD COLUMN source TEXT", [])?;
        }
        // Avatar media key on users — additive, NULL = none.
        let has_avatar: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('users') WHERE name = 'avatar_key'",
            [],
            |r| r.get(0),
        )?;
        if has_avatar == 0 {
            conn.execute("ALTER TABLE users ADD COLUMN avatar_key TEXT", [])?;
        }
        // Soft-delete tombstone (see packages/db schema note) — additive, NULL = live.
        let has_deleted: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('ingredients') WHERE name = 'deleted_at'",
            [],
            |r| r.get(0),
        )?;
        if has_deleted == 0 {
            conn.execute("ALTER TABLE ingredients ADD COLUMN deleted_at INTEGER", [])?;
        }
    }
    // MIGRATION: supplements moved from `profiles` (a standing setting) to per-day `day_supplements`.
    // On a DB that predates the move, the profiles table still carries the old supplement columns; seed
    // a single carry-forward FLOOR row (date '1970-01-01') per user from those flags so pre-existing
    // coverage survives (get_day_supplements picks the most-recent row <= the viewed date). Guarded on
    // the old columns still existing (a fresh DB never grew them) and idempotent (skips users who already
    // have a row). The dead profiles.supplement_* columns are then left untouched — SQLite can't cheaply
    // drop a column, and nothing reads them anymore.
    let has_supp_col: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('profiles') WHERE name = 'supplement_b12'",
        [],
        |r| r.get(0),
    )?;
    if has_supp_col == 1 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO day_supplements (user_id, date, b12, vit_d, algae_oil, created_at, updated_at)
             SELECT user_id, '1970-01-01',
                    COALESCE(supplement_b12, 0), COALESCE(supplement_vit_d, 0),
                    COALESCE(supplement_algae_oil, 0), ?1, ?1
             FROM profiles
             WHERE (supplement_b12 = 1 OR supplement_vit_d = 1 OR supplement_algae_oil = 1)
               AND user_id NOT IN (SELECT user_id FROM day_supplements)",
            [now],
        )?;
    }
    Ok(())
}

/// Liveness probe — unauthenticated, no DB touch. The release pipeline polls this through CloudFront
/// after the server deploys, as the gate before it publishes the web + desktop clients.
async fn health() -> &'static str {
    "ok"
}

/// A general per-IP request cap across EVERY route (the auth endpoints layer their stricter limits on
/// top in the handlers). This is what protects the public READ endpoints — content pull, search,
/// profiles — which are otherwise unthrottled: no single source can overwhelm the nano or scrape it at
/// speed. `/health` is exempt so the deploy's liveness poll is never throttled.
async fn rate_limit_mw(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if req.uri().path() != "/health" {
        if let Err(retry) = ratelimit::guard(&state.rate, ratelimit::GENERAL_IP, &ip) {
            return AppError::RateLimited(retry).into_response();
        }
    }
    next.run(req).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Structured logs (RUST_LOG overrides the default). Emits to stdout → the systemd journal on the
    // box (`journalctl -u vegify`); the TraceLayer below adds a per-request span with status + latency.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,vegify_server=debug"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();

    let db_path = vegify_config::server::database_path();
    let port = vegify_config::server::port();

    // WAL on EBS-class storage: concurrent readers + a serialized writer (no NFS, no reserved
    // concurrency). busy_timeout lets a waiting writer retry instead of erroring under contention.
    let manager = SqliteConnectionManager::file(&db_path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )
    });
    let pool = r2d2::Pool::builder().max_size(8).build(manager)?;
    // Apply the idempotent additive auth-reset migration before serving (the live DB predates these).
    let setup_conn = pool.get()?;
    ensure_schema(&setup_conn)?;
    // Backfill SEO slugs for any rows created before the slug column (idempotent — skips rows that
    // already have one). Server is the sole slug authority; the next pull carries them to replicas.
    vegify_core::backfill_all_slugs(&setup_conn)?;
    // Migrate the blog posts into the DB on first boot (no-op once any post exists). Posts are data
    // now, not code — authoring one never bumps the app version.
    blog::seed_if_empty(&setup_conn)?;
    // The USDA plant catalog (communal reference ingredients) — DATA lives in S3, not the repo:
    // marker-gated fetch of the processed artifact from the Data bucket, then a one-transaction
    // ingest (~2k foods, seconds, once). Missing/corrupt data warns and serves without the catalog —
    // reference data is an enhancement, never a boot blocker.
    if usda::catalog_missing(&setup_conn)? {
        if let Some(artifact) = usda::fetch_artifact().await {
            match usda::ingest(&setup_conn, &artifact) {
                Ok(foods) => tracing::info!(foods, "USDA plant catalog seeded"),
                Err(e) => {
                    tracing::warn!(error = %e, "USDA catalog ingest failed — serving without it")
                }
            }
        }
    }
    // Change-fanout bus: write handlers `send` a signal, each /ws client `subscribe`s a Receiver. Buffer
    // 64 — a client that lags further behind gets a Lagged error and a "pull all" nudge (it self-heals).
    let (change_tx, _) = tokio::sync::broadcast::channel::<String>(64);
    let state = AppState {
        pool,
        change_tx,
        rate: Arc::new(RateLimiter::new()),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/auth/login", post(login))
        .route("/api/auth/signup", post(signup))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/bootstrap", post(bootstrap))
        .route("/api/auth/session", get(session))
        .route(
            "/api/auth/password-reset/request",
            post(request_password_reset),
        )
        .route(
            "/api/auth/password-reset/confirm",
            post(confirm_password_reset),
        )
        .route(
            "/api/auth/email-verification/request",
            post(request_email_verification),
        )
        .route(
            "/api/auth/email-verification/confirm",
            post(confirm_email_verification),
        )
        .route("/api/messages/conversations", get(message_conversations))
        .route("/api/messages/thread", get(message_thread))
        .route("/api/messages/send", post(message_send))
        .route("/api/messages/unread", get(message_unread))
        .route("/api/notifications", get(notifications_list))
        .route("/api/notifications/unread", get(notifications_unread))
        .route("/api/notifications/read", post(notifications_read_all))
        .route(
            "/api/content/recipes",
            get(list_recipes).post(save_recipe).delete(delete_recipe),
        )
        .route(
            "/api/content/ingredients",
            get(list_ingredients)
                .post(save_ingredient)
                .delete(delete_ingredient),
        )
        .route("/api/content/recipe-detail", get(recipe_detail))
        .route("/api/content/recipe-edit", get(recipe_edit))
        .route("/api/content/ingredient-detail", get(ingredient_detail))
        .route("/api/content/ingredient-restore", post(restore_ingredient))
        .route("/api/content/upload-url", post(upload_url))
        .route("/api/content/attach-photo", post(attach_photo))
        .route("/api/content/attach-avatar", post(attach_avatar))
        .route("/api/content/report", post(report_content))
        .route("/api/users/block", post(block_user))
        .route("/api/users/unblock", post(unblock_user))
        .route("/api/auth/delete-account", post(delete_account))
        .route("/api/auth/invite", post(invite_account))
        .route("/api/content/ingredient-edit", get(ingredient_edit))
        .route("/api/content/search", get(search))
        .route("/api/content/pull", get(pull))
        // Public profile by handle — optionally-authed (a signed-in viewer also sees their own
        // non-public recipes on their own profile). Unlike the rest of /api/content, no auth required.
        .route("/api/content/profile", get(profile))
        .route("/api/content/recipe-by-slug", get(recipe_by_slug))
        .route("/api/content/ingredient-by-slug", get(ingredient_by_slug))
        .route("/api/content/sitemap", get(sitemap))
        .route("/api/content/blog", get(blog_list))
        .route("/api/content/blog-detail", get(blog_detail))
        // Food diary (PRIVATE per-user — every handler hard-auths the owner; never in the anon pull or
        // sitemap). POST/PATCH both upsert-by-id; day totals roll up via the nested-recipe CTE.
        .route(
            "/api/log/entries",
            post(save_log_entry)
                .patch(save_log_entry)
                .delete(delete_log_entry),
        )
        .route("/api/log/day", get(log_day))
        .route("/api/log/recents", get(log_recents))
        .route("/api/log/pull", get(log_pull))
        // Per-day supplements (PRIVATE — part of the day's plan). POST upserts the date's record; the
        // day's effective supplements come back in GET /api/log/day, and all records ride the log pull.
        .route("/api/day-supplements", post(save_day_supplements))
        // Nutrition PROFILE (PRIVATE per-user — drives personalized vegan-aware targets). GET reads it
        // (defaults when unset), POST upserts it.
        .route(
            "/api/profile",
            get(get_nutrition_profile).post(save_nutrition_profile),
        )
        // WebSocket push: content writes fan out here so the desktop pulls on change instead of polling.
        .route("/ws", get(ws))
        // Per-request span (method, path) + a response line with status + latency; failures at ERROR.
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                )
                .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
        )
        // Gzip responses when the client asks (ureq/fetch both do): the USDA catalog pushed the full
        // content pull to ~3.6 MB raw — ~10x smaller on the wire. Skips WS upgrades automatically.
        .layer(tower_http::compression::CompressionLayer::new())
        // General per-IP cap over every route (auth endpoints add stricter limits in-handler). Inside
        // the TraceLayer above, so a throttled request is still logged. state is cloned in — the router
        // takes ownership of the original just below.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_mw,
        ))
        .with_state(state);

    // Bind all interfaces so CloudFront can reach the origin (the SG locks the port to CloudFront's
    // prefix list — that's the access control, not the bind address). 127.0.0.1 would be loopback-only.
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, db = %db_path, "vegify-server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod migration_tests {
    use super::*;

    #[test]
    fn ensure_schema_is_idempotent_and_additive() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT, email TEXT, password_hash TEXT, created_at INTEGER, updated_at INTEGER);",
        )
        .unwrap();
        // Run twice: the first creates the table + adds the column, the second is a clean no-op.
        ensure_schema(&conn).unwrap();
        ensure_schema(&conn).unwrap();
        let prt_cols: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('password_reset_tokens')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            prt_cols >= 6,
            "password_reset_tokens should be created with its columns"
        );
        let evt_cols: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('email_verification_tokens')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            evt_cols >= 6,
            "email_verification_tokens should be created with its columns"
        );
        let added: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('users') WHERE name = 'email_verified_at'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(added, 1, "email_verified_at should be added exactly once");
    }

    #[test]
    fn supplements_migrate_from_profile_to_a_carry_forward_floor_row() {
        // Reproduce a PRE-move DB: `profiles` still carries the old supplement columns, and a user has
        // B12 + algae-oil flagged (John's live shape). This is the risky path the move must preserve.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT, email TEXT, password_hash TEXT, created_at INTEGER, updated_at INTEGER);
             INSERT INTO users(id, name, email) VALUES ('u1', 'John', 'j@x');
             CREATE TABLE profiles (
                user_id TEXT PRIMARY KEY, birth_year INTEGER, dri_sex TEXT, weight_kg REAL,
                pregnancy INTEGER, lactation INTEGER,
                supplement_b12 INTEGER, supplement_vit_d INTEGER, supplement_algae_oil INTEGER,
                created_at INTEGER, updated_at INTEGER);
             INSERT INTO profiles(user_id, supplement_b12, supplement_vit_d, supplement_algae_oil)
                VALUES ('u1', 1, 0, 1);",
        )
        .unwrap();

        ensure_schema(&conn).unwrap();

        // A single floor row ('1970-01-01') carries the old flags, so carry-forward covers every real day.
        let (date, b12, vit_d, algae): (String, i64, i64, i64) = conn
            .query_row(
                "SELECT date, b12, vit_d, algae_oil FROM day_supplements WHERE user_id = 'u1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(date, "1970-01-01");
        assert_eq!((b12, vit_d, algae), (1, 0, 1), "the old flags carried over");

        // Idempotent: a second boot must NOT insert a duplicate (the user already has a row).
        ensure_schema(&conn).unwrap();
        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM day_supplements WHERE user_id = 'u1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1, "re-run seeds no duplicate floor row");

        // The effective supplements for any real day inherit the floor via carry-forward.
        let eff = vegify_core::get_day_supplements(&conn, "u1", "2026-07-23").unwrap();
        assert!(eff.b12 && eff.algae_oil && !eff.vit_d, "coverage preserved");
    }

    #[test]
    fn fresh_db_never_grows_supplement_columns_and_skips_the_seed() {
        // A brand-new DB: profiles is created WITHOUT the old supplement columns, so the guarded seed is
        // a no-op (and never errors reading columns that don't exist).
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT, email TEXT, password_hash TEXT, created_at INTEGER, updated_at INTEGER);",
        )
        .unwrap();
        ensure_schema(&conn).unwrap();
        let has_supp: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('profiles') WHERE name = 'supplement_b12'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            has_supp, 0,
            "a fresh profiles table has no supplement columns"
        );
        let seeded: i64 = conn
            .query_row("SELECT COUNT(*) FROM day_supplements", [], |r| r.get(0))
            .unwrap();
        assert_eq!(seeded, 0, "nothing to migrate ⇒ no floor rows");
    }
}
