//! On-device DAL + scale-to-zero changeset sync for the Tauri desktop shell.
//!
//! The `#[procedures]` trait is the typed contract (same surface as @vegify/db on the web). Backed
//! by rusqlite (bundled SQLite, sync); nutrition is ONE recursive CTE ported from
//! packages/db/src/nutrition.ts. IDs are client-generated ULIDs (text) — matching the shared
//! schema — so offline rows never collide on sync; every INSERT mints a ULID (no autoincrement).
//! Mutations are ported from packages/db/src/mutations.ts (with the amounts cascade-cleanup fixes)
//! and run inside `write_capture`, which records a SQLite `session` changeset and persists it as an
//! immutable blob (the S3-transportable sync unit). `sync()` pulls unseen blobs and applies (LWW).

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::session::{Changegroup, ConflictAction, Session};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use ttipc::procedures;
use ulid::Ulid;

fn new_id() -> String {
    Ulid::new().to_string()
}

// ---- UGC visibility (mirrors @vegify/db: schema `visibility` + the access.ts policy) ----

/// The app is public-default sharing: `public` = anyone lists+reads; `unlisted` = readable by
/// direct id/link but not listed; `private` = owner only. Stored on `ingredients` — a recipe IS an
/// ingredient (`as_ingredient_id`), so this one field covers both. Ownership (`user_id`) gates
/// EDITING, not reading.
#[derive(Serialize, Deserialize, Type, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
    Unlisted,
}

impl Visibility {
    /// For binding into SQL (the stored TEXT column).
    fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Unlisted => "unlisted",
        }
    }
    /// Parse the stored column; unknown/legacy values fall back to public (the column default).
    fn from_db(s: &str) -> Self {
        match s {
            "private" => Visibility::Private,
            "unlisted" => Visibility::Unlisted,
            _ => Visibility::Public,
        }
    }
}

// Access policy — the Rust mirror of packages/db/src/access.ts (one rule set, two impls). `isListed`
// (public OR own) is inlined into the list/search SQL (commented at each site); these two cover the
// single-row gates.
/// Edit/delete + the edit-load gate: owner only (both ids present and equal).
fn is_owner(owner: Option<&str>, viewer: Option<&str>) -> bool {
    matches!((owner, viewer), (Some(o), Some(v)) if o == v)
}
/// Readable by direct id/link: anything not private, or your own.
fn can_view(visibility: Visibility, owner: Option<&str>, viewer: Option<&str>) -> bool {
    visibility != Visibility::Private || is_owner(owner, viewer)
}

