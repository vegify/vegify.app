//! Vegify shared DAL — the SQLite-backed content model (recipes / ingredients / nutrition / UGC
//! visibility), consumed by BOTH the desktop shell (apps/desktop/src-tauri) and the standing server
//! (crates/vegify-server). The @vegify/db analogue for the Rust side.
//!
//! Connection-agnostic: every function takes a `&Connection` / `&mut Connection`, so each consumer
//! owns its connection lifecycle — the desktop a single mutexed connection, the server an r2d2 WAL
//! pool. No Tauri, no HTTP, no keychain, no sync engine — those live in the consumers.
//!
//! Reads take an explicit `viewer: Option<&str>` (the signed-in user id, or None) and apply the same
//! visibility scoping on both sides — ONE implementation, so the server and the desktop can never
//! drift. Nutrition is ONE recursive CTE ported from packages/db/src/nutrition.ts (per-100g).
//! Mutations are ported from packages/db/src/mutations.ts (with the amounts cascade-cleanup fixes);
//! ids are client-generated ULIDs (text) so offline rows never collide and stay authoritative.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use ulid::Ulid;

/// A fresh client-side ULID (text id). Minted on insert when no id is supplied; a supplied id is
/// honored, so offline creates and sync re-applies stay authoritative.
pub fn new_id() -> String {
    Ulid::new().to_string()
}

