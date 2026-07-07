//! Vegify shared DAL — the SQLite-backed content model (recipes / ingredients / nutrition / UGC
//! visibility), consumed by BOTH the desktop shell (apps/desktop/src-tauri) and the standing server
//! (services/api). The @vegify/db analogue for the Rust side.
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
    /// SQLite failure, stringified for transport.
    Db(String),
    /// Authentication/authorization failure, stringified for transport.
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
    /// Listed and readable by everyone.
    Public,
    /// Readable only by the owner.
    Private,
    /// Readable via direct link; never listed or searched.
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
/// One nutrient measurement, normalized per 100 g.
pub struct Reading {
    /// Nutrient name (e.g. "Iron").
    pub name: String,
    /// Quantity per 100 g of the food.
    pub amount_per_100g: f64,
    /// Display unit for the quantity (g, mg, µg).
    pub unit: String,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// A recipe's nutrition rolled up from its items, per 100 g.
pub struct AggregatedNutrition {
    /// Calories per 100 g; None when no item carries calorie data.
    pub calories_per_100g: Option<f64>,
    /// Per-100 g readings summed across items, keyed by nutrient name.
    pub readings: Vec<Reading>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// A quantity as entered plus its canonical mass: `grams` is the ground
/// truth all math runs on; `amount`/`unit` preserve how the user wrote it.
pub struct Amount {
    /// Quantity in `unit`, as entered; None when only the mass is known.
    pub amount: Option<f64>,
    /// Display unit (cup, tbsp, g); None when only the mass is known.
    pub unit: Option<String>,
    /// Canonical mass in grams.
    pub grams: f64,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// One recipe line as read: the ingredient, its display name, its amount.
pub struct RecipeItem {
    /// The underlying ingredient's id.
    pub id: String,
    /// Ingredient display name at read time.
    pub name: String,
    /// The line's quantity (display form + canonical grams).
    pub amount: Amount,
    /// Set when this item is itself a recipe-as-ingredient (e.g. a Biga in a Dough),
    /// so the UI links to that recipe's page instead of a (sparse) ingredient page.
    pub recipe_id: Option<String>,
    /// True only in the DELETER's own recipes: the ingredient is soft-deleted (tombstoned) AND its
    /// owner owns this recipe. Other users' recipes render the same ingredient normally — a soft
    /// delete disowns from the catalog without breaking anyone's recipe. Drives the greyed row +
    /// the "restore?" affordance.
    pub deleted: bool,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// Full recipe detail as rendered by the recipe page.
pub struct RecipeView {
    /// Recipe id.
    pub id: String,
    /// Recipe title.
    pub name: String,
    /// Optional subtitle under the title.
    pub subtitle: Option<String>,
    /// Free-text directions (markdown-ish plain text); None = none written.
    pub directions: Option<String>,
    /// The owner's username — also the first segment of the canonical URL `/<creator>/<slug>`.
    pub creator: Option<String>,
    /// This recipe's current slug (the `/<creator>/<slug>` segment). None only pre-backfill.
    pub slug: Option<String>,
    /// Whether the current viewer owns this recipe — drives the edit affordance in the UI. The real
    /// guard stays server-side (owner-only edit-load + mutation); false for anonymous + non-owner viewers.
    pub can_edit: bool,
    /// Serving size, when declared (drives per-serving nutrition).
    pub serving: Option<Amount>,
    /// Total batch mass in grams, when declared — the denominator that
    /// turns summed item nutrition into per-100 g.
    pub batch_grams: Option<f64>,
    /// The recipe's lines, in order.
    pub items: Vec<RecipeItem>,
    /// Nutrition rolled up from the items, per 100 g.
    pub nutrition: AggregatedNutrition,
    /// Media key of the hero photo — see [`RecipeCard::photo_key`].
    pub photo_key: Option<String>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// Recipe list/browse card — the light projection for grids.
pub struct RecipeCard {
    /// Recipe id.
    pub id: String,
    /// Recipe title.
    pub name: String,
    /// Optional subtitle, shown under the title on cards.
    pub subtitle: Option<String>,
    /// Owner handle + slug for the canonical `/<username>/<slug>` link. Optional (pre-backfill /
    /// ownerless rows); the UI falls back to `/recipes/<id>` when either is missing.
    pub username: Option<String>,
    /// The recipe's slug half of the canonical link (see `username`).
    pub slug: Option<String>,
    /// Media key of the hero photo (attached to the recipe's as-ingredient); clients compose the
    /// URL as `<api base>/<key>`. None = no photo yet (cards render the placeholder tile).
    pub photo_key: Option<String>,
}

/// A public profile: the handle, the display name, and the user's visible recipes. Shared by the
/// server's `/api/content/profile` endpoint and the desktop DAL so both render the identical screen.
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// The profile's handle (URL segment).
    pub username: String,
    /// Display name.
    pub name: String,
    /// The user's recipes visible to the viewer, as cards.
    pub recipes: Vec<RecipeCard>,
    /// The user's LEAF ingredients (created or imported by them), visible to the viewer and not
    /// tombstoned — browsable under `/<username>/ingredients/<slug>`.
    pub ingredients: Vec<IngredientCard>,
    /// Media key of the profile avatar; clients compose `<api base>/<key>`.
    pub avatar_key: Option<String>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// One ingredient hit in the recipe composer's search box.
pub struct IngredientSearchResult {
    /// Ingredient id.
    pub id: String,
    /// Ingredient name.
    pub name: String,
    /// Serving size in grams, when the ingredient declares one (the composer's
    /// default line quantity).
    pub serving_grams: Option<f64>,
    /// Calories per 100 g, when known.
    pub calories_per_100g: Option<f64>,
    /// Per-100 g nutrient readings, for the composer's live nutrition preview.
    pub readings: Vec<Reading>,
}

/// RecipeForm edit-mode defaults: per-item nutrition included so each row shows live nutrition.
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeEditItem {
    /// The referenced ingredient's id.
    pub ingredient_id: String,
    /// Ingredient display name.
    pub name: String,
    /// Line quantity in grams (canonical).
    pub grams: f64,
    /// Calories per 100 g, when known — the edit screen's live math.
    pub calories_per_100g: Option<f64>,
    /// Per-100 g readings for the edit screen's live nutrition roll-up.
    pub readings: Vec<Reading>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// RecipeForm edit-mode source data (per-100 g; the frontend scales).
pub struct RecipeEditData {
    /// Recipe id.
    pub id: String,
    /// Recipe title.
    pub name: String,
    /// Optional subtitle.
    pub subtitle: Option<String>,
    /// Free-text directions; None = none written.
    pub directions: Option<String>,
    /// Declared servings per batch, when set.
    pub servings: Option<f64>,
    /// Current visibility.
    pub visibility: Visibility,
    /// The recipe's lines in edit form.
    pub items: Vec<RecipeEditItem>,
}

/// Ingredient browser card (leaf ingredients — those not backing a recipe).
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IngredientCard {
    /// Ingredient id.
    pub id: String,
    /// Ingredient name.
    pub name: String,
    /// Calories per 100 g, when known (list-card badge).
    pub calories_per_100g: Option<f64>,
    /// Slug for the canonical link; fall back to `/ingredients/<id>`.
    pub slug: Option<String>,
    /// Owner handle: an OWNED ingredient is canonical at `/<username>/ingredients/<slug>` (browsable
    /// under its creator); None = the communal catalog, canonical at `/ingredients/<slug>`.
    pub username: Option<String>,
}

/// IngredientForm edit-mode source data (per-100g; the frontend scales to per-serving).
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IngredientEditData {
    /// Ingredient id.
    pub id: String,
    /// Ingredient name.
    pub name: String,
    /// Optional description shown on the detail page.
    pub description: Option<String>,
    /// Price in cents, when tracked.
    pub price: Option<i32>,
    /// Calories per 100 g, when known.
    pub calories_per_100g: Option<f64>,
    /// Serving size in grams, when declared.
    pub serving_grams: Option<f64>,
    /// Package mass in grams, when declared (price-per-100 g math).
    pub package_grams: Option<f64>,
    /// Current visibility.
    pub visibility: Visibility,
    /// Canonical URL segment `/ingredients/<slug>`. None only pre-backfill.
    pub slug: Option<String>,
    /// Whether the current viewer owns this ingredient — drives the edit affordance in the UI. Always
    /// true on the owner-only edit-load path; on the detail path it reflects ownership (false for
    /// anonymous + non-owner viewers). The real guard stays server-side.
    pub can_edit: bool,
    /// Soft-deleted (tombstoned) by its owner: delisted from browse/search but preserved for the
    /// recipes that use it. Detail renders a badge; lists never surface it.
    pub deleted: bool,
    /// Owner handle (None = the communal catalog) — the detail page's breadcrumb + canonical URL.
    pub creator: Option<String>,
    /// Per-100 g nutrient rows as stored (the form scales to per-serving).
    pub nutrients: Vec<Reading>,
}

// ---- write/input wire types (mirror @vegify/db mutation inputs) ----

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// One nutrient row of a SaveIngredientInput, normalized per 100 g.
pub struct IngredientNutrientInput {
    /// Nutrient name (e.g. "Iron").
    pub name: String,
    /// Quantity per 100 g.
    pub amount_per_100g: f64,
    /// Unit for the quantity (g, mg, µg).
    pub unit: String,
}

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// Create-or-update payload for an ingredient. `id: Some` updates that row
/// (owner-guarded); `None` creates, minting the id.
pub struct SaveIngredientInput {
    /// Existing row to update, or None to create. Client-supplied ids are
    /// honored so sync replays are idempotent cross-replica.
    pub id: Option<String>,
    /// Visibility to set; None keeps the default (public).
    pub visibility: Option<Visibility>,
    /// Ingredient name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Price in cents, when tracked.
    pub price: Option<i32>, // cents
    /// Calories per 100 g, when known.
    pub calories_per_100g: Option<f64>,
    /// Serving size in grams, when declared.
    pub serving_grams: Option<f64>,
    /// Package mass in grams, when declared.
    pub package_grams: Option<f64>,
    /// Per-100 g nutrient rows (replaces the stored set).
    pub nutrients: Vec<IngredientNutrientInput>,
    /// SEO slug. `None` on a user create/edit ⇒ the DAL generates a unique one (and logs a rename to
    /// slug_history). `Some` only on the sync pull-apply, which carries the SERVER's authoritative slug
    /// so replicas never diverge. Serde-default so pre-slug payloads deserialize as None.
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// One line of a SaveRecipeInput: which ingredient, how many grams.
pub struct RecipeItemInput {
    /// The ingredient this line references.
    pub ingredient_id: String,
    /// Line quantity in grams (canonical).
    pub grams: f64,
    /// Display unit the user picked; None = grams.
    pub unit: Option<String>,
}

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// Create-or-update payload for a recipe; ids are honored when supplied so
/// sync replays are idempotent cross-replica.
pub struct SaveRecipeInput {
    /// Existing recipe to update, or None to create (id minted).
    pub id: Option<String>,
    /// The recipe's as-ingredient id. Threaded so a nested recipe (a Biga consumed by a Dough as an
    /// item) keeps a stable id cross-replica — else the consuming item's FK orphans after a pull.
    /// `None` on a fresh local create (minted); set by the sync pull when mirroring server rows.
    pub as_ingredient_id: Option<String>,
    /// Visibility to set; None keeps the default (public).
    pub visibility: Option<Visibility>,
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
    /// The recipe's lines (replaces the stored set, in order).
    pub items: Vec<RecipeItemInput>,
    /// See SaveIngredientInput::slug. `None` ⇒ generate (unique per owner); `Some` ⇒ pull carries the
    /// server's slug verbatim.
    #[serde(default)]
    pub slug: Option<String>,
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
    let Some(grams) = grams else {
        return Ok(id.map(str::to_string));
    };
    if let Some(id) = id {
        conn.execute(
            "UPDATE amounts SET grams = ?2, unit = ?3 WHERE id = ?1",
            params![id, grams, unit],
        )?;
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
    if let Some(id) = conn
        .query_row("SELECT id FROM nutrients WHERE name = ?1", [name], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
    {
        return Ok(id);
    }
    let id = new_id();
    conn.execute(
        "INSERT INTO nutrients(id, name) VALUES (?1, ?2)",
        params![id, name],
    )?;
    Ok(id)
}

/// SEO slug scope. A recipe's slug is unique among its owner's recipes (`/<username>/<slug>`); a leaf
/// ingredient's is unique globally (`/ingredients/<slug>`). The two namespaces are independent.
#[derive(Clone, Copy)]
pub enum SlugScope<'a> {
    /// A recipe slug, unique among this owner's recipes.
    UserRecipes(&'a str),
    /// A leaf-ingredient slug, unique across the whole catalog.
    GlobalIngredients,
}

impl SlugScope<'_> {
    /// The slug_history.scope key. Empty string (not NULL) for the global namespace so the
    /// UNIQUE(scope, slug) index actually dedups it — SQLite treats NULLs as distinct.
    fn key(&self) -> &str {
        match self {
            SlugScope::UserRecipes(u) => u,
            SlugScope::GlobalIngredients => "",
        }
    }
}

/// Kebab-case a display name into a URL segment: lowercase, non-alphanumeric runs → single '-',
/// trimmed, capped at 60. Empty input (name of only punctuation) falls back to "item" so the segment
/// is never blank.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > 60 {
        out.truncate(60);
        while out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        "item".to_string()
    } else {
        out
    }
}

/// Is `slug` already used in `scope` by a row other than `exclude_id`?
fn slug_taken(
    conn: &Connection,
    slug: &str,
    scope: SlugScope,
    exclude_id: &str,
) -> Result<bool, Error> {
    let n: i64 = match scope {
        SlugScope::UserRecipes(uid) => conn.query_row(
            "SELECT COUNT(*) FROM ingredients i JOIN recipes r ON r.as_ingredient_id = i.id
             WHERE i.slug = ?1 AND i.user_id = ?2 AND i.id != ?3",
            params![slug, uid, exclude_id],
            |r| r.get(0),
        )?,
        SlugScope::GlobalIngredients => conn.query_row(
            "SELECT COUNT(*) FROM ingredients i
             WHERE i.slug = ?1 AND i.id != ?2 AND i.id NOT IN (SELECT as_ingredient_id FROM recipes)",
            params![slug, exclude_id],
            |r| r.get(0),
        )?,
    };
    Ok(n > 0)
}

/// Generate a unique slug from `name` in `scope` (appending -2, -3… on collision), store it on the
/// ingredients row `ing_id`, and — if the row's slug changed (a rename) — log the OLD slug to
/// slug_history so it 301s to the row's new canonical URL. Returns the assigned slug.
fn assign_generated_slug(
    conn: &Connection,
    ing_id: &str,
    name: &str,
    scope: SlugScope,
) -> Result<String, Error> {
    let base = slugify(name);
    let mut candidate = base.clone();
    let mut n = 1u32;
    while slug_taken(conn, &candidate, scope, ing_id)? {
        n += 1;
        candidate = format!("{base}-{n}");
    }
    let current: Option<String> = conn
        .query_row(
            "SELECT slug FROM ingredients WHERE id = ?1",
            [ing_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if let Some(old) = current {
        if old != candidate && !old.is_empty() {
            let scope_key = scope.key();
            // This row is (re)claiming `candidate`; drop any stale redirect that pointed there.
            conn.execute(
                "DELETE FROM slug_history WHERE scope = ?1 AND slug = ?2",
                params![scope_key, candidate],
            )?;
            // Log old → this row (upsert: renaming back over a prior old slug just re-points it).
            conn.execute(
                "INSERT INTO slug_history(id, slug, scope, target_id) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(scope, slug) DO UPDATE SET target_id = excluded.target_id",
                params![new_id(), old, scope_key, ing_id],
            )?;
        }
    }
    conn.execute(
        "UPDATE ingredients SET slug = ?1 WHERE id = ?2",
        params![candidate, ing_id],
    )?;
    Ok(candidate)
}

/// Store a slug verbatim (the sync pull-apply — the server is authoritative, so no generation or
/// history on replicas).
fn store_slug(conn: &Connection, ing_id: &str, slug: &str) -> Result<(), Error> {
    conn.execute(
        "UPDATE ingredients SET slug = ?1 WHERE id = ?2",
        params![slug, ing_id],
    )?;
    Ok(())
}

/// One-time boot backfill: assign a slug to every ingredients row that lacks one. Recipe rows get an
/// owner-scoped slug; leaf rows a global one. Idempotent (skips rows that already have a slug).
pub fn backfill_all_slugs(conn: &Connection) -> Result<(), Error> {
    let rows: Vec<(String, String, Option<String>, bool)> = {
        let mut stmt = conn.prepare(
            "SELECT i.id, i.name, i.user_id, (r.id IS NOT NULL)
             FROM ingredients i LEFT JOIN recipes r ON r.as_ingredient_id = i.id
             WHERE i.slug IS NULL OR i.slug = '' ORDER BY i.id",
        )?;
        let v = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get::<_, i64>(3)? != 0,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    for (id, name, user_id, is_recipe) in rows {
        let scope = match (is_recipe, user_id.as_deref()) {
            (true, Some(u)) => SlugScope::UserRecipes(u),
            _ => SlugScope::GlobalIngredients,
        };
        assign_generated_slug(conn, &id, &name, scope)?;
    }
    Ok(())
}

/// Create or update an ingredient (the shared desktop + server save path).
/// Honors a client-supplied id; guards updates to the owner.
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
        let id = input
            .id
            .as_deref()
            .expect("existing row implies a supplied id");
        if !is_owner(owner.as_deref(), user_id) {
            return Err(Error::Db("You can only edit your own ingredients.".into()));
        }
        let serving_size_id =
            upsert_amount(conn, serving.as_deref(), input.serving_grams, "serving")?;
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
        conn.execute(
            "DELETE FROM ingredient_nutrient WHERE ingredient_id = ?1",
            [id],
        )?;
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

    // Slug: verbatim from the pull (server-authoritative), else generate a globally-unique one for
    // this leaf ingredient (logging a rename to slug_history).
    match input.slug.as_deref() {
        Some(s) => store_slug(conn, &ingredient_id, s)?,
        None => {
            assign_generated_slug(
                conn,
                &ingredient_id,
                &input.name,
                SlugScope::GlobalIngredients,
            )?;
        }
    }

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
        let unit = if n.unit.is_empty() {
            "g"
        } else {
            n.unit.as_str()
        };
        conn.execute(
            "INSERT INTO ingredient_nutrient(id, ingredient_id, nutrient_id, amount_per_100g, unit)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                new_id(),
                ingredient_id,
                nutrient_id,
                n.amount_per_100g,
                unit
            ],
        )?;
    }
    Ok(ingredient_id)
}

/// Soft-delete an ingredient (tombstone): delisted everywhere, preserved
/// for recipes that reference it. Owner-guarded.
pub fn do_delete_ingredient(
    conn: &Connection,
    id: &str,
    user_id: Option<&str>,
) -> Result<(), Error> {
    let existing: Option<(Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT serving_size_id, batch_size_id, user_id FROM ingredients WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((serving, batch, owner)) = existing else {
        return Ok(());
    };
    if !is_owner(owner.as_deref(), user_id) {
        return Err(Error::Db(
            "You can only delete your own ingredients.".into(),
        ));
    }
    // A recipe's as-ingredient card must go through do_delete_recipe: recipes.as_ingredient_id
    // CASCADES, so a raw delete here would silently take the whole recipe with it.
    let backs_recipe: Option<String> = conn
        .query_row(
            "SELECT id FROM recipes WHERE as_ingredient_id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    if backs_recipe.is_some() {
        return Err(Error::Db(
            "This is a recipe's ingredient card — delete the recipe instead.".into(),
        ));
    }
    // In use by any recipe → SOFT delete (tombstone). The row, amounts, and readings all survive so
    // every recipe that references it keeps working at full fidelity; it just leaves browse/search
    // (list/search/sitemap filter the tombstone) and greys out in the owner's own recipes with a
    // restore affordance (do_restore_ingredient). Unreferenced → hard delete, as before.
    let used_by: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT recipe_id) FROM ingredient_in_recipe WHERE ingredient_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    if used_by > 0 {
        conn.execute(
            "UPDATE ingredients SET deleted_at = strftime('%s','now') * 1000 WHERE id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        return Ok(());
    }
    conn.execute("DELETE FROM ingredients WHERE id = ?1", [id])?;
    delete_amounts(conn, &[serving, batch])
}

/// Undo a soft delete (the greyed row's "restore?" affordance). Owner-gated like every mutation; a
/// row that isn't tombstoned is a no-op.
pub fn do_restore_ingredient(
    conn: &Connection,
    id: &str,
    user_id: Option<&str>,
) -> Result<(), Error> {
    let owner: Option<Option<String>> = conn
        .query_row("SELECT user_id FROM ingredients WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    let Some(owner) = owner else { return Ok(()) };
    if !is_owner(owner.as_deref(), user_id) {
        return Err(Error::Db(
            "You can only restore your own ingredients.".into(),
        ));
    }
    conn.execute(
        "UPDATE ingredients SET deleted_at = NULL WHERE id = ?1",
        [id],
    )?;
    Ok(())
}

/// Row shape shared by the recipe owner-gate lookups: (as_ingredient_id,
/// serving_size_id, batch_size_id, owner user_id).
type RecipeOwnerRow = (String, Option<String>, Option<String>, Option<String>);
/// Create or update a recipe and its as-ingredient pair (the shared
/// desktop + server save path). Honors client-supplied ids; owner-guarded.
pub fn do_save_recipe(
    conn: &Connection,
    input: &SaveRecipeInput,
    user_id: Option<&str>,
) -> Result<String, Error> {
    let visibility = input.visibility.unwrap_or(Visibility::Public).as_str();
    // Upsert by id (see do_save_ingredient). A supplied-but-absent recipe id inserts WITH that id; the
    // as-ingredient id is threaded too (input.as_ingredient_id) so a nested recipe stays addressable
    // cross-replica (Biga-in-Dough). No id mints both. The owner gate applies only to an existing recipe.
    let existing: Option<RecipeOwnerRow> = match &input.id {
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
        let id = input
            .id
            .as_deref()
            .expect("existing recipe implies a supplied id");
        // The recipe's owner is its as-ingredient's owner; only the owner may edit.
        if !is_owner(owner.as_deref(), user_id) {
            return Err(Error::Db("You can only edit your own recipes.".into()));
        }
        let serving_size_id =
            upsert_amount(conn, serving.as_deref(), input.serving_grams, "serving")?;
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
            let mut stmt =
                conn.prepare("SELECT amount_id FROM ingredient_in_recipe WHERE recipe_id = ?1")?;
            let v = stmt
                .query_map([id], |r| r.get::<_, Option<String>>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        conn.execute(
            "DELETE FROM ingredient_in_recipe WHERE recipe_id = ?1",
            [id],
        )?;
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

    // Slug on the recipe's as-ingredient: verbatim from the pull, else generate one unique among the
    // owner's recipes (a rename logs to slug_history). Scope is the as-ingredient's owner.
    {
        let as_ing_id: String = conn.query_row(
            "SELECT as_ingredient_id FROM recipes WHERE id = ?1",
            [&recipe_id],
            |r| r.get(0),
        )?;
        match input.slug.as_deref() {
            Some(s) => store_slug(conn, &as_ing_id, s)?,
            None => {
                let owner: Option<String> = conn
                    .query_row(
                        "SELECT user_id FROM ingredients WHERE id = ?1",
                        [&as_ing_id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten();
                let scope = match owner.as_deref() {
                    Some(u) => SlugScope::UserRecipes(u),
                    None => SlugScope::GlobalIngredients,
                };
                assign_generated_slug(conn, &as_ing_id, &input.name, scope)?;
            }
        }
    }

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

/// Delete a recipe and its as-ingredient row. Owner-guarded.
pub fn do_delete_recipe(conn: &Connection, id: &str, user_id: Option<&str>) -> Result<(), Error> {
    let as_ing: Option<RecipeOwnerRow> = conn
        .query_row(
            "SELECT r.as_ingredient_id, i.serving_size_id, i.batch_size_id, i.user_id
             FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id WHERE r.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((as_ing_id, serving, batch, owner)) = as_ing else {
        return Ok(());
    };
    if !is_owner(owner.as_deref(), user_id) {
        return Err(Error::Db("You can only delete your own recipes.".into()));
    }

    let item_amounts: Vec<Option<String>> = {
        let mut stmt =
            conn.prepare("SELECT amount_id FROM ingredient_in_recipe WHERE recipe_id = ?1")?;
        let v = stmt
            .query_map([id], |r| r.get::<_, Option<String>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    conn.execute("DELETE FROM recipes WHERE id = ?1", [id])?; // cascades ingredient_in_recipe
    delete_amounts(conn, &item_amounts)?;

    let still_used: Option<String> = conn
        .query_row(
            "SELECT id FROM ingredient_in_recipe WHERE ingredient_id = ?1 LIMIT 1",
            [&as_ing_id],
            |r| r.get(0),
        )
        .optional()?;
    if still_used.is_none() {
        conn.execute("DELETE FROM ingredients WHERE id = ?1", [&as_ing_id])?; // cascades ingredient_nutrient
        delete_amounts(conn, &[serving, batch])?;
    }
    Ok(())
}

/// Aggregate an ingredient's (or recipe-as-ingredient's) nutrition to per-100g via the recursive CTE.
pub fn aggregate_per100g(
    conn: &Connection,
    ingredient_id: &str,
) -> Result<AggregatedNutrition, Error> {
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
                    readings.push(Reading {
                        name,
                        amount_per_100g: v,
                        unit,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(AggregatedNutrition {
        calories_per_100g,
        readings,
    })
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
                    i.visibility, i.user_id, i.slug, i.deleted_at IS NOT NULL, u.username
             FROM ingredients i
             LEFT JOIN users u ON u.id = i.user_id
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
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, bool>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            },
        )
        .optional()?;
    let Some((
        name,
        description,
        price,
        calories_per_100g,
        serving_grams,
        package_grams,
        visibility,
        owner,
        slug,
        deleted,
        creator,
    )) = meta
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
                Ok(Reading {
                    name: r.get(0)?,
                    amount_per_100g: r.get(1)?,
                    unit: r.get(2)?,
                })
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
            slug,
            can_edit: false,
            deleted,
            creator,
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
    /// Newest first (creation order).
    Newest,
    /// Oldest first.
    Oldest,
    /// Name A→Z.
    NameAsc,
    /// Name Z→A.
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
    /// Sort order for the page.
    pub sort: Sort,
    /// Keyset cursor: the last card's id from the previous page.
    pub cursor: Option<String>,
    /// The last card's name, required by the name sorts' keyset.
    pub cursor_name: Option<String>,
    /// Page size; None = unbounded.
    pub limit: Option<u32>,
}

/// List recipes visible to `viewer`, one keyset page at a time.
pub fn list_recipes(
    conn: &Connection,
    viewer: Option<&str>,
    page: &Page,
) -> Result<Vec<RecipeCard>, Error> {
    let (keyset, order) = page.sort.clauses("r.id", "i.name");
    let sql = format!(
        "SELECT r.id, i.name, r.subtitle, u.username, i.slug,
                (SELECT 'media/' || im.uuid || '.' || im.extension FROM ingredient_img ii JOIN imgs im ON im.id = ii.img_id WHERE ii.ingredient_id = i.id LIMIT 1) AS photo_key
         FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id
         LEFT JOIN users u ON u.id = i.user_id
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
            |row| {
                Ok(RecipeCard {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    subtitle: row.get(2)?,
                    username: row.get(3)?,
                    slug: row.get(4)?,
                    photo_key: row.get(5)?,
                })
            },
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
    let user: Option<(String, String, String, Option<String>)> = conn
        .query_row(
            "SELECT id, name, username, avatar_key FROM users WHERE username = ?1",
            [username],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((uid, name, username, avatar_key)) = user else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(
        "SELECT r.id, i.name, r.subtitle, u.username, i.slug,
                (SELECT 'media/' || im.uuid || '.' || im.extension FROM ingredient_img ii JOIN imgs im ON im.id = ii.img_id WHERE ii.ingredient_id = i.id LIMIT 1) AS photo_key
         FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id
         LEFT JOIN users u ON u.id = i.user_id
         WHERE i.user_id = ?1 AND (i.visibility = 'public' OR i.user_id = ?2)
         ORDER BY i.name",
    )?;
    let recipes = stmt
        .query_map(params![uid, viewer], |row| {
            Ok(RecipeCard {
                id: row.get(0)?,
                name: row.get(1)?,
                subtitle: row.get(2)?,
                username: row.get(3)?,
                slug: row.get(4)?,
                photo_key: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // The user's LEAF ingredients (not recipe cards), viewer-visible, tombstones excluded — the
    // profile's second shelf, canonical at /<username>/ingredients/<slug>.
    let mut istmt = conn.prepare(
        "SELECT i.id, i.name, i.calories_per_100g, i.slug, u.username
         FROM ingredients i
         LEFT JOIN users u ON u.id = i.user_id
         WHERE i.user_id = ?1 AND i.id NOT IN (SELECT as_ingredient_id FROM recipes)
           AND i.deleted_at IS NULL
           AND (i.visibility = 'public' OR i.user_id = ?2)
         ORDER BY i.name",
    )?;
    let ingredients = istmt
        .query_map(params![uid, viewer], |row| {
            Ok(IngredientCard {
                id: row.get(0)?,
                name: row.get(1)?,
                calories_per_100g: row.get(2)?,
                slug: row.get(3)?,
                username: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(Profile {
        username,
        name,
        recipes,
        ingredients,
        avatar_key,
    }))
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// Resolution of a recipe slug (current or historical) to its row.
pub struct RecipeSlugHit {
    /// The recipe the slug resolves to.
    pub recipe_id: String,
    /// The recipe's CURRENT slug. When it differs from the requested slug, the caller 301s to
    /// `/<username>/<canonical_slug>`.
    pub canonical_slug: String,
}

/// Resolve a `/<username>/<slug>` recipe URL to a recipe id + its current canonical slug. A live-slug
/// match returns (id, requested-slug); a slug_history hit returns (id, current-slug) so the caller
/// redirects; no match ⇒ None (404). Visibility is enforced by `recipe()` at render time, so this can
/// resolve private recipes too (the render then 404s a non-owner) — matching the by-id behavior.
pub fn resolve_recipe_by_slug(
    conn: &Connection,
    username: &str,
    slug: &str,
) -> Result<Option<RecipeSlugHit>, Error> {
    let uid: Option<String> = conn
        .query_row(
            "SELECT id FROM users WHERE username = ?1",
            [username],
            |r| r.get(0),
        )
        .optional()?;
    let Some(uid) = uid else { return Ok(None) };

    // Current-slug match.
    let live: Option<String> = conn
        .query_row(
            "SELECT r.id FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id
             WHERE i.user_id = ?1 AND i.slug = ?2",
            params![uid, slug],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(recipe_id) = live {
        return Ok(Some(RecipeSlugHit {
            recipe_id,
            canonical_slug: slug.to_string(),
        }));
    }

    // Old slug → 301 to the row's current canonical (scope = owner user_id).
    let target: Option<String> = conn
        .query_row(
            "SELECT target_id FROM slug_history WHERE scope = ?1 AND slug = ?2",
            params![uid, slug],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(target_ing) = target {
        let hit: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT r.id, i.slug FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id
                 WHERE i.id = ?1",
                [target_ing],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((recipe_id, Some(canonical_slug))) = hit {
            return Ok(Some(RecipeSlugHit {
                recipe_id,
                canonical_slug,
            }));
        }
    }
    Ok(None)
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// Resolution of an ingredient slug (current or historical) to its row.
pub struct IngredientSlugHit {
    /// The ingredient the slug resolves to.
    pub ingredient_id: String,
    /// The ingredient's CURRENT slug — differs when the hit came from slug
    /// history, in which case the caller 301s here.
    pub canonical_slug: String,
    /// Owner handle when the ingredient is user-owned: `/ingredients/<slug>` 301s to
    /// `/<username>/ingredients/<slug>` (the catalog stays at the global path).
    pub username: Option<String>,
}

/// Resolve `/ingredients/<slug>` → an ingredient id + its current canonical slug. Live-slug match
/// among LEAF ingredients (recipe as-ingredients live at `/<user>/<slug>`, not here), else a
/// slug_history hit (scope = "" for the global ingredient namespace → 301), else None. Visibility is
/// enforced by `ingredient()` at render time.
pub fn resolve_ingredient_by_slug(
    conn: &Connection,
    slug: &str,
) -> Result<Option<IngredientSlugHit>, Error> {
    let live: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT i.id, u.username FROM ingredients i
             LEFT JOIN users u ON u.id = i.user_id
             WHERE i.slug = ?1 AND i.id NOT IN (SELECT as_ingredient_id FROM recipes)",
            [slug],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((ingredient_id, username)) = live {
        return Ok(Some(IngredientSlugHit {
            ingredient_id,
            canonical_slug: slug.to_string(),
            username,
        }));
    }
    let target: Option<String> = conn
        .query_row(
            "SELECT target_id FROM slug_history WHERE scope = '' AND slug = ?1",
            [slug],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(target) = target {
        let canonical: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT i.slug, u.username FROM ingredients i
                 LEFT JOIN users u ON u.id = i.user_id WHERE i.id = ?1",
                [&target],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((Some(canonical_slug), username)) = canonical {
            return Ok(Some(IngredientSlugHit {
                ingredient_id: target,
                canonical_slug,
                username,
            }));
        }
    }
    Ok(None)
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// Owner handle (first segment of the canonical URL).
pub struct SitemapRecipe {
    /// Owner handle (first segment of the canonical URL).
    pub username: String,
    /// The recipe's slug (second URL segment).
    pub slug: String,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// One indexable ingredient URL for the sitemap.
pub struct SitemapIngredient {
    /// Owner handle: owned rows are canonical at `/<username>/ingredients/<slug>`; None = catalog
    /// (`/ingredients/<slug>`).
    pub username: Option<String>,
    /// The ingredient's slug.
    pub slug: String,
}

/// The public, canonical, indexable URLs — everything with a slug that anyone can read. Recipes and
/// OWNED ingredients carry their owner handle; unowned ingredients are the catalog namespace.
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SitemapData {
    /// All indexable recipe URLs.
    pub recipes: Vec<SitemapRecipe>,
    /// All indexable ingredient URLs.
    pub ingredients: Vec<SitemapIngredient>,
}

/// Enumerate every PUBLIC recipe (owner handle + slug) and PUBLIC leaf ingredient (slug) for the
/// sitemap. Public-only + slug-bearing — never private/unlisted, never a pre-backfill NULL slug, never
/// a headless (no-username) recipe. No viewer/pagination: a crawler-facing full list.
pub fn public_sitemap(conn: &Connection) -> Result<SitemapData, Error> {
    let recipes = {
        let mut stmt = conn.prepare(
            "SELECT u.username, i.slug
             FROM recipes r
             JOIN ingredients i ON i.id = r.as_ingredient_id
             JOIN users u ON u.id = i.user_id
             WHERE i.visibility = 'public' AND i.slug IS NOT NULL AND u.username IS NOT NULL
             ORDER BY i.slug",
        )?;
        let v = stmt
            .query_map([], |row| {
                Ok(SitemapRecipe {
                    username: row.get(0)?,
                    slug: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    let ingredients = {
        let mut stmt = conn.prepare(
            "SELECT u.username, i.slug FROM ingredients i
             LEFT JOIN users u ON u.id = i.user_id
             WHERE i.id NOT IN (SELECT as_ingredient_id FROM recipes)
               AND i.deleted_at IS NULL
               AND i.visibility = 'public' AND i.slug IS NOT NULL
             ORDER BY i.slug",
        )?;
        let v = stmt
            .query_map([], |row| {
                Ok(SitemapIngredient {
                    username: row.get(0)?,
                    slug: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    Ok(SitemapData {
        recipes,
        ingredients,
    })
}

/// Ingredient search (isListed: public + own — same scoping as the lists), with per-100g nutrition.
pub fn search_ingredients(
    conn: &Connection,
    query: String,
    viewer: Option<&str>,
) -> Result<Vec<IngredientSearchResult>, Error> {
    let like = format!("%{}%", query.replace(['%', '_'], ""));
    let rows: Vec<(String, String, Option<f64>)> = {
        let mut stmt = conn.prepare(
            "SELECT i.id, i.name, sa.grams
             FROM ingredients i
             LEFT JOIN amounts sa ON sa.id = i.serving_size_id
             WHERE i.name LIKE ?1 AND i.deleted_at IS NULL
               AND (i.visibility = 'public' OR i.user_id = ?2)
             ORDER BY i.name LIMIT 20",
        )?;
        let v = stmt
            .query_map(params![like, viewer], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
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
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                ))
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
        "SELECT i.id, i.name, i.calories_per_100g, i.slug, u.username
         FROM ingredients i
         LEFT JOIN users u ON u.id = i.user_id
         WHERE i.id NOT IN (SELECT as_ingredient_id FROM recipes)
           AND i.deleted_at IS NULL
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
            |row| {
                Ok(IngredientCard {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    calories_per_100g: row.get(2)?,
                    slug: row.get(3)?,
                    username: row.get(4)?,
                })
            },
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
pub fn recipe(
    conn: &Connection,
    id: String,
    viewer: Option<&str>,
) -> Result<Option<RecipeView>, Error> {
    let meta = conn
        .query_row(
            "SELECT i.id, i.name, r.subtitle, r.directions, u.username,
                    sa.amount, sa.unit, sa.grams, ba.grams, i.user_id, i.slug,
                    (SELECT 'media/' || im.uuid || '.' || im.extension FROM ingredient_img ii JOIN imgs im ON im.id = ii.img_id WHERE ii.ingredient_id = i.id LIMIT 1) AS photo_key
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
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                ))
            },
        )
        .optional()?;
    let Some((
        as_ing_id,
        name,
        subtitle,
        directions,
        creator,
        s_amount,
        s_unit,
        s_grams,
        batch_grams,
        owner,
        slug,
        photo_key,
    )) = meta
    else {
        return Ok(None);
    };
    let can_edit = is_owner(owner.as_deref(), viewer);

    let mut istmt = conn.prepare(
        "SELECT i.id, i.name, a.amount, a.unit, a.grams, r2.id AS recipe_id,
                i.deleted_at IS NOT NULL, i.user_id
         FROM ingredient_in_recipe iir
         JOIN ingredients i ON i.id = iir.ingredient_id
         JOIN amounts a ON a.id = iir.amount_id
         LEFT JOIN recipes r2 ON r2.as_ingredient_id = i.id
         WHERE iir.recipe_id = ?1 ORDER BY iir.\"order\"",
    )?;
    let recipe_owner = owner.clone();
    let items = istmt
        .query_map([&id], |row| {
            let tombstoned: bool = row.get(6)?;
            let ing_owner: Option<String> = row.get(7)?;
            Ok(RecipeItem {
                id: row.get(0)?,
                name: row.get(1)?,
                amount: Amount {
                    amount: row.get(2)?,
                    unit: row.get(3)?,
                    grams: row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                },
                recipe_id: row.get(5)?,
                // Greyed ONLY in the deleter's own recipes (ingredient owner == recipe owner);
                // everyone else's recipes render it untouched.
                deleted: tombstoned && ing_owner.is_some() && ing_owner == recipe_owner,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let nutrition = aggregate_per100g(conn, &as_ing_id)?;
    let serving = s_grams.map(|grams| Amount {
        amount: s_amount,
        unit: s_unit,
        grams,
    });

    Ok(Some(RecipeView {
        id,
        name,
        subtitle,
        directions,
        creator,
        slug,
        can_edit,
        serving,
        batch_grams,
        items,
        nutrition,
        photo_key,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod delete_guard_tests {
    use super::*;

    /// The REAL client schema (drift-test-pinned to the drizzle source in the desktop crate).
    const CLIENT_SCHEMA: &str = include_str!("../../../apps/desktop/src-tauri/schema.sql");

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        c.execute_batch(CLIENT_SCHEMA).unwrap();
        // username is a SERVER-side ensure_schema addition (not in the client schema) — mirror it.
        c.execute_batch("ALTER TABLE users ADD COLUMN username TEXT;")
            .unwrap();
        c.execute(
            "INSERT INTO users (id, name, email, username) VALUES ('u1', 'Ada', 'a@x', 'ada')",
            [],
        )
        .unwrap();
        c
    }

    fn save_leaf(c: &Connection, name: &str) -> String {
        do_save_ingredient(
            c,
            &SaveIngredientInput {
                id: None,
                visibility: Some(Visibility::Public),
                name: name.into(),
                description: None,
                price: None,
                calories_per_100g: None,
                serving_grams: None,
                package_grams: None,
                nutrients: vec![],
                slug: None,
            },
            Some("u1"),
        )
        .unwrap()
    }

    #[test]
    fn deleting_an_in_use_ingredient_soft_deletes_scoped_to_the_deleters_own_recipes() {
        let c = conn();
        c.execute(
            "INSERT INTO users (id, name, email) VALUES ('u2', 'Bob', 'b@x')",
            [],
        )
        .unwrap();
        let flour = save_leaf(&c, "Flour");
        // u1's own recipe uses it, and so does u2's.
        let mine = save_recipe_with(&c, "My Bread", &flour, "u1");
        let theirs = save_recipe_with(&c, "Their Bread", &flour, "u2");

        // Soft delete: Ok, row survives tombstoned, readings intact.
        do_delete_ingredient(&c, &flour, Some("u1")).unwrap();
        let (alive, tombstoned): (i64, bool) = c
            .query_row(
                "SELECT COUNT(*), MAX(deleted_at IS NOT NULL) FROM ingredients WHERE id = ?1",
                [&flour],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((alive, tombstoned), (1, true), "tombstoned, not destroyed");

        // Delisted everywhere public: browse, search, sitemap.
        let listed = list_ingredients(&c, Some("u1"), &Page::default()).unwrap();
        assert!(
            listed.iter().all(|i| i.id != flour),
            "browse must exclude the tombstone"
        );
        let found = search_ingredients(&c, "Flour".into(), Some("u1")).unwrap();
        assert!(
            found.iter().all(|i| i.id != flour),
            "search must exclude the tombstone"
        );

        // Greyed ONLY in the deleter's own recipe; the other user's recipe renders it untouched.
        let my_view = recipe(&c, mine.clone(), Some("u1")).unwrap().unwrap();
        assert!(
            my_view.items[0].deleted,
            "deleter's own recipe shows the tombstone"
        );
        let their_view = recipe(&c, theirs.clone(), Some("u2")).unwrap().unwrap();
        assert!(
            !their_view.items[0].deleted,
            "other users' recipes are untouched"
        );
        assert_eq!(their_view.items[0].name, "Flour", "full fidelity preserved");

        // Restore (owner-gated) brings it back to life everywhere.
        assert!(
            do_restore_ingredient(&c, &flour, Some("u2")).is_err(),
            "only the owner restores"
        );
        do_restore_ingredient(&c, &flour, Some("u1")).unwrap();
        let my_view = recipe(&c, mine, Some("u1")).unwrap().unwrap();
        assert!(!my_view.items[0].deleted);
        let found = search_ingredients(&c, "Flour".into(), None).unwrap();
        assert!(
            found.iter().any(|i| i.id == flour),
            "restored = searchable again"
        );

        // Unreferenced deletes stay HARD: drop both recipes, delete again, row is gone.
        do_delete_recipe(&c, &theirs, Some("u2")).unwrap();
        // (u2 owns "Their Bread" — its delete already freed one reference.)
        let my2: String = c
            .query_row("SELECT id FROM recipes LIMIT 1", [], |r| r.get(0))
            .unwrap();
        do_delete_recipe(&c, &my2, Some("u1")).unwrap();
        do_delete_ingredient(&c, &flour, Some("u1")).unwrap();
        let left: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM ingredients WHERE id = ?1",
                [&flour],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "unreferenced delete removes the row");
    }

    fn save_recipe_with(c: &Connection, name: &str, ingredient_id: &str, user: &str) -> String {
        do_save_recipe(
            c,
            &SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: Some(Visibility::Public),
                name: name.into(),
                subtitle: None,
                directions: None,
                serving_grams: None,
                batch_grams: None,
                items: vec![RecipeItemInput {
                    ingredient_id: ingredient_id.to_string(),
                    grams: 500.0,
                    unit: None,
                }],
                slug: None,
            },
            Some(user),
        )
        .unwrap()
    }

    #[test]
    fn deleting_a_recipes_ingredient_card_directly_is_refused_it_would_cascade_the_recipe() {
        let c = conn();
        let recipe = do_save_recipe(
            &c,
            &SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: Some(Visibility::Public),
                name: "Soup".into(),
                subtitle: None,
                directions: None,
                serving_grams: None,
                batch_grams: None,
                items: vec![],
                slug: None,
            },
            Some("u1"),
        )
        .unwrap();
        let card: String = c
            .query_row(
                "SELECT as_ingredient_id FROM recipes WHERE id = ?1",
                [&recipe],
                |r| r.get(0),
            )
            .unwrap();
        let err = do_delete_ingredient(&c, &card, Some("u1")).unwrap_err();
        assert!(format!("{err:?}").contains("delete the recipe instead"));
        let recipes_left: i64 = c
            .query_row("SELECT COUNT(*) FROM recipes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            recipes_left, 1,
            "the recipe must survive the refused card delete"
        );
    }
}