// ---- read/output wire types ----

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub name: String,
    pub amount_per_100g: f64,
    pub unit: String,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedNutrition {
    pub calories_per_100g: Option<f64>,
    pub readings: Vec<Reading>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Amount {
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub grams: f64,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeItem {
    pub id: String,
    pub name: String,
    pub amount: Amount,
    /// Set when this item is itself a recipe-as-ingredient (e.g. a Biga in a Dough),
    /// so the UI links to that recipe's page instead of a (sparse) ingredient page.
    pub recipe_id: Option<String>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeView {
    pub id: String,
    pub name: String,
    pub subtitle: Option<String>,
    pub directions: Option<String>,
    pub creator: Option<String>,
    pub serving: Option<Amount>,
    pub batch_grams: Option<f64>,
    pub items: Vec<RecipeItem>,
    pub nutrition: AggregatedNutrition,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeCard {
    pub id: String,
    pub name: String,
    pub subtitle: Option<String>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IngredientSearchResult {
    pub id: String,
    pub name: String,
    pub serving_grams: Option<f64>,
    pub calories_per_100g: Option<f64>,
    pub readings: Vec<Reading>,
}

/// RecipeForm edit-mode defaults: per-item nutrition included so each row shows live nutrition.
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeEditItem {
    pub ingredient_id: String,
    pub name: String,
    pub grams: f64,
    pub calories_per_100g: Option<f64>,
    pub readings: Vec<Reading>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeEditData {
    pub id: String,
    pub name: String,
    pub subtitle: Option<String>,
    pub directions: Option<String>,
    pub servings: Option<f64>,
    pub visibility: Visibility,
    pub items: Vec<RecipeEditItem>,
}

/// Ingredient browser card (leaf ingredients — those not backing a recipe).
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IngredientCard {
    pub id: String,
    pub name: String,
    pub calories_per_100g: Option<f64>,
}

/// IngredientForm edit-mode source data (per-100g; the frontend scales to per-serving).
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IngredientEditData {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub price: Option<i32>,
    pub calories_per_100g: Option<f64>,
    pub serving_grams: Option<f64>,
    pub package_grams: Option<f64>,
    pub visibility: Visibility,
    pub nutrients: Vec<Reading>,
}

// ---- write/input wire types (mirror @vegify/db mutation inputs) ----

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IngredientNutrientInput {
    pub name: String,
    pub amount_per_100g: f64,
    pub unit: String,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveIngredientInput {
    pub id: Option<String>,
    pub visibility: Option<Visibility>,
    pub name: String,
    pub description: Option<String>,
    pub price: Option<i32>, // cents
    pub calories_per_100g: Option<f64>,
    pub serving_grams: Option<f64>,
    pub package_grams: Option<f64>,
    pub nutrients: Vec<IngredientNutrientInput>,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeItemInput {
    pub ingredient_id: String,
    pub grams: f64,
    pub unit: Option<String>,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveRecipeInput {
    pub id: Option<String>,
    pub visibility: Option<Visibility>,
    pub name: String,
    pub subtitle: Option<String>,
    pub directions: Option<String>,
    pub serving_grams: Option<f64>,
    pub batch_grams: Option<f64>,
    pub items: Vec<RecipeItemInput>,
}

#[derive(Debug, ttipc::Error)]
pub enum DataError {
    Db(String),
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

fn io_err(e: std::io::Error) -> DataError {
    DataError::Db(e.to_string())
}

// ---- auth (desktop sign-in over HTTPS → token in the OS keychain) ----

/// The current user, mirrored from the web auth response. Stamped on local writes, and upserted
/// into the local `users` table so the foreign key (and the recipe `creator`) resolves on-device.
#[derive(Serialize, Deserialize, Type, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SignInInput {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SignUpInput {
    pub name: String,
    pub email: String,
    pub password: String,
}

/// What the OS keychain holds: the opaque session token + the user profile. The token authorizes
/// server-side logout (and sync, in A3); the cached profile lets `current_user` work offline.
#[derive(Serialize, Deserialize, Clone)]
struct StoredSession {
    token: String,
    user: AuthUser,
}

#[derive(Deserialize)]
struct AuthErrorBody {
    error: String,
}

const KEYCHAIN_SERVICE: &str = "app.vegify.desktop";
const KEYCHAIN_ACCOUNT: &str = "session";

/// Base URL of the web shell that owns the credential store. Override with VEGIFY_AUTH_URL (dev → a
/// local `bun serve-bun.mjs`); default = the deployed CloudFront origin.
fn auth_base_url() -> String {
    std::env::var("VEGIFY_AUTH_URL")
        .unwrap_or_else(|_| "https://EXAMPLEDISTOLD.cloudfront.net".to_string())
}

fn keychain_entry() -> Result<keyring::Entry, DataError> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(|e| DataError::Auth(e.to_string()))
}

fn keychain_load() -> Option<StoredSession> {
    let json = keychain_entry().ok()?.get_password().ok()?;
    serde_json::from_str(&json).ok()
}

fn keychain_store(s: &StoredSession) -> Result<(), DataError> {
    let json = serde_json::to_string(s).map_err(|e| DataError::Auth(e.to_string()))?;
    keychain_entry()?
        .set_password(&json)
        .map_err(|e| DataError::Auth(e.to_string()))
}

fn keychain_clear() {
    if let Ok(e) = keychain_entry() {
        let _ = e.delete_credential();
    }
}

/// POST credentials to the web shell's JSON auth route; on success returns the session to store.
fn post_auth(path: &str, body: serde_json::Value) -> Result<StoredSession, DataError> {
    let url = format!("{}/api/auth/{path}", auth_base_url());
    match ureq::post(&url).send_json(body) {
        Ok(resp) => resp
            .into_json::<StoredSession>()
            .map_err(|e| DataError::Auth(e.to_string())),
        Err(ureq::Error::Status(_, resp)) => {
            let msg = resp
                .into_json::<AuthErrorBody>()
                .map(|e| e.error)
                .unwrap_or_else(|_| "Authentication failed.".to_string());
            Err(DataError::Auth(msg))
        }
        Err(e) => Err(DataError::Auth(format!("Network error: {e}"))),
    }
}

/// Same recursive CTE as packages/db/src/nutrition.ts, normalized to per-100g (text ids).
const CTE: &str = "
WITH RECURSIVE
recipe_total AS (
  SELECT r.id AS recipe_id, r.as_ingredient_id AS as_ingredient_id, SUM(a.grams) AS total_grams
  FROM recipes r
  JOIN ingredient_in_recipe iir ON iir.recipe_id = r.id
  JOIN amounts a ON a.id = iir.amount_id
  GROUP BY r.id
),
expand(ingredient_id, eff_grams, denom, depth) AS (
  SELECT i.id, COALESCE(rt.total_grams, 1.0), COALESCE(rt.total_grams, 1.0), 0
  FROM ingredients i
  LEFT JOIN recipe_total rt ON rt.as_ingredient_id = i.id
  WHERE i.id = ?1
  UNION ALL
  SELECT iir.ingredient_id, e.eff_grams * a.grams / rt.total_grams, e.denom, e.depth + 1
  FROM expand e
  JOIN recipes r ON r.as_ingredient_id = e.ingredient_id
  JOIN recipe_total rt ON rt.recipe_id = r.id
  JOIN ingredient_in_recipe iir ON iir.recipe_id = r.id
  JOIN amounts a ON a.id = iir.amount_id
  WHERE e.depth < 32 AND rt.total_grams > 0
)
SELECT 'cal' AS kind, NULL AS name, NULL AS unit,
       SUM(i.calories_per_100g * e.eff_grams / e.denom) AS per100g
FROM expand e JOIN ingredients i ON i.id = e.ingredient_id
WHERE i.calories_per_100g IS NOT NULL
UNION ALL
SELECT 'nut' AS kind, n.name AS name, inu.unit AS unit,
       SUM(inu.amount_per_100g * e.eff_grams / e.denom) AS per100g
FROM expand e
JOIN ingredient_nutrient inu ON inu.ingredient_id = e.ingredient_id
JOIN nutrients n ON n.id = inu.nutrient_id
GROUP BY n.id
ORDER BY name";

// ---- mutation helpers (ported from mutations.ts; ULIDs minted on insert) ----

fn upsert_amount(
    conn: &Connection,
    id: Option<&str>,
    grams: Option<f64>,
    unit: &str,
) -> Result<Option<String>, DataError> {
    let Some(grams) = grams else { return Ok(id.map(str::to_string)) };
    if let Some(id) = id {
        conn.execute("UPDATE amounts SET grams = ?2, unit = ?3 WHERE id = ?1", params![id, grams, unit])?;
        Ok(Some(id.to_string()))
    } else {
        let id = new_id();
        conn.execute(
            "INSERT INTO amounts(id, grams, unit, amount, preferred) VALUES (?1, ?2, ?3, 1, 'grams')",
            params![id, grams, unit],
        )?;
        Ok(Some(id))
    }
}

fn delete_amounts(conn: &Connection, ids: &[Option<String>]) -> Result<(), DataError> {
    for id in ids.iter().flatten() {
        conn.execute("DELETE FROM amounts WHERE id = ?1", [id])?;
    }
    Ok(())
}

fn find_or_create_nutrient(conn: &Connection, name: &str) -> Result<String, DataError> {
    if let Some(id) =
        conn.query_row("SELECT id FROM nutrients WHERE name = ?1", [name], |r| r.get::<_, String>(0)).optional()?
    {
        return Ok(id);
    }
    let id = new_id();
    conn.execute("INSERT INTO nutrients(id, name) VALUES (?1, ?2)", params![id, name])?;
    Ok(id)
}

fn do_save_ingredient(
    conn: &Connection,
    input: &SaveIngredientInput,
    user_id: Option<&str>,
) -> Result<String, DataError> {
    let visibility = input.visibility.unwrap_or(Visibility::Public).as_str();
    let ingredient_id: String = if let Some(id) = &input.id {
        let existing: Option<(Option<String>, Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT serving_size_id, batch_size_id, user_id FROM ingredients WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        // Owner-gated edit (only when the row exists; a missing id no-ops, matching the web).
        if let Some((_, _, owner)) = &existing {
            if !is_owner(owner.as_deref(), user_id) {
                return Err(DataError::Db("You can only edit your own ingredients.".into()));
            }
        }
        let (serving, batch) = existing.map(|(s, b, _)| (s, b)).unwrap_or((None, None));
        let serving_size_id = upsert_amount(conn, serving.as_deref(), input.serving_grams, "serving")?;
        let batch_size_id = upsert_amount(conn, batch.as_deref(), input.package_grams, "package")?;
        conn.execute(
            "UPDATE ingredients SET name=?2, description=?3, price=?4, calories_per_100g=?5,
             serving_size_id=?6, batch_size_id=?7, visibility=?8 WHERE id=?1",
            params![
                id,
                input.name,
                input.description.as_deref(),
                input.price,
                input.calories_per_100g,
                serving_size_id,
                batch_size_id,
                visibility
            ],
        )?;
        conn.execute("DELETE FROM ingredient_nutrient WHERE ingredient_id = ?1", [id])?;
        id.clone()
    } else {
        let serving_size_id = upsert_amount(conn, None, input.serving_grams, "serving")?;
        let batch_size_id = upsert_amount(conn, None, input.package_grams, "package")?;
        let id = new_id();
        conn.execute(
            "INSERT INTO ingredients(id, user_id, visibility, name, description, is_vegan, price,
             calories_per_100g, serving_size_id, batch_size_id) VALUES (?1, ?8, ?9, ?2, ?3, 1, ?4, ?5, ?6, ?7)",
            params![
                id,
                input.name,
                input.description.as_deref(),
                input.price,
                input.calories_per_100g,
                serving_size_id,
                batch_size_id,
                user_id,
                visibility
            ],
        )?;
        id
    };

    let mut seen = HashSet::new();
    for n in &input.nutrients {
        let name = n.name.trim();
        if name.is_empty() {
            continue;
        }
        let nutrient_id = find_or_create_nutrient(conn, name)?;
        if !seen.insert(nutrient_id.clone()) {
            continue;
        }
        let unit = if n.unit.is_empty() { "g" } else { n.unit.as_str() };
        conn.execute(
            "INSERT INTO ingredient_nutrient(id, ingredient_id, nutrient_id, amount_per_100g, unit)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![new_id(), ingredient_id, nutrient_id, n.amount_per_100g, unit],
        )?;
    }
    Ok(ingredient_id)
}

fn do_delete_ingredient(conn: &Connection, id: &str, user_id: Option<&str>) -> Result<(), DataError> {
    let existing: Option<(Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT serving_size_id, batch_size_id, user_id FROM ingredients WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((serving, batch, owner)) = existing else { return Ok(()) };
    if !is_owner(owner.as_deref(), user_id) {
        return Err(DataError::Db("You can only delete your own ingredients.".into()));
    }
    conn.execute("DELETE FROM ingredients WHERE id = ?1", [id])?;
    delete_amounts(conn, &[serving, batch])
}

fn do_save_recipe(
    conn: &Connection,
    input: &SaveRecipeInput,
    user_id: Option<&str>,
) -> Result<String, DataError> {
    let visibility = input.visibility.unwrap_or(Visibility::Public).as_str();
    let recipe_id: String = if let Some(id) = &input.id {
        let (as_ing_id, serving, batch, owner): (String, Option<String>, Option<String>, Option<String>) =
            conn.query_row(
                "SELECT r.as_ingredient_id, i.serving_size_id, i.batch_size_id, i.user_id
                 FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id WHERE r.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        // The recipe's owner is its as-ingredient's owner; only the owner may edit.
        if !is_owner(owner.as_deref(), user_id) {
            return Err(DataError::Db("You can only edit your own recipes.".into()));
        }
        let serving_size_id = upsert_amount(conn, serving.as_deref(), input.serving_grams, "serving")?;
        let batch_size_id = upsert_amount(conn, batch.as_deref(), input.batch_grams, "batch")?;
        conn.execute(
            "UPDATE ingredients SET name=?2, visibility=?3, serving_size_id=?4, batch_size_id=?5 WHERE id=?1",
            params![as_ing_id, input.name, visibility, serving_size_id, batch_size_id],
        )?;
        conn.execute(
            "UPDATE recipes SET subtitle=?2, directions=?3 WHERE id=?1",
            params![id, input.subtitle.as_deref(), input.directions.as_deref()],
        )?;
        let prev: Vec<Option<String>> = {
            let mut stmt = conn.prepare("SELECT amount_id FROM ingredient_in_recipe WHERE recipe_id = ?1")?;
            let v = stmt
                .query_map([id], |r| r.get::<_, Option<String>>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        conn.execute("DELETE FROM ingredient_in_recipe WHERE recipe_id = ?1", [id])?;
        delete_amounts(conn, &prev)?;
        id.clone()
    } else {
        let serving_size_id = upsert_amount(conn, None, input.serving_grams, "serving")?;
        let batch_size_id = upsert_amount(conn, None, input.batch_grams, "batch")?;
        let as_ing_id = new_id();
        conn.execute(
            "INSERT INTO ingredients(id, user_id, visibility, name, is_vegan, serving_size_id, batch_size_id)
             VALUES (?1, ?5, ?6, ?2, 1, ?3, ?4)",
            params![as_ing_id, input.name, serving_size_id, batch_size_id, user_id, visibility],
        )?;
        let rid = new_id();
        conn.execute(
            "INSERT INTO recipes(id, as_ingredient_id, subtitle, directions) VALUES (?1, ?2, ?3, ?4)",
            params![rid, as_ing_id, input.subtitle.as_deref(), input.directions.as_deref()],
        )?;
        rid
    };

    let mut order = 0i64;
    for item in &input.items {
        if item.ingredient_id.is_empty() || item.grams == 0.0 {
            continue;
        }
        let unit = item.unit.as_deref().unwrap_or("g");
        let amount_id = new_id();
        conn.execute(
            "INSERT INTO amounts(id, grams, amount, unit, preferred) VALUES (?1, ?2, ?2, ?3, 'grams')",
            params![amount_id, item.grams, unit],
        )?;
        conn.execute(
            "INSERT INTO ingredient_in_recipe(id, \"order\", recipe_id, ingredient_id, amount_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![new_id(), order, recipe_id, item.ingredient_id, amount_id],
        )?;
        order += 1;
    }
    Ok(recipe_id)
}

fn do_delete_recipe(conn: &Connection, id: &str, user_id: Option<&str>) -> Result<(), DataError> {
    let as_ing: Option<(String, Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT r.as_ingredient_id, i.serving_size_id, i.batch_size_id, i.user_id
             FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id WHERE r.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((as_ing_id, serving, batch, owner)) = as_ing else { return Ok(()) };
    if !is_owner(owner.as_deref(), user_id) {
        return Err(DataError::Db("You can only delete your own recipes.".into()));
    }

    let item_amounts: Vec<Option<String>> = {
        let mut stmt = conn.prepare("SELECT amount_id FROM ingredient_in_recipe WHERE recipe_id = ?1")?;
        let v = stmt
            .query_map([id], |r| r.get::<_, Option<String>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    conn.execute("DELETE FROM recipes WHERE id = ?1", [id])?; // cascades ingredient_in_recipe
    delete_amounts(conn, &item_amounts)?;

    let still_used: Option<String> = conn
        .query_row("SELECT id FROM ingredient_in_recipe WHERE ingredient_id = ?1 LIMIT 1", [&as_ing_id], |r| {
            r.get(0)
        })
        .optional()?;
    if still_used.is_none() {
        conn.execute("DELETE FROM ingredients WHERE id = ?1", [&as_ing_id])?; // cascades ingredient_nutrient
        delete_amounts(conn, &[serving, batch])?;
    }
    Ok(())
}

/// Where changeset blobs live. A local dir today (LocalBlobStore); an S3 bucket in production
/// (S3BlobStore) — same trait, so the sync/compact logic is storage-agnostic. Blob id = ULID.
pub trait BlobStore: Send + Sync {
    fn put(&self, id: &str, bytes: &[u8]) -> Result<(), DataError>;
    /// Blob ids present, sorted (ULID = chronological = correct apply/merge order).
    fn list(&self) -> Result<Vec<String>, DataError>;
    fn get(&self, id: &str) -> Result<Vec<u8>, DataError>;
    fn delete(&self, id: &str) -> Result<(), DataError>;
}

/// Filesystem blob store: `<dir>/<ulid>.cs`. The dev/offline default and the S3 stand-in.
pub struct LocalBlobStore {
    dir: PathBuf,
}

impl LocalBlobStore {
    pub fn new(dir: &str) -> Result<Self, DataError> {
        let dir = PathBuf::from(dir);
        fs::create_dir_all(&dir).map_err(io_err)?;
        Ok(Self { dir })
    }
    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.cs"))
    }
}

impl BlobStore for LocalBlobStore {
    fn put(&self, id: &str, bytes: &[u8]) -> Result<(), DataError> {
        fs::write(self.path(id), bytes).map_err(io_err)
    }
    fn list(&self) -> Result<Vec<String>, DataError> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.dir).map_err(io_err)? {
            let path = entry.map_err(io_err)?.path();
            if path.extension().and_then(|s| s.to_str()) == Some("cs") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    ids.push(stem.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }
    fn get(&self, id: &str) -> Result<Vec<u8>, DataError> {
        fs::read(self.path(id)).map_err(io_err)
    }
    fn delete(&self, id: &str) -> Result<(), DataError> {
        fs::remove_file(self.path(id)).map_err(io_err)
    }
}

/// S3 blob store — changeset blobs as objects `<prefix><ulid>.cs` in a bucket. Self-hostable: works
/// against any S3-compatible endpoint (real AWS S3, or MinIO for local dev — path-style addressing).
/// Sync (blocking) client, matching the sync DAL. THIS is the production sync transport, scale-to-zero.
pub struct S3BlobStore {
    bucket: Box<s3::Bucket>,
}

impl S3BlobStore {
    /// `endpoint` empty → real AWS (region-based); non-empty → S3-compatible (MinIO), path-style.
    pub fn new(
        bucket: &str,
        region: &str,
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self, DataError> {
        let s3err = |e: s3::error::S3Error| DataError::Db(e.to_string());
        let region = if endpoint.is_empty() {
            region.parse().map_err(|_| DataError::Db(format!("bad region {region}")))?
        } else {
            s3::Region::Custom { region: region.to_string(), endpoint: endpoint.to_string() }
        };
        let creds = s3::creds::Credentials::new(Some(access_key), Some(secret_key), None, None, None)
            .map_err(|e| DataError::Db(e.to_string()))?;
        let mut b = s3::Bucket::new(bucket, region, creds).map_err(s3err)?;
        if !endpoint.is_empty() {
            b = b.with_path_style();
        }
        Ok(Self { bucket: b })
    }
    fn key(id: &str) -> String {
        format!("{id}.cs")
    }
}

impl BlobStore for S3BlobStore {
    fn put(&self, id: &str, bytes: &[u8]) -> Result<(), DataError> {
        self.bucket
            .put_object_blocking(Self::key(id), bytes)
            .map_err(|e| DataError::Db(e.to_string()))?;
        Ok(())
    }
    fn list(&self) -> Result<Vec<String>, DataError> {
        let pages = self
            .bucket
            .list_blocking(String::new(), None)
            .map_err(|e| DataError::Db(e.to_string()))?;
        let mut ids = Vec::new();
        for page in pages {
            for obj in page.contents {
                if let Some(stem) = obj.key.strip_suffix(".cs") {
                    ids.push(stem.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }
    fn get(&self, id: &str) -> Result<Vec<u8>, DataError> {
        let resp = self
            .bucket
            .get_object_blocking(Self::key(id))
            .map_err(|e| DataError::Db(e.to_string()))?;
        Ok(resp.bytes().to_vec())
    }
    fn delete(&self, id: &str) -> Result<(), DataError> {
        self.bucket
            .delete_object_blocking(Self::key(id))
            .map_err(|e| DataError::Db(e.to_string()))?;
        Ok(())
    }
}

pub struct Db {
    conn: Mutex<Connection>,
    blobs: Box<dyn BlobStore>,
    auth: Mutex<Option<StoredSession>>,
}

impl Db {
    pub fn open(db_path: &str, blob_dir: &str) -> Result<Self, DataError> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")
            .ok();
        conn.execute_batch("CREATE TABLE IF NOT EXISTS _applied_changesets(id TEXT PRIMARY KEY);")?;
        Ok(Self {
            conn: Mutex::new(conn),
            blobs: Box::new(LocalBlobStore::new(blob_dir)?),
            auth: Mutex::new(keychain_load()),
        })
    }

    /// Open with an explicit blob store (e.g. an S3BlobStore) instead of the local-dir default.
    pub fn open_with(db_path: &str, blobs: Box<dyn BlobStore>) -> Result<Self, DataError> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")
            .ok();
        conn.execute_batch("CREATE TABLE IF NOT EXISTS _applied_changesets(id TEXT PRIMARY KEY);")?;
        Ok(Self { conn: Mutex::new(conn), blobs, auth: Mutex::new(keychain_load()) })
    }

    fn write_capture<T>(
        &self,
        write: impl FnOnce(&Connection) -> Result<T, DataError>,
    ) -> Result<T, DataError> {
        let conn = self.conn.lock().unwrap();
        let mut bytes = Vec::new();
        let value = {
            let mut session = Session::new(&conn)?;
            session.attach(None)?;
            let v = write(&conn)?;
            session.changeset_strm(&mut bytes)?;
            v
        };
        if !bytes.is_empty() {
            let id = new_id();
            self.blobs.put(&id, &bytes)?;
            conn.execute("INSERT OR IGNORE INTO _applied_changesets(id) VALUES (?1)", [&id])?;
        }
        Ok(value)
    }

    fn current_uid(&self) -> Option<String> {
        self.auth.lock().unwrap().as_ref().map(|s| s.user.id.clone())
    }

    /// Upsert the signed-in user into the local `users` table so write-time foreign keys (and the
    /// recipe `creator`) resolve on-device. Runs outside `write_capture` — identity isn't synced
    /// content. Idempotent.
    fn ensure_user_local(&self, user: &AuthUser) -> Result<(), DataError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users(id, name, email) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, email = excluded.email",
            params![user.id, user.name, user.email],
        )?;
        Ok(())
    }
}

fn aggregate_per100g(conn: &Connection, ingredient_id: &str) -> Result<AggregatedNutrition, DataError> {
    let mut stmt = conn.prepare(CTE)?;
    let rows = stmt.query_map([ingredient_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<f64>>(3)?,
        ))
    })?;
    let mut calories_per_100g = None;
    let mut readings = Vec::new();
    for r in rows {
        let (kind, name, unit, per100g) = r?;
        match kind.as_str() {
            "cal" => calories_per_100g = per100g,
            "nut" => {
                if let (Some(name), Some(unit), Some(v)) = (name, unit, per100g) {
                    readings.push(Reading { name, amount_per_100g: v, unit });
                }
            }
            _ => {}
        }
    }
    Ok(AggregatedNutrition { calories_per_100g, readings })
}

/// Load an ingredient's edit-shape data + its owner id (for the visibility gate). Shared by the
/// `ingredient` detail read (canView) and `ingredient_for_edit` (isOwner) — the desktop reuses one
/// loader where the web splits it into two server fns. The detail VM simply ignores the extra fields.
fn load_ingredient_edit(
    conn: &Connection,
    id: &str,
) -> Result<Option<(IngredientEditData, Option<String>)>, DataError> {
    let meta = conn
        .query_row(
            "SELECT i.name, i.description, i.price, i.calories_per_100g, sa.grams, ba.grams,
                    i.visibility, i.user_id
             FROM ingredients i
             LEFT JOIN amounts sa ON sa.id = i.serving_size_id
             LEFT JOIN amounts ba ON ba.id = i.batch_size_id
             WHERE i.id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, Option<f64>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((name, description, price, calories_per_100g, serving_grams, package_grams, visibility, owner)) =
        meta
    else {
        return Ok(None);
    };
    let nutrients: Vec<Reading> = {
        let mut stmt = conn.prepare(
            "SELECT n.name, inu.amount_per_100g, inu.unit
             FROM ingredient_nutrient inu
             JOIN nutrients n ON n.id = inu.nutrient_id
             WHERE inu.ingredient_id = ?1 ORDER BY n.name",
        )?;
        let v = stmt
            .query_map([id], |r| {
                Ok(Reading { name: r.get(0)?, amount_per_100g: r.get(1)?, unit: r.get(2)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    Ok(Some((
        IngredientEditData {
            id: id.to_string(),
            name,
            description,
            price: price.map(|p| p as i32),
            calories_per_100g,
            serving_grams,
            package_grams,
            visibility: Visibility::from_db(&visibility),
            nutrients,
        },
        owner,
    )))
}

#[procedures]
pub trait VegifyData {
    fn list_recipes(&self) -> Result<Vec<RecipeCard>, DataError>;
    fn recipe(&self, id: String) -> Result<Option<RecipeView>, DataError>;
    fn recipe_for_edit(&self, id: String) -> Result<Option<RecipeEditData>, DataError>;
    fn list_ingredients(&self) -> Result<Vec<IngredientCard>, DataError>;
    fn ingredient(&self, id: String) -> Result<Option<IngredientEditData>, DataError>;
    fn ingredient_for_edit(&self, id: String) -> Result<Option<IngredientEditData>, DataError>;
    fn search_ingredients(&self, query: String) -> Result<Vec<IngredientSearchResult>, DataError>;
    fn save_ingredient(&self, input: SaveIngredientInput) -> Result<String, DataError>;
    fn delete_ingredient(&self, id: String) -> Result<(), DataError>;
    fn save_recipe(&self, input: SaveRecipeInput) -> Result<String, DataError>;
    fn delete_recipe(&self, id: String) -> Result<(), DataError>;
    fn sync(&self) -> Result<(), DataError>;
    fn compact(&self) -> Result<(), DataError>;
    fn current_user(&self) -> Result<Option<AuthUser>, DataError>;
    fn sign_in(&self, input: SignInInput) -> Result<AuthUser, DataError>;
    fn sign_up(&self, input: SignUpInput) -> Result<AuthUser, DataError>;
    fn sign_out(&self) -> Result<(), DataError>;
}

impl VegifyData for Db {
    fn list_recipes(&self) -> Result<Vec<RecipeCard>, DataError> {
        let me = self.current_uid();
        let conn = self.conn.lock().unwrap();
        // isListed: public catalog + your own (any visibility). `user_id = NULL` never matches, so
        // a signed-out viewer (me = NULL) sees only public.
        let mut stmt = conn.prepare(
            "SELECT r.id, i.name, r.subtitle
             FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id
             WHERE i.visibility = 'public' OR i.user_id = ?1
             ORDER BY i.name",
        )?;
        let rows = stmt
            .query_map(params![me], |row| {
                Ok(RecipeCard { id: row.get(0)?, name: row.get(1)?, subtitle: row.get(2)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn search_ingredients(&self, query: String) -> Result<Vec<IngredientSearchResult>, DataError> {
        let me = self.current_uid();
        let conn = self.conn.lock().unwrap();
        let like = format!("%{}%", query.replace('%', "").replace('_', ""));
        let rows: Vec<(String, String, Option<f64>)> = {
            // isListed: public catalog + your own — same scoping as the recipe/ingredient lists.
            let mut stmt = conn.prepare(
                "SELECT i.id, i.name, sa.grams
                 FROM ingredients i
                 LEFT JOIN amounts sa ON sa.id = i.serving_size_id
                 WHERE i.name LIKE ?1 AND (i.visibility = 'public' OR i.user_id = ?2)
                 ORDER BY i.name LIMIT 20",
            )?;
            let v = stmt
                .query_map(params![like, me], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        let mut out = Vec::new();
        for (id, name, serving_grams) in rows {
            let nut = aggregate_per100g(&conn, &id)?;
            out.push(IngredientSearchResult {
                id,
                name,
                serving_grams,
                calories_per_100g: nut.calories_per_100g,
                readings: nut.readings,
            });
        }
        Ok(out)
    }

    fn recipe_for_edit(&self, id: String) -> Result<Option<RecipeEditData>, DataError> {
        let me = self.current_uid();
        let conn = self.conn.lock().unwrap();
        // Owner-only edit-load: `i.user_id = ?2` is the isOwner rule inline (NULL never matches), so
        // a non-owner gets None → the route renders NotFound, mirroring the web's edit-load 404.
        let meta = conn
            .query_row(
                "SELECT i.name, r.subtitle, r.directions, sa.grams, ba.grams, i.visibility
                 FROM recipes r
                 JOIN ingredients i ON i.id = r.as_ingredient_id
                 LEFT JOIN amounts sa ON sa.id = i.serving_size_id
                 LEFT JOIN amounts ba ON ba.id = i.batch_size_id
                 WHERE r.id = ?1 AND i.user_id = ?2",
                params![id, me],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<f64>>(3)?,
                        row.get::<_, Option<f64>>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((name, subtitle, directions, sg, bg, visibility)) = meta else {
            return Ok(None);
        };
        let servings = match (sg, bg) {
            (Some(s), Some(b)) if s > 0.0 => Some(b / s),
            _ => None,
        };

        let rows: Vec<(String, String, f64)> = {
            let mut stmt = conn.prepare(
                "SELECT i.id, i.name, a.grams
                 FROM ingredient_in_recipe iir
                 JOIN ingredients i ON i.id = iir.ingredient_id
                 JOIN amounts a ON a.id = iir.amount_id
                 WHERE iir.recipe_id = ?1 ORDER BY iir.\"order\"",
            )?;
            let v = stmt
                .query_map([&id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<f64>>(2)?.unwrap_or(0.0)))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        let mut items = Vec::new();
        for (ingredient_id, iname, grams) in rows {
            let nut = aggregate_per100g(&conn, &ingredient_id)?;
            items.push(RecipeEditItem {
                ingredient_id,
                name: iname,
                grams,
                calories_per_100g: nut.calories_per_100g,
                readings: nut.readings,
            });
        }
        Ok(Some(RecipeEditData {
            id,
            name,
            subtitle,
            directions,
            servings,
            visibility: Visibility::from_db(&visibility),
            items,
        }))
    }

    fn list_ingredients(&self) -> Result<Vec<IngredientCard>, DataError> {
        let me = self.current_uid();
        let conn = self.conn.lock().unwrap();
        // Standalone ingredients (not a recipe's as-ingredient), scoped isListed: public + your own.
        let mut stmt = conn.prepare(
            "SELECT i.id, i.name, i.calories_per_100g
             FROM ingredients i
             WHERE i.id NOT IN (SELECT as_ingredient_id FROM recipes)
               AND (i.visibility = 'public' OR i.user_id = ?1)
             ORDER BY i.name",
        )?;
        let rows = stmt
            .query_map(params![me], |row| {
                Ok(IngredientCard { id: row.get(0)?, name: row.get(1)?, calories_per_100g: row.get(2)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn ingredient(&self, id: String) -> Result<Option<IngredientEditData>, DataError> {
        let me = self.current_uid();
        let conn = self.conn.lock().unwrap();
        // Detail read: canView — anything not private, or your own.
        match load_ingredient_edit(&conn, &id)? {
            Some((data, owner)) if can_view(data.visibility, owner.as_deref(), me.as_deref()) => {
                Ok(Some(data))
            }
            _ => Ok(None),
        }
    }

    fn ingredient_for_edit(&self, id: String) -> Result<Option<IngredientEditData>, DataError> {
        let me = self.current_uid();
        let conn = self.conn.lock().unwrap();
        // Owner-only edit-load (mirrors the web's edit-load 404 for non-owners).
        match load_ingredient_edit(&conn, &id)? {
            Some((data, owner)) if is_owner(owner.as_deref(), me.as_deref()) => Ok(Some(data)),
            _ => Ok(None),
        }
    }

    fn recipe(&self, id: String) -> Result<Option<RecipeView>, DataError> {
        let me = self.current_uid();
        let conn = self.conn.lock().unwrap();
        // canView: serve unless it's someone else's private recipe (then None → NotFound, as the web 404s).
        let meta = conn
            .query_row(
                "SELECT i.id, i.name, r.subtitle, r.directions, u.name,
                        sa.amount, sa.unit, sa.grams, ba.grams
                 FROM recipes r
                 JOIN ingredients i ON i.id = r.as_ingredient_id
                 LEFT JOIN users u ON u.id = i.user_id
                 LEFT JOIN amounts sa ON sa.id = i.serving_size_id
                 LEFT JOIN amounts ba ON ba.id = i.batch_size_id
                 WHERE r.id = ?1 AND (i.visibility != 'private' OR i.user_id = ?2)",
                params![id, me],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<f64>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<f64>>(7)?,
                        row.get::<_, Option<f64>>(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((as_ing_id, name, subtitle, directions, creator, s_amount, s_unit, s_grams, batch_grams)) = meta
        else {
            return Ok(None);
        };

        let mut istmt = conn.prepare(
            "SELECT i.id, i.name, a.amount, a.unit, a.grams, r2.id AS recipe_id
             FROM ingredient_in_recipe iir
             JOIN ingredients i ON i.id = iir.ingredient_id
             JOIN amounts a ON a.id = iir.amount_id
             LEFT JOIN recipes r2 ON r2.as_ingredient_id = i.id
             WHERE iir.recipe_id = ?1 ORDER BY iir.\"order\"",
        )?;
        let items = istmt
            .query_map([&id], |row| {
                Ok(RecipeItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    amount: Amount {
                        amount: row.get(2)?,
                        unit: row.get(3)?,
                        grams: row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                    },
                    recipe_id: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let nutrition = aggregate_per100g(&conn, &as_ing_id)?;
        let serving = s_grams.map(|grams| Amount { amount: s_amount, unit: s_unit, grams });

        Ok(Some(RecipeView {
            id,
            name,
            subtitle,
            directions,
            creator,
            serving,
            batch_grams,
            items,
            nutrition,
        }))
    }

    fn save_ingredient(&self, input: SaveIngredientInput) -> Result<String, DataError> {
        let uid = self.current_uid();
        self.write_capture(|conn| do_save_ingredient(conn, &input, uid.as_deref()))
    }

    fn delete_ingredient(&self, id: String) -> Result<(), DataError> {
        let uid = self.current_uid();
        self.write_capture(|conn| do_delete_ingredient(conn, &id, uid.as_deref()))
    }

    fn save_recipe(&self, input: SaveRecipeInput) -> Result<String, DataError> {
        let uid = self.current_uid();
        self.write_capture(|conn| do_save_recipe(conn, &input, uid.as_deref()))
    }

    fn delete_recipe(&self, id: String) -> Result<(), DataError> {
        let uid = self.current_uid();
        self.write_capture(|conn| do_delete_recipe(conn, &id, uid.as_deref()))
    }

    fn sync(&self) -> Result<(), DataError> {
        let conn = self.conn.lock().unwrap();
        // A changeset applies table-by-table, so on a fresh replica a child row (e.g. an
        // ingredient_in_recipe) can land before its parent, and SQLite raises a foreign-key
        // conflict at the end of apply. Our conflict handler returns REPLACE, which is only valid
        // for DATA/CONFLICT conflicts — for an FK conflict it makes apply return SQLITE_MISUSE (21).
        // The changeset is internally consistent, so disable FK enforcement across the apply.
        conn.execute_batch("PRAGMA foreign_keys = OFF;").ok();
        let res: Result<(), DataError> = (|| {
            for id in self.blobs.list()? {
                let seen: Option<String> = conn
                    .query_row("SELECT id FROM _applied_changesets WHERE id = ?1", [&id], |r| r.get(0))
                    .optional()?;
                if seen.is_some() {
                    continue;
                }
                let bytes = self.blobs.get(&id)?;
                conn.apply_strm(
                    &mut &bytes[..],
                    None::<fn(&str) -> bool>,
                    |_conflict, _item| ConflictAction::SQLITE_CHANGESET_REPLACE,
                )?;
                conn.execute("INSERT OR IGNORE INTO _applied_changesets(id) VALUES (?1)", [&id])?;
            }
            Ok(())
        })();
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        res
    }

    /// Compaction: squash every changeset blob into ONE via SQLite's changegroup, bounding the
    /// store (and a new device's replay cost). ULID filenames sort chronologically, which is the
    /// correct merge order. A fresh device applies just the combined changeset over the seed;
    /// devices that already applied the originals re-apply the combined one harmlessly (LWW).
    fn compact(&self) -> Result<(), DataError> {
        let conn = self.conn.lock().unwrap();
        let ids = self.blobs.list()?;
        if ids.len() <= 1 {
            return Ok(());
        }
        let mut group = Changegroup::new()?;
        for id in &ids {
            let bytes = self.blobs.get(id)?;
            group.add_stream(&mut &bytes[..])?;
        }
        let mut combined = Vec::new();
        group.output_strm(&mut combined)?;
        let new = new_id();
        self.blobs.put(&new, &combined)?;
        for id in &ids {
            self.blobs.delete(id)?;
        }
        conn.execute("INSERT OR IGNORE INTO _applied_changesets(id) VALUES (?1)", [&new])?;
        Ok(())
    }

    fn current_user(&self) -> Result<Option<AuthUser>, DataError> {
        let user = self.auth.lock().unwrap().as_ref().map(|s| s.user.clone());
        if let Some(u) = &user {
            // Restored from the keychain on launch — make sure the local row exists before any write.
            self.ensure_user_local(u)?;
        }
        Ok(user)
    }

    fn sign_in(&self, input: SignInInput) -> Result<AuthUser, DataError> {
        let session = post_auth(
            "login",
            serde_json::json!({ "email": input.email, "password": input.password }),
        )?;
        let user = session.user.clone();
        self.ensure_user_local(&user)?;
        keychain_store(&session)?;
        *self.auth.lock().unwrap() = Some(session);
        Ok(user)
    }

    fn sign_up(&self, input: SignUpInput) -> Result<AuthUser, DataError> {
        let session = post_auth(
            "signup",
            serde_json::json!({ "name": input.name, "email": input.email, "password": input.password }),
        )?;
        let user = session.user.clone();
        self.ensure_user_local(&user)?;
        keychain_store(&session)?;
        *self.auth.lock().unwrap() = Some(session);
        Ok(user)
    }

    fn sign_out(&self) -> Result<(), DataError> {
        let token = self.auth.lock().unwrap().as_ref().map(|s| s.token.clone());
        if let Some(token) = token {
            // best-effort server-side revoke; ignore network errors so logout always works locally
            let _ = ureq::post(&format!("{}/api/auth/logout", auth_base_url()))
                .set("authorization", &format!("Bearer {token}"))
                .call();
        }
        keychain_clear();
        *self.auth.lock().unwrap() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe_id_by_name(db: &Db, needle: &str) -> String {
        VegifyData::list_recipes(db)
            .expect("list")
            .into_iter()
            .find(|c| c.name.contains(needle))
            .unwrap_or_else(|| panic!("no recipe matching {needle:?}"))
            .id
    }

    /// Stamp the in-memory session as `id` (writes get owned by it; reads scope to it).
    fn set_auth(db: &Db, id: &str, name: &str) {
        *db.auth.lock().unwrap() = Some(StoredSession {
            token: "t".into(),
            user: AuthUser { id: id.into(), name: name.into(), email: format!("{name}@x") },
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
        let blobs = std::env::temp_dir().join("vegify-cte-blobs");
        let db = Db::open(&crate::db_path(), blobs.to_str().unwrap()).expect("open");
        let id = recipe_id_by_name(&db, "Complete Shake");
        let r = VegifyData::recipe(&db, id).expect("query ok").expect("recipe exists");
        let cal100 = r.nutrition.calories_per_100g.expect("has calories");
        let grams = r.serving.as_ref().expect("has serving").grams;
        let per_serving = cal100 * grams / 100.0;
        eprintln!("recipe {:?}: {:.1} cal/serving (ULID {})", r.name, per_serving, r.id);
        assert!((per_serving - 307.5).abs() < 0.5, "got {per_serving:.2}");
    }

    // Full write surface flows through sync: device A SAVES a new recipe (id minted as a ULID) →
    // changeset blob → device B sync() → B sees the new recipe. No server.
    #[test]
    fn save_recipe_syncs_between_devices() {
        let tmp = std::env::temp_dir();
        let a_db = tmp.join("vegify-write-A.db");
        let b_db = tmp.join("vegify-write-B.db");
        let blobs = tmp.join("vegify-write-blobs");
        let _ = fs::remove_file(&a_db);
        let _ = fs::remove_file(&b_db);
        let _ = fs::remove_dir_all(&blobs);
        fs::copy(crate::db_path(), &a_db).expect("seed A");
        fs::copy(crate::db_path(), &b_db).expect("seed B");

        let a = Db::open(a_db.to_str().unwrap(), blobs.to_str().unwrap()).expect("open A");
        let b = Db::open(b_db.to_str().unwrap(), blobs.to_str().unwrap()).expect("open B");

        let input = SaveRecipeInput {
            id: None,
            visibility: None,
            name: "Synced New Recipe".into(),
            subtitle: Some("made offline on A".into()),
            directions: None,
            serving_grams: Some(100.0),
            batch_grams: Some(300.0),
            items: vec![],
        };
        let new_id = VegifyData::save_recipe(&a, input).expect("A saves recipe");
        assert_eq!(new_id.len(), 26, "save_recipe returns a ULID");

        VegifyData::sync(&b).expect("B syncs");
        let names: Vec<String> = VegifyData::list_recipes(&b)
            .expect("B lists")
            .into_iter()
            .map(|c| c.name)
            .collect();
        eprintln!("device B recipes after sync: {names:?}");
        assert!(names.contains(&"Synced New Recipe".to_string()), "B should see A's new recipe");
    }

    // The exact path the write UI drives: search an ingredient → save a recipe USING it as an item
    // → read it back with items + aggregated nutrition. (Covers do_save_recipe's item INSERTs,
    // which the empty-items sync test does not.)
    #[test]
    fn save_recipe_with_items_via_search_flow() {
        let blobs = std::env::temp_dir().join("vegify-uiflow-blobs");
        let db_path = std::env::temp_dir().join("vegify-uiflow.db");
        let _ = fs::remove_dir_all(&blobs);
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap(), blobs.to_str().unwrap()).expect("open");
        sign_in_seed(&db); // own the saved recipe so the owner-gated edit-load returns it

        let hits = VegifyData::search_ingredients(&db, "Flour".into()).expect("search");
        let flour = hits.into_iter().find(|h| h.name.contains("Flour")).expect("a flour exists");

        let id = VegifyData::save_recipe(
            &db,
            SaveRecipeInput {
                id: None,
                visibility: None,
                name: "UI Flow Bread".into(),
                subtitle: None,
                directions: Some("mix".into()),
                serving_grams: Some(100.0),
                batch_grams: Some(500.0),
                items: vec![RecipeItemInput { ingredient_id: flour.id.clone(), grams: 500.0, unit: None }],
            },
        )
        .expect("save recipe with an item");

        let r = VegifyData::recipe(&db, id.clone()).expect("read").expect("exists");
        assert_eq!(r.name, "UI Flow Bread");
        assert_eq!(r.items.len(), 1, "the searched ingredient is attached as an item");
        assert_eq!(r.items[0].name, flour.name);
        assert!(r.nutrition.calories_per_100g.is_some(), "flour has calories → recipe aggregates them");
        eprintln!("UI-flow recipe: {} item(s), {:?} cal/100g", r.items.len(), r.nutrition.calories_per_100g);

        // edit-with-defaults: per-item nutrition + servings (batch/serving = 500/100 = 5).
        let edit = VegifyData::recipe_for_edit(&db, id).expect("edit").expect("exists");
        assert_eq!(edit.servings, Some(5.0));
        assert_eq!(edit.items.len(), 1);
        assert_eq!(edit.items[0].calories_per_100g, Some(364.0));
    }

    // Ingredient browser + edit: a saved ingredient is listed (leaf only, not recipe
    // as-ingredients) and its edit defaults (per-100g + own nutrients) round-trip.
    #[test]
    fn ingredient_browser_and_edit_round_trip() {
        let blobs = std::env::temp_dir().join("vegify-ing-blobs");
        let db_path = std::env::temp_dir().join("vegify-ing.db");
        let _ = fs::remove_dir_all(&blobs);
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap(), blobs.to_str().unwrap()).expect("open");
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
            },
        )
        .expect("save ingredient");

        let cards = VegifyData::list_ingredients(&db).expect("list");
        assert!(cards.iter().any(|c| c.name == "Test Tofu"), "browser shows the new ingredient");
        assert!(
            !cards.iter().any(|c| c.name.contains("Complete Shake")),
            "browser excludes recipe as-ingredients"
        );

        let e = VegifyData::ingredient_for_edit(&db, id).expect("edit").expect("exists");
        assert_eq!(e.name, "Test Tofu");
        assert_eq!(e.serving_grams, Some(85.0));
        assert_eq!(e.calories_per_100g, Some(144.0));
        assert_eq!(e.nutrients.len(), 1);
        assert_eq!(e.nutrients[0].name, "Protein");
        eprintln!("ingredient edit: {} nutrient(s), serving {:?}g", e.nutrients.len(), e.serving_grams);
    }

    // Compaction bounds the blob count without losing data: 3 writes → 3 blobs → compact → 1 blob,
    // and a FRESH device that syncs the single combined changeset still gets all 3 recipes.
    #[test]
    fn compaction_squashes_changesets_losslessly() {
        let tmp = std::env::temp_dir();
        let a_db = tmp.join("vegify-compact-A.db");
        let c_db = tmp.join("vegify-compact-C.db");
        let blobs = tmp.join("vegify-compact-blobs");
        let _ = fs::remove_file(&a_db);
        let _ = fs::remove_file(&c_db);
        let _ = fs::remove_dir_all(&blobs);
        fs::copy(crate::db_path(), &a_db).expect("seed A");
        fs::copy(crate::db_path(), &c_db).expect("seed C");

        let a = Db::open(a_db.to_str().unwrap(), blobs.to_str().unwrap()).expect("open A");
        let made = ["Compact R1", "Compact R2", "Compact R3"];
        for name in made {
            VegifyData::save_recipe(
                &a,
                SaveRecipeInput {
                    id: None,
                    visibility: None,
                    name: name.into(),
                    subtitle: None,
                    directions: None,
                    serving_grams: Some(100.0),
                    batch_grams: Some(200.0),
                    items: vec![],
                },
            )
            .expect("A save");
        }
        let count_cs = || {
            fs::read_dir(&blobs)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("cs"))
                .count()
        };
        let before = count_cs();
        assert!(before >= 3, "expected >=3 changeset blobs, got {before}");
        VegifyData::compact(&a).expect("compact");
        let after = count_cs();
        eprintln!("compaction: {before} blobs -> {after}");
        assert_eq!(after, 1, "compaction should collapse to one combined blob");

        // Fresh device C: seed + sync the single combined changeset → must have all 3 writes.
        let c = Db::open(c_db.to_str().unwrap(), blobs.to_str().unwrap()).expect("open C");
        VegifyData::sync(&c).expect("C sync");
        let names: Vec<String> =
            VegifyData::list_recipes(&c).expect("C list").into_iter().map(|r| r.name).collect();
        for name in made {
            assert!(names.contains(&name.to_string()), "C missing {name:?} after compacted sync");
        }
    }

    // Regression for the SQLITE_MISUSE (21) found during the 2026-06-23 window click-through: a
    // compacted changeset that contains an ingredient_in_recipe row applies child-before-parent on
    // a fresh replica; with foreign_keys ON, the REPLACE conflict handler returns SQLITE_MISUSE.
    // sync() now disables FK across apply. (compaction_squashes_changesets_losslessly used empty
    // items, so it never built an ingredient_in_recipe row and missed this.)
    #[test]
    fn fresh_replica_replays_recipe_with_items() {
        let tmp = std::env::temp_dir();
        let a_db = tmp.join("vegify-fkrepl-A.db");
        let c_db = tmp.join("vegify-fkrepl-C.db");
        let blobs = tmp.join("vegify-fkrepl-blobs");
        let _ = fs::remove_file(&a_db);
        let _ = fs::remove_file(&c_db);
        let _ = fs::remove_dir_all(&blobs);
        fs::copy(crate::db_path(), &a_db).expect("seed A");
        fs::copy(crate::db_path(), &c_db).expect("seed C");

        let a = Db::open(a_db.to_str().unwrap(), blobs.to_str().unwrap()).expect("open A");
        // item references a SEED ingredient → the changeset includes an ingredient_in_recipe row
        let flour = VegifyData::search_ingredients(&a, "Flour".into())
            .expect("search")
            .into_iter()
            .next()
            .expect("seed flour exists");
        VegifyData::save_recipe(
            &a,
            SaveRecipeInput {
                id: None,
                visibility: None,
                name: "FK Replay Loaf".into(),
                subtitle: None,
                directions: None,
                serving_grams: Some(250.0),
                batch_grams: Some(500.0),
                items: vec![RecipeItemInput { ingredient_id: flour.id, grams: 500.0, unit: None }],
            },
        )
        .expect("A save with item");
        VegifyData::compact(&a).expect("compact");

        // Fresh device C: seed + sync the compacted changeset. Without the FK fix this returns
        // SQLITE_MISUSE; with it, C rebuilds the recipe and its replayed ingredient link.
        let c = Db::open(c_db.to_str().unwrap(), blobs.to_str().unwrap()).expect("open C");
        VegifyData::sync(&c).expect("C sync must not return SQLITE_MISUSE");
        let made = VegifyData::list_recipes(&c).expect("C list");
        let loaf = made.iter().find(|r| r.name == "FK Replay Loaf").expect("C missing FK Replay Loaf");
        let view = VegifyData::recipe(&c, loaf.id.clone()).expect("C recipe").expect("exists");
        assert_eq!(view.items.len(), 1, "the ingredient_in_recipe row should have replayed");
    }

    // Real S3 transport (the production sync path), verified against MinIO. #[ignore]'d so the
    // default `cargo test` needs no Docker; run with MinIO up + bucket `vegify-sync`:
    //   cargo test --lib s3_blob_store -- --ignored --nocapture
    #[test]
    #[ignore]
    fn s3_blob_store_syncs_and_compacts() {
        // Env-driven: defaults to MinIO (offline) but runs against a real S3 bucket when SYNC_S3_*
        // are set (SYNC_S3_ENDPOINT set to empty ⇒ real AWS, region-based).
        let mk = || {
            let bucket = std::env::var("SYNC_S3_BUCKET").unwrap_or_else(|_| "vegify-sync".into());
            let region = std::env::var("SYNC_S3_REGION").unwrap_or_else(|_| "us-east-1".into());
            let endpoint =
                std::env::var("SYNC_S3_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:9000".into());
            let access = std::env::var("SYNC_S3_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".into());
            let secret = std::env::var("SYNC_S3_SECRET_KEY").unwrap_or_else(|_| "minioadmin".into());
            S3BlobStore::new(&bucket, &region, &endpoint, &access, &secret).expect("s3 store")
        };
        // deterministic run: clear the bucket first
        let cleaner = mk();
        for id in cleaner.list().expect("list") {
            cleaner.delete(&id).expect("delete");
        }

        let tmp = std::env::temp_dir();
        let a_db = tmp.join("vegify-s3-A.db");
        let b_db = tmp.join("vegify-s3-B.db");
        let _ = fs::remove_file(&a_db);
        let _ = fs::remove_file(&b_db);
        fs::copy(crate::db_path(), &a_db).expect("seed A");
        fs::copy(crate::db_path(), &b_db).expect("seed B");

        let a = Db::open_with(a_db.to_str().unwrap(), Box::new(mk())).expect("open A");
        let b = Db::open_with(b_db.to_str().unwrap(), Box::new(mk())).expect("open B");

        // A recipe WITH an item → the changeset includes an ingredient_in_recipe row, so B's sync
        // exercises the FK-on-fresh-replica path (the SQLITE_MISUSE fix) over real S3 transport.
        let flour = VegifyData::search_ingredients(&a, "Flour".into())
            .expect("search")
            .into_iter()
            .next()
            .expect("seed flour exists");
        let new_id = VegifyData::save_recipe(
            &a,
            SaveRecipeInput {
                id: None,
                visibility: None,
                name: "Synced via S3".into(),
                subtitle: None,
                directions: None,
                serving_grams: Some(100.0),
                batch_grams: Some(200.0),
                items: vec![RecipeItemInput { ingredient_id: flour.id, grams: 300.0, unit: None }],
            },
        )
        .expect("A save -> S3");
        assert_eq!(new_id.len(), 26);

        VegifyData::sync(&b).expect("B sync from S3");
        let made = VegifyData::list_recipes(&b).expect("B list");
        eprintln!("device B (via S3) recipes: {:?}", made.iter().map(|r| &r.name).collect::<Vec<_>>());
        let loaf = made
            .iter()
            .find(|r| r.name == "Synced via S3")
            .expect("B should see A's S3-synced recipe");
        let view = VegifyData::recipe(&b, loaf.id.clone()).expect("B recipe").expect("exists");
        assert_eq!(view.items.len(), 1, "B should have replayed the ingredient_in_recipe row over S3");

        // compaction works over S3 too: a 2nd write, then compact → one object remains.
        VegifyData::save_recipe(
            &a,
            SaveRecipeInput {
                id: None,
                visibility: None,
                name: "S3 R2".into(),
                subtitle: None,
                directions: None,
                serving_grams: Some(50.0),
                batch_grams: None,
                items: vec![],
            },
        )
        .expect("A save 2");
        VegifyData::compact(&a).expect("compact over S3");
        let remaining = mk().list().expect("list after compact").len();
        eprintln!("S3 objects after compact: {remaining}");
        assert_eq!(remaining, 1, "compaction should leave a single S3 object");
    }

    // A signed-in user's writes are stamped with their id. foreign_keys=ON requires that user to
    // exist locally — which ensure_user_local guarantees in the real flow; here we use a seed user.
    #[test]
    fn writes_stamp_signed_in_user() {
        let blobs = std::env::temp_dir().join("vegify-stamp-blobs");
        let db_path = std::env::temp_dir().join("vegify-stamp.db");
        let _ = fs::remove_dir_all(&blobs);
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap(), blobs.to_str().unwrap()).expect("open");

        let uid = sign_in_seed(&db);

        let rid = VegifyData::save_recipe(
            &db,
            SaveRecipeInput {
                id: None,
                visibility: None,
                name: "Owned Recipe".into(),
                subtitle: None,
                directions: None,
                serving_grams: Some(100.0),
                batch_grams: Some(200.0),
                items: vec![],
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
        assert_eq!(owner.as_deref(), Some(uid.as_str()), "recipe is stamped with the signed-in user");
    }

    // Full sign-in path against a running web shell (real network + OS keychain). #[ignore]'d so the
    // default suite needs neither. Run with the web build served on $VEGIFY_AUTH_URL:
    //   VEGIFY_AUTH_URL=http://localhost:39008 \
    //     cargo test --lib sign_in_round_trip -- --ignored --nocapture
    #[test]
    #[ignore]
    fn sign_in_round_trip_against_web() {
        let blobs = std::env::temp_dir().join("vegify-signin-blobs");
        let db_path = std::env::temp_dir().join("vegify-signin.db");
        let _ = fs::remove_dir_all(&blobs);
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap(), blobs.to_str().unwrap()).expect("open");
        let _ = VegifyData::sign_out(&db); // start from a clean keychain slot

        let user = VegifyData::sign_in(
            &db,
            SignInInput { email: "dev@example.com".into(), password: "dev-password".into() },
        )
        .expect("sign in");
        assert_eq!(user.email, "dev@example.com");
        eprintln!("signed in as {} ({})", user.name, user.id);

        // A fresh Db (simulating relaunch) restores the session from the keychain — offline-capable.
        let db2 = Db::open(db_path.to_str().unwrap(), blobs.to_str().unwrap()).expect("reopen");
        let restored =
            VegifyData::current_user(&db2).expect("current").expect("restored from keychain");
        assert_eq!(restored.id, user.id, "session persisted to the keychain across reopen");

        // Wrong password is rejected (the server's message surfaces as DataError::Auth).
        let bad = VegifyData::sign_in(
            &db,
            SignInInput { email: "dev@example.com".into(), password: "nope".into() },
        );
        assert!(bad.is_err(), "wrong password should be rejected");

        VegifyData::sign_out(&db).expect("sign out");
        assert!(VegifyData::current_user(&db).expect("current").is_none(), "cleared after sign out");
    }

    // A4 visibility model (mirrors the web's 2-user test): public content is shared, private is
    // owner-only, and edit/delete are owner-gated. Two accounts over one local DB exercise the
    // per-viewer policy directly (the desktop is single-user-local until A3, but the DAL gates are
    // per-viewer regardless). Covers BOTH entities: a recipe IS an ingredient at the data level, but
    // the recipe read procedures have their own SQL gates, so they're asserted separately below.
    #[test]
    fn visibility_scopes_reads_and_guards_writes() {
        let blobs = std::env::temp_dir().join("vegify-vis-blobs");
        let db_path = std::env::temp_dir().join("vegify-vis.db");
        let _ = fs::remove_dir_all(&blobs);
        let _ = fs::remove_file(&db_path);
        fs::copy(crate::db_path(), &db_path).expect("seed");
        let db = Db::open(db_path.to_str().unwrap(), blobs.to_str().unwrap()).expect("open");

        // Two accounts: john (the seed owner) + Bob (upserted locally, as sign-in would guarantee).
        let john = sign_in_seed(&db);
        let bob = new_id();
        db.ensure_user_local(&AuthUser { id: bob.clone(), name: "Bob".into(), email: "bob@x".into() })
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
                visibility: Some(Visibility::Private),
                name: "John Secret Recipe".into(),
                subtitle: None,
                directions: None,
                serving_grams: Some(100.0),
                batch_grams: Some(200.0),
                items: vec![],
            },
        )
        .expect("john saves recipe");

        // --- As Bob (a different account) ---
        set_auth(&db, &bob, "Bob");

        // Lists + search (isListed): Bob sees John's PUBLIC, not John's private.
        let listed: Vec<String> =
            VegifyData::list_ingredients(&db).expect("bob list").into_iter().map(|c| c.name).collect();
        assert!(listed.contains(&"John Public Sauce".to_string()), "bob sees john's public");
        assert!(!listed.contains(&"John Secret Sauce".to_string()), "bob must NOT see john's private");
        let found: Vec<String> = VegifyData::search_ingredients(&db, "John".into())
            .expect("bob search")
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert!(found.contains(&"John Public Sauce".to_string()), "search returns public");
        assert!(!found.contains(&"John Secret Sauce".to_string()), "search hides private");

        // Detail (canView): public viewable; private 404s (None).
        assert!(VegifyData::ingredient(&db, public.clone()).expect("ok").is_some(), "public viewable");
        assert!(VegifyData::ingredient(&db, secret.clone()).expect("ok").is_none(), "private hidden");

        // Edit-load (isOwner): Bob can't load John's public ingredient for editing.
        assert!(
            VegifyData::ingredient_for_edit(&db, public.clone()).expect("ok").is_none(),
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
            },
        );
        assert!(hijack.is_err(), "bob can't edit john's ingredient");
        assert!(VegifyData::delete_ingredient(&db, public.clone()).is_err(), "bob can't delete john's");

        // Recipe gates (separate SQL from the ingredient path): John's private recipe is hidden,
        // unviewable, un-editable, and un-deletable by Bob.
        let recipe_names: Vec<String> =
            VegifyData::list_recipes(&db).expect("bob recipes").into_iter().map(|c| c.name).collect();
        assert!(!recipe_names.contains(&"John Secret Recipe".to_string()), "private recipe not listed");
        assert!(VegifyData::recipe(&db, secret_recipe.clone()).expect("ok").is_none(), "private recipe 404s");
        assert!(
            VegifyData::recipe_for_edit(&db, secret_recipe.clone()).expect("ok").is_none(),
            "non-owner can't edit-load the recipe"
        );
        assert!(VegifyData::delete_recipe(&db, secret_recipe.clone()).is_err(), "bob can't delete the recipe");

        // --- Back as John (the owner) ---
        set_auth(&db, &john, "John");
        assert!(
            VegifyData::recipe_for_edit(&db, secret_recipe).expect("ok").is_some(),
            "owner can edit-load their recipe"
        );
        let e = VegifyData::ingredient_for_edit(&db, secret.clone())
            .expect("ok")
            .expect("owner edit-load");
        assert_eq!(e.visibility, Visibility::Private, "edit defaults carry the stored visibility");
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
            },
        )
        .expect("owner edits own");
        VegifyData::delete_ingredient(&db, public).expect("owner deletes own");
        eprintln!("visibility: public shared, private hidden from non-owner, edit/delete owner-gated");
    }
}