/// The crate error. `Db` = a SQLite/data failure (incl. the owner-guard messages); `Auth` is
/// reserved for credential/identity failures. Deliberately plain — no IPC/HTTP coupling — so each
/// consumer maps it to its own transport error (the desktop's ttipc `DataError`, the server's HTTP
/// status). The ttipc `Error` derive can't live here: it pulls in Tauri, which the server must not.
#[derive(Debug)]
pub enum Error {
    Db(String),
    Auth(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Db(m) => write!(f, "{m}"),
            Error::Auth(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Db(e.to_string())
    }
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
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Unlisted => "unlisted",
        }
    }
    /// Parse the stored column; unknown/legacy values fall back to public (the column default).
    pub fn from_db(s: &str) -> Self {
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
pub fn is_owner(owner: Option<&str>, viewer: Option<&str>) -> bool {
    matches!((owner, viewer), (Some(o), Some(v)) if o == v)
}
/// Readable by direct id/link: anything not private, or your own.
pub fn can_view(visibility: Visibility, owner: Option<&str>, viewer: Option<&str>) -> bool {
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
    /// Whether the current viewer owns this recipe — drives the edit affordance in the UI. The real
    /// guard stays server-side (owner-only edit-load + mutation); false for anonymous + non-owner viewers.
    pub can_edit: bool,
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

/// A public profile: the handle, the display name, and the user's visible recipes. Shared by the
/// server's `/api/content/profile` endpoint and the desktop DAL so both render the identical screen.
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub username: String,
    pub name: String,
    pub recipes: Vec<RecipeCard>,
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
    /// Whether the current viewer owns this ingredient — drives the edit affordance in the UI. Always
    /// true on the owner-only edit-load path; on the detail path it reflects ownership (false for
    /// anonymous + non-owner viewers). The real guard stays server-side.
    pub can_edit: bool,
    pub nutrients: Vec<Reading>,
}

// ---- write/input wire types (mirror @vegify/db mutation inputs) ----

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IngredientNutrientInput {
    pub name: String,
    pub amount_per_100g: f64,
    pub unit: String,
}

#[derive(Serialize, Deserialize, Type)]
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

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeItemInput {
    pub ingredient_id: String,
    pub grams: f64,
    pub unit: Option<String>,
}

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveRecipeInput {
    pub id: Option<String>,
    /// The recipe's as-ingredient id. Threaded so a nested recipe (a Biga consumed by a Dough as an
    /// item) keeps a stable id cross-replica — else the consuming item's FK orphans after a pull.
    /// `None` on a fresh local create (minted); set by the sync pull when mirroring server rows.
    pub as_ingredient_id: Option<String>,
    pub visibility: Option<Visibility>,
    pub name: String,
    pub subtitle: Option<String>,
    pub directions: Option<String>,
    pub serving_grams: Option<f64>,
    pub batch_grams: Option<f64>,
    pub items: Vec<RecipeItemInput>,
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
) -> Result<Option<String>, Error> {
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

fn delete_amounts(conn: &Connection, ids: &[Option<String>]) -> Result<(), Error> {
    for id in ids.iter().flatten() {
        conn.execute("DELETE FROM amounts WHERE id = ?1", [id])?;
    }
    Ok(())
}

fn find_or_create_nutrient(conn: &Connection, name: &str) -> Result<String, Error> {
    if let Some(id) =
        conn.query_row("SELECT id FROM nutrients WHERE name = ?1", [name], |r| r.get::<_, String>(0)).optional()?
    {
        return Ok(id);
    }
    let id = new_id();
    conn.execute("INSERT INTO nutrients(id, name) VALUES (?1, ?2)", params![id, name])?;
    Ok(id)
}

pub fn do_save_ingredient(
    conn: &Connection,
    input: &SaveIngredientInput,
    user_id: Option<&str>,
) -> Result<String, Error> {
    let visibility = input.visibility.unwrap_or(Visibility::Public).as_str();
    // Upsert by id: an existing row updates (owner-gated); a supplied-but-absent id inserts WITH that
    // id (an offline create's client ULID, or a row mirrored from the server by the sync pull); no id
    // mints a fresh ULID and inserts. Look up the row only when an id was supplied.
    let existing: Option<(Option<String>, Option<String>, Option<String>)> = match &input.id {
        Some(id) => conn
            .query_row(
                "SELECT serving_size_id, batch_size_id, user_id FROM ingredients WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?,
        None => None,
    };

    let ingredient_id: String = if let Some((serving, batch, owner)) = existing {
        let id = input.id.as_deref().expect("existing row implies a supplied id");
        if !is_owner(owner.as_deref(), user_id) {
            return Err(Error::Db("You can only edit your own ingredients.".into()));
        }
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
        id.to_string()
    } else {
        let serving_size_id = upsert_amount(conn, None, input.serving_grams, "serving")?;
        let batch_size_id = upsert_amount(conn, None, input.package_grams, "package")?;
        let id = input.id.clone().unwrap_or_else(new_id);
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

pub fn do_delete_ingredient(conn: &Connection, id: &str, user_id: Option<&str>) -> Result<(), Error> {
    let existing: Option<(Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT serving_size_id, batch_size_id, user_id FROM ingredients WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((serving, batch, owner)) = existing else { return Ok(()) };
    if !is_owner(owner.as_deref(), user_id) {
        return Err(Error::Db("You can only delete your own ingredients.".into()));
    }
    conn.execute("DELETE FROM ingredients WHERE id = ?1", [id])?;
    delete_amounts(conn, &[serving, batch])
}

pub fn do_save_recipe(
    conn: &Connection,
    input: &SaveRecipeInput,
    user_id: Option<&str>,
) -> Result<String, Error> {
    let visibility = input.visibility.unwrap_or(Visibility::Public).as_str();
    // Upsert by id (see do_save_ingredient). A supplied-but-absent recipe id inserts WITH that id; the
    // as-ingredient id is threaded too (input.as_ingredient_id) so a nested recipe stays addressable
    // cross-replica (Biga-in-Dough). No id mints both. The owner gate applies only to an existing recipe.
    let existing: Option<(String, Option<String>, Option<String>, Option<String>)> = match &input.id {
        Some(id) => conn
            .query_row(
                "SELECT r.as_ingredient_id, i.serving_size_id, i.batch_size_id, i.user_id
                 FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id WHERE r.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?,
        None => None,
    };

    let recipe_id: String = if let Some((as_ing_id, serving, batch, owner)) = existing {
        let id = input.id.as_deref().expect("existing recipe implies a supplied id");
        // The recipe's owner is its as-ingredient's owner; only the owner may edit.
        if !is_owner(owner.as_deref(), user_id) {
            return Err(Error::Db("You can only edit your own recipes.".into()));
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
        id.to_string()
    } else {
        let serving_size_id = upsert_amount(conn, None, input.serving_grams, "serving")?;
        let batch_size_id = upsert_amount(conn, None, input.batch_grams, "batch")?;
        let as_ing_id = input.as_ingredient_id.clone().unwrap_or_else(new_id);
        conn.execute(
            "INSERT INTO ingredients(id, user_id, visibility, name, is_vegan, serving_size_id, batch_size_id)
             VALUES (?1, ?5, ?6, ?2, 1, ?3, ?4)",
            params![as_ing_id, input.name, serving_size_id, batch_size_id, user_id, visibility],
        )?;
        let rid = input.id.clone().unwrap_or_else(new_id);
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

pub fn do_delete_recipe(conn: &Connection, id: &str, user_id: Option<&str>) -> Result<(), Error> {
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
        return Err(Error::Db("You can only delete your own recipes.".into()));
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

/// Aggregate an ingredient's (or recipe-as-ingredient's) nutrition to per-100g via the recursive CTE.
pub fn aggregate_per100g(conn: &Connection, ingredient_id: &str) -> Result<AggregatedNutrition, Error> {
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
) -> Result<Option<(IngredientEditData, Option<String>)>, Error> {
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
            can_edit: false,
            nutrients,
        },
        owner,
    )))
}

// ---- reads (free fns, viewer-scoped; the desktop trait + the server handlers are thin wrappers) ----

/// Recipe browser cards. isListed: public catalog + your own (any visibility). `user_id = NULL` never
/// matches, so a signed-out viewer (viewer = None) sees only public.
/// Public recipe catalog for `viewer` (public rows + the viewer's own), NEWEST FIRST by id — ids are
/// ULIDs, so id order is creation order. Keyset-paginated for infinite scroll: pass the last card's `id`
/// as `cursor` to get the page after it; `limit` caps the page (None = no limit, i.e. the full list).
/// Sort order for the catalog list reads. Recency sorts key on the id (ids are ULIDs, so id order is
/// creation order); name sorts use a composite (name, id) keyset since names are not unique. Default
/// = Newest (the catalog's first impression).
#[derive(Serialize, Deserialize, Type, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    #[default]
    Newest,
    Oldest,
    NameAsc,
    NameDesc,
}

impl Sort {
    /// The `(keyset predicate, ORDER BY)` SQL for this sort over the given id and name column
    /// expressions. `?2` binds the cursor id and `?3` the cursor name — both NULL on the first page,
    /// so the predicate is vacuously true. The keyset selects the rows AFTER the cursor in sort order.
    fn clauses(self, id: &str, name: &str) -> (String, String) {
        match self {
            Sort::Newest => (format!("(?2 IS NULL OR {id} < ?2)"), format!("{id} DESC")),
            Sort::Oldest => (format!("(?2 IS NULL OR {id} > ?2)"), format!("{id} ASC")),
            Sort::NameAsc => (
                format!("(?2 IS NULL OR {name} > ?3 OR ({name} = ?3 AND {id} > ?2))"),
                format!("{name} ASC, {id} ASC"),
            ),
            Sort::NameDesc => (
                format!("(?2 IS NULL OR {name} < ?3 OR ({name} = ?3 AND {id} < ?2))"),
                format!("{name} DESC, {id} DESC"),
            ),
        }
    }
}

/// One page of a catalog list: the sort, a keyset cursor (the last card's id, plus its name for the
/// name sorts), and a page size. `Default` = newest-first, no cursor, no limit (the whole list).
#[derive(Serialize, Deserialize, Type, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Page {
    pub sort: Sort,
    pub cursor: Option<String>,
    pub cursor_name: Option<String>,
    pub limit: Option<u32>,
}

pub fn list_recipes(
    conn: &Connection,
    viewer: Option<&str>,
    page: &Page,
) -> Result<Vec<RecipeCard>, Error> {
    let (keyset, order) = page.sort.clauses("r.id", "i.name");
    let sql = format!(
        "SELECT r.id, i.name, r.subtitle
         FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id
         WHERE (i.visibility = 'public' OR i.user_id = ?1) AND {keyset}
         ORDER BY {order}
         LIMIT ?4"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(
            params![
                viewer,
                page.cursor.as_deref(),
                page.cursor_name.as_deref(),
                page.limit.map_or(-1, |n| n as i64)
            ],
            |row| Ok(RecipeCard { id: row.get(0)?, name: row.get(1)?, subtitle: row.get(2)? }),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// A user's public profile by handle: the user (if the handle exists) + the recipes visible to this
/// viewer — their public recipes, plus the viewer's own non-public ones when viewing themselves. The
/// recipe scope mirrors `list_recipes`, narrowed to this user. None when no such handle exists.
pub fn get_profile(
    conn: &Connection,
    username: &str,
    viewer: Option<&str>,
) -> Result<Option<Profile>, Error> {
    let user: Option<(String, String, String)> = conn
        .query_row(
            "SELECT id, name, username FROM users WHERE username = ?1",
            [username],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((uid, name, username)) = user else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(
        "SELECT r.id, i.name, r.subtitle
         FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id
         WHERE i.user_id = ?1 AND (i.visibility = 'public' OR i.user_id = ?2)
         ORDER BY i.name",
    )?;
    let recipes = stmt
        .query_map(params![uid, viewer], |row| {
            Ok(RecipeCard { id: row.get(0)?, name: row.get(1)?, subtitle: row.get(2)? })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(Profile { username, name, recipes }))
}

/// Ingredient search (isListed: public + own — same scoping as the lists), with per-100g nutrition.
pub fn search_ingredients(
    conn: &Connection,
    query: String,
    viewer: Option<&str>,
) -> Result<Vec<IngredientSearchResult>, Error> {
    let like = format!("%{}%", query.replace('%', "").replace('_', ""));
    let rows: Vec<(String, String, Option<f64>)> = {
        let mut stmt = conn.prepare(
            "SELECT i.id, i.name, sa.grams
             FROM ingredients i
             LEFT JOIN amounts sa ON sa.id = i.serving_size_id
             WHERE i.name LIKE ?1 AND (i.visibility = 'public' OR i.user_id = ?2)
             ORDER BY i.name LIMIT 20",
        )?;
        let v = stmt
            .query_map(params![like, viewer], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    let mut out = Vec::new();
    for (id, name, serving_grams) in rows {
        let nut = aggregate_per100g(conn, &id)?;
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

/// Recipe edit-load defaults. Owner-only (`i.user_id = ?2` is the isOwner rule inline; NULL never
/// matches), so a non-owner gets None → the route renders NotFound, mirroring the web's edit-load 404.
pub fn recipe_for_edit(
    conn: &Connection,
    id: String,
    viewer: Option<&str>,
) -> Result<Option<RecipeEditData>, Error> {
    let meta = conn
        .query_row(
            "SELECT i.name, r.subtitle, r.directions, sa.grams, ba.grams, i.visibility
             FROM recipes r
             JOIN ingredients i ON i.id = r.as_ingredient_id
             LEFT JOIN amounts sa ON sa.id = i.serving_size_id
             LEFT JOIN amounts ba ON ba.id = i.batch_size_id
             WHERE r.id = ?1 AND i.user_id = ?2",
            params![id, viewer],
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
        let nut = aggregate_per100g(conn, &ingredient_id)?;
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

/// Ingredient browser cards — standalone ingredients (not a recipe's as-ingredient), isListed-scoped.
/// Standalone-ingredient catalog for `viewer` (recipe as-ingredients excluded), NEWEST FIRST by id.
/// Keyset-paginated like [`list_recipes`]: `cursor` = the last card's `id`, `limit` caps the page
/// (None = the full list).
pub fn list_ingredients(
    conn: &Connection,
    viewer: Option<&str>,
    page: &Page,
) -> Result<Vec<IngredientCard>, Error> {
    let (keyset, order) = page.sort.clauses("i.id", "i.name");
    let sql = format!(
        "SELECT i.id, i.name, i.calories_per_100g
         FROM ingredients i
         WHERE i.id NOT IN (SELECT as_ingredient_id FROM recipes)
           AND (i.visibility = 'public' OR i.user_id = ?1) AND {keyset}
         ORDER BY {order}
         LIMIT ?4"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(
            params![
                viewer,
                page.cursor.as_deref(),
                page.cursor_name.as_deref(),
                page.limit.map_or(-1, |n| n as i64)
            ],
            |row| Ok(IngredientCard { id: row.get(0)?, name: row.get(1)?, calories_per_100g: row.get(2)? }),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Ingredient detail (canView — anything not private, or your own).
pub fn ingredient(
    conn: &Connection,
    id: String,
    viewer: Option<&str>,
) -> Result<Option<IngredientEditData>, Error> {
    match load_ingredient_edit(conn, &id)? {
        Some((mut data, owner)) if can_view(data.visibility, owner.as_deref(), viewer) => {
            data.can_edit = is_owner(owner.as_deref(), viewer);
            Ok(Some(data))
        }
        _ => Ok(None),
    }
}

/// Ingredient edit-load (isOwner — owner only; mirrors the web's edit-load 404 for non-owners).
pub fn ingredient_for_edit(
    conn: &Connection,
    id: String,
    viewer: Option<&str>,
) -> Result<Option<IngredientEditData>, Error> {
    match load_ingredient_edit(conn, &id)? {
        Some((mut data, owner)) if is_owner(owner.as_deref(), viewer) => {
            data.can_edit = true;
            Ok(Some(data))
        }
        _ => Ok(None),
    }
}

/// Recipe detail (canView: serve unless it's someone else's private recipe → None, as the web 404s).
pub fn recipe(conn: &Connection, id: String, viewer: Option<&str>) -> Result<Option<RecipeView>, Error> {
    let meta = conn
        .query_row(
            "SELECT i.id, i.name, r.subtitle, r.directions, u.username,
                    sa.amount, sa.unit, sa.grams, ba.grams, i.user_id
             FROM recipes r
             JOIN ingredients i ON i.id = r.as_ingredient_id
             LEFT JOIN users u ON u.id = i.user_id
             LEFT JOIN amounts sa ON sa.id = i.serving_size_id
             LEFT JOIN amounts ba ON ba.id = i.batch_size_id
             WHERE r.id = ?1 AND (i.visibility != 'private' OR i.user_id = ?2)",
            params![id, viewer],
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
                    row.get::<_, Option<String>>(9)?,
                ))
            },
        )
        .optional()?;
    let Some((as_ing_id, name, subtitle, directions, creator, s_amount, s_unit, s_grams, batch_grams, owner)) =
        meta
    else {
        return Ok(None);
    };
    let can_edit = is_owner(owner.as_deref(), viewer);

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

    let nutrition = aggregate_per100g(conn, &as_ing_id)?;
    let serving = s_grams.map(|grams| Amount { amount: s_amount, unit: s_unit, grams });

    Ok(Some(RecipeView {
        id,
        name,
        subtitle,
        directions,
        creator,
        can_edit,
        serving,
        batch_grams,
        items,
        nutrition,
    }))
}
