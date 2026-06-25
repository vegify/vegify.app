//! Vegify standing backend — Axum over SQLite-WAL, serving the SAME `/api/auth/*` + `/api/content/*`
//! contract the clients already speak (so they re-point, not rewrite). Reads/writes go through
//! vegify-core (one shared DAL with the desktop); auth is the Rust port of auth.ts. rusqlite is sync,
//! so every DB touch runs on an r2d2 WAL pool via spawn_blocking (concurrent readers, serialized writer).
//!
//! Run: DATABASE_PATH=<file> PORT=8787 cargo run -p vegify-server

mod auth;
mod content;
mod error;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{bearer_token, User};
use crate::error::AppError;

type Pool = r2d2::Pool<SqliteConnectionManager>;

#[derive(Clone)]
struct AppState {
    pool: Pool,
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

// ---- auth routes (JSON body in, token in the body out; no cookie/CSRF — for native clients) ----

#[derive(Deserialize)]
struct LoginBody {
    email: Option<String>,
    password: Option<String>,
}

async fn login(State(state): State<AppState>, Json(body): Json<LoginBody>) -> Result<Json<Value>, AppError> {
    let (email, password) = match (body.email, body.password) {
        (Some(e), Some(p)) if !e.is_empty() && !p.is_empty() => (e, p),
        _ => return Err(AppError::BadRequest("Email and password are required.".into())),
    };
    let out = db(&state, move |conn| match auth::authenticate(conn, &email, &password)? {
        Some(user) => {
            let token = auth::create_session(conn, &user.id)?;
            Ok(json!({ "token": token, "user": user }))
        }
        None => Err(AppError::InvalidCredentials),
    })
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
struct SignupBody {
    name: Option<String>,
    email: Option<String>,
    password: Option<String>,
}

async fn signup(State(state): State<AppState>, Json(body): Json<SignupBody>) -> Result<Json<Value>, AppError> {
    let name = body.name.unwrap_or_default().trim().to_string();
    let email = body.email.unwrap_or_default().trim().to_string();
    let password = body.password.unwrap_or_default();
    if name.is_empty() || email.is_empty() {
        return Err(AppError::BadRequest("Name and email are required.".into()));
    }
    if password.chars().count() < 8 {
        return Err(AppError::BadRequest("Password must be at least 8 characters.".into()));
    }
    let out = db(&state, move |conn| {
        if auth::email_exists(conn, &email)? {
            return Err(AppError::Conflict("An account with that email already exists.".into()));
        }
        let user = auth::create_user(conn, &name, &email, &password)?;
        let token = auth::create_session(conn, &user.id)?;
        Ok(json!({ "token": token, "user": user }))
    })
    .await?;
    Ok(Json(out))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, AppError> {
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

async fn bootstrap(State(state): State<AppState>, Json(body): Json<BootstrapBody>) -> Result<Json<Value>, AppError> {
    let email = body.email.unwrap_or_default().trim().to_string();
    let password = body.password.unwrap_or_default();
    if email.is_empty() || password.chars().count() < 8 {
        return Err(AppError::BadRequest("Email and an 8+ character password are required.".into()));
    }
    let normalized = email.to_lowercase();
    db(&state, move |conn| auth::set_initial_password(conn, &email, &password)).await?;
    Ok(Json(json!({ "ok": true, "email": normalized })))
}

// ---- content routes (Bearer-authed; userId always stamped server-side from the session) ----

async fn list_recipes(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<vegify_core::RecipeCard>>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::list_recipes(conn, Some(&me.id)).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
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
    Ok(Json(json!({ "id": id })))
}

async fn delete_recipe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let id = q.id.ok_or_else(|| AppError::BadRequest("id is required.".into()))?;
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::do_delete_recipe(conn, &id, Some(&me.id)).map_err(AppError::from)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn list_ingredients(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<vegify_core::IngredientCard>>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::list_ingredients(conn, Some(&me.id)).map_err(AppError::from)
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
    let id = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::do_save_ingredient(conn, &input, Some(&me.id)).map_err(AppError::from)
    })
    .await?;
    Ok(Json(json!({ "id": id })))
}

async fn delete_ingredient(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Value>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let id = q.id.ok_or_else(|| AppError::BadRequest("id is required.".into()))?;
    db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::do_delete_ingredient(conn, &id, Some(&me.id)).map_err(AppError::from)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn recipe_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> Result<Json<Option<vegify_core::RecipeView>>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        match q.id {
            Some(id) => vegify_core::recipe(conn, id, Some(&me.id)).map_err(AppError::from),
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
            Some(id) => vegify_core::recipe_for_edit(conn, id, Some(&me.id)).map_err(AppError::from),
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
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        match q.id {
            Some(id) => vegify_core::ingredient(conn, id, Some(&me.id)).map_err(AppError::from),
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
            Some(id) => vegify_core::ingredient_for_edit(conn, id, Some(&me.id)).map_err(AppError::from),
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
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let q = query.q.unwrap_or_default();
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        vegify_core::search_ingredients(conn, q, Some(&me.id)).map_err(AppError::from)
    })
    .await?;
    Ok(Json(out))
}

async fn pull(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<content::PullPayload>, AppError> {
    let token = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    let out = db(&state, move |conn| {
        let me = require_user(conn, &token)?;
        content::pull(conn, Some(&me.id))
    })
    .await?;
    Ok(Json(out))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "vegify.db".to_string());
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8787);

    // WAL on EBS-class storage: concurrent readers + a serialized writer (no NFS, no reserved
    // concurrency). busy_timeout lets a waiting writer retry instead of erroring under contention.
    let manager = SqliteConnectionManager::file(&db_path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )
    });
    let pool = r2d2::Pool::builder().max_size(8).build(manager)?;
    let state = AppState { pool };

    let app = Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/signup", post(signup))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/bootstrap", post(bootstrap))
        .route(
            "/api/content/recipes",
            get(list_recipes).post(save_recipe).delete(delete_recipe),
        )
        .route(
            "/api/content/ingredients",
            get(list_ingredients).post(save_ingredient).delete(delete_ingredient),
        )
        .route("/api/content/recipe-detail", get(recipe_detail))
        .route("/api/content/recipe-edit", get(recipe_edit))
        .route("/api/content/ingredient-detail", get(ingredient_detail))
        .route("/api/content/ingredient-edit", get(ingredient_edit))
        .route("/api/content/search", get(search))
        .route("/api/content/pull", get(pull))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("vegify-server listening on http://{addr} (db: {db_path})");
    axum::serve(listener, app).await?;
    Ok(())
}
