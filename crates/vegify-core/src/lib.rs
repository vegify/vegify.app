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
    Ulid::generate().to_string()
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
    /// The serving's unit name (a count unit like "bun"/"slice"; "serving" by default) — the composer
    /// offers it as a count unit whose grams factor is `serving_grams`. None ⇒ no serving declared.
    pub serving_unit: Option<String>,
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
    /// The count in `unit` the author entered (e.g. 2 for "2 buns"); mirrors grams on a legacy line.
    pub amount: f64,
    /// Display unit for `amount`; None ⇒ grams.
    pub unit: Option<String>,
    /// "units" (show `amount unit`) or "grams" (show grams) — how the line was entered.
    pub preferred: String,
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
    /// The serving's unit name (a count unit like "bun"/"slice"; "serving" by default). None ⇒ no
    /// serving declared. Round-trips through the ingredient form so it can be renamed.
    pub serving_unit: Option<String>,
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
    /// The serving's unit name (a count unit like "bun"/"slice"; None ⇒ "serving"). Stored on the
    /// serving amount's `unit`, so a recipe line can be entered as a count of it.
    #[serde(default)]
    pub serving_unit: Option<String>,
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
/// One line of a SaveRecipeInput: which ingredient, how much. `grams` is always canonical (all
/// nutrition math reads it); `amount` + `unit` are the display form the user entered (e.g. 2 "bun"),
/// preserved so the line reads naturally instead of as raw grams.
pub struct RecipeItemInput {
    /// The ingredient this line references.
    pub ingredient_id: String,
    /// Line quantity in grams (canonical).
    pub grams: f64,
    /// Display unit the user picked; None ⇒ grams.
    pub unit: Option<String>,
    /// The count in `unit` (e.g. 2 for "2 buns"). None ⇒ legacy grams-only line (amount == grams).
    #[serde(default)]
    pub amount: Option<f64>,
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
    // The serving's unit name (a count unit like "bun"/"slice"); defaults to "serving". Stored on the
    // serving amount so a recipe line can be entered as a count of it.
    let serving_unit = input.serving_unit.as_deref().unwrap_or("serving");
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
            upsert_amount(conn, serving.as_deref(), input.serving_grams, serving_unit)?;
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
        let serving_size_id = upsert_amount(conn, None, input.serving_grams, serving_unit)?;
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
    // In use by any recipe OR any live diary entry → SOFT delete (tombstone). The row, amounts, and
    // readings all survive so every recipe that references it keeps working at full fidelity and every
    // logged day still resolves; it just leaves browse/search (list/search/sitemap filter the tombstone)
    // and greys out in the owner's own recipes with a restore affordance (do_restore_ingredient).
    // Counting log_entries here is also what keeps the log_entries RESTRICT FK from ever firing.
    // Unreferenced → hard delete, as before.
    let used_by: i64 = conn.query_row(
        "SELECT (SELECT COUNT(DISTINCT recipe_id) FROM ingredient_in_recipe WHERE ingredient_id = ?1)
              + (SELECT COUNT(*) FROM log_entries WHERE ingredient_id = ?1 AND deleted_at IS NULL)",
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
        // Display form: `unit` labels the quantity, `amount` is the count in it. A non-gram unit
        // (bun/slice/oz/…) is shown "as units"; a grams line (or a legacy amount-less payload) is shown
        // as grams. Canonical `grams` is untouched either way — all nutrition math reads only grams.
        let (unit, preferred) = match item.unit.as_deref() {
            Some(u) if !u.is_empty() && u != "g" => (u, "units"),
            _ => ("g", "grams"),
        };
        let amount = item.amount.unwrap_or(item.grams);
        let amount_id = new_id();
        conn.execute(
            "INSERT INTO amounts(id, grams, amount, unit, preferred) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![amount_id, item.grams, amount, unit, preferred],
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

// ---- diary (log_entries): a user's PRIVATE food log; day totals reuse aggregate_per100g ----

/// Current time in Unix milliseconds — log timestamps + soft-delete tombstones.
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// Create-or-update payload for a diary entry. `id: Some` + an existing row updates it (owner-gated);
/// `id: Some` + no such row inserts WITH that id (an offline create's client ULID / a sync replay);
/// `id: None` mints a fresh ULID. Logging a recipe = passing its as-ingredient id as `ingredientId`.
pub struct SaveLogEntryInput {
    /// Existing entry to update, or None to create (id minted). Client ids are honored so replays
    /// stay idempotent cross-replica.
    pub id: Option<String>,
    /// The ingredient — or a recipe's as_ingredient_id — being logged.
    pub ingredient_id: String,
    /// User-local calendar date 'YYYY-MM-DD', chosen client-side (no server timezone modeling).
    pub date: String,
    /// Meal slot (breakfast/lunch/dinner/snack); currently ignored by the UI. Serde-default so
    /// slot-less payloads deserialize.
    #[serde(default)]
    pub slot: Option<String>,
    /// Amount logged, canonical grams.
    pub grams: f64,
    /// Display unit the user picked; None = grams. Mirrors RecipeItemInput.
    #[serde(default)]
    pub unit: Option<String>,
    /// When it was logged (ms epoch); None ⇒ now. Set by a sync replay to preserve ordering.
    #[serde(default)]
    pub logged_at: Option<i64>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// One diary entry as read: what was logged, how much, and — when the logged row is itself a
/// recipe-as-ingredient — a link to that recipe (mirrors RecipeItem.recipe_id). `calories` is this
/// entry's own contribution (its per-100g rollup × grams/100), for the row display.
pub struct LogEntryView {
    /// The entry id.
    pub id: String,
    /// User-local calendar date 'YYYY-MM-DD'.
    pub date: String,
    /// Meal slot; currently unused by the UI.
    pub slot: Option<String>,
    /// The logged ingredient's id (a recipe's as-ingredient id when a recipe was logged).
    pub ingredient_id: String,
    /// Ingredient (or recipe) display name at read time.
    pub name: String,
    /// Set when the logged row is itself a recipe-as-ingredient, so the UI links to the recipe page.
    pub recipe_id: Option<String>,
    /// The amount logged (display form + canonical grams).
    pub amount: Amount,
    /// This entry's calorie contribution (per-100g rollup × grams/100), when calorie data exists.
    pub calories: Option<f64>,
    /// When it was logged (ms epoch).
    pub logged_at: i64,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// One nutrient's ABSOLUTE total for a day (summed across entries), keyed by name + unit. Distinct
/// from `Reading`, which is per-100g.
pub struct NutrientTotal {
    /// Nutrient name (e.g. "Iron").
    pub name: String,
    /// Absolute amount for the day, in `unit`.
    pub amount: f64,
    /// Display unit (g, mg, µg).
    pub unit: String,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// One diary day: the entries plus server-computed nutrient totals, each rolled up from its entry via
/// the SAME recursive CTE recipes use (so a logged recipe expands through its nesting). PRIVATE — only
/// ever returned to its owner, never listed, never in the anonymous pull.
pub struct DayLog {
    /// The 'YYYY-MM-DD' this day covers (echoes the request).
    pub date: String,
    /// The day's live entries, newest logged first.
    pub entries: Vec<LogEntryView>,
    /// Total calories for the day; None when no entry carries calorie data.
    pub calories: Option<f64>,
    /// Per-nutrient absolute totals for the day, ordered by nutrient name.
    pub totals: Vec<NutrientTotal>,
    /// The viewer's personalized vegan-aware daily targets (from their profile; generic-adult when
    /// unset), with per-DAY supplement coverage applied. Returned per-day so the Day screen can render
    /// progress vs. targets in one payload. Match a `total` to its `target` by nutrient name.
    pub targets: Vec<NutrientTarget>,
    /// The supplements taken on this day (effective, carry-forward). Renders the day's supplement
    /// checklist and drives the `supplementCovered` flags above.
    pub supplements: DaySupplements,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
/// A recently/frequently logged ingredient, surfaced so re-logging is fast (the add-flow prepends
/// these before global search — personal ranking beats global relevance for logging speed).
pub struct RecentIngredient {
    /// The ingredient's id (a recipe's as-ingredient id when a recipe).
    pub ingredient_id: String,
    /// Display name.
    pub name: String,
    /// Set when it's a recipe-as-ingredient (the UI can badge/link it).
    pub recipe_id: Option<String>,
    /// How many times the user has logged it (lifetime, non-deleted).
    pub count: i64,
    /// The most recent log's grams, to prefill the amount on re-log.
    pub last_grams: f64,
    /// The most recent log's display unit, if any.
    pub last_unit: Option<String>,
    /// When it was last logged (ms epoch).
    pub last_logged_at: i64,
}

/// Freeze a log entry's nutrition into its immutable snapshot: compute the logged food's CURRENT
/// per-100g rollup (the same recursive CTE recipes use) and store it — calories on the entry plus one
/// `log_entry_nutrient` row per reading — replacing any prior snapshot. This is what makes a logged day
/// permanent: `log_day` reads THIS, never the live graph, so a later edit to the source recipe/ingredient
/// can't rewrite a past day. Called at create and whenever the entry's ingredient itself changes.
fn snapshot_log_entry_nutrition(
    conn: &Connection,
    log_entry_id: &str,
    ingredient_id: &str,
) -> Result<(), Error> {
    let agg = aggregate_per100g(conn, ingredient_id)?;
    conn.execute(
        "UPDATE log_entries SET calories_per_100g = ?2 WHERE id = ?1",
        params![log_entry_id, agg.calories_per_100g],
    )?;
    conn.execute(
        "DELETE FROM log_entry_nutrient WHERE log_entry_id = ?1",
        [log_entry_id],
    )?;
    for r in &agg.readings {
        conn.execute(
            "INSERT INTO log_entry_nutrient(id, log_entry_id, name, amount_per_100g, unit)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![new_id(), log_entry_id, r.name, r.amount_per_100g, r.unit],
        )?;
    }
    Ok(())
}

/// Create or update a diary entry (owner-guarded on update). Mints the entry's `amounts` row on create
/// — the ingredient_in_recipe amount pattern, grams canonical — and FREEZES the food's nutrition into
/// the entry's snapshot (see `snapshot_log_entry_nutrition`), re-freezing only when the logged food
/// itself changes. Returns the entry id.
pub fn do_save_log_entry(
    conn: &Connection,
    input: &SaveLogEntryInput,
    user_id: &str,
) -> Result<String, Error> {
    // Require a finite, strictly-positive mass (rejects 0, negatives, and any NaN/inf).
    if !(input.grams.is_finite() && input.grams > 0.0) {
        return Err(Error::Db(
            "A log entry needs a positive gram amount.".into(),
        ));
    }
    let date = input.date.trim();
    if date.is_empty() {
        return Err(Error::Db("A log entry needs a date.".into()));
    }
    // The logged item must exist (a recipe is logged via its as-ingredient id). A clean message beats
    // the raw FK error the INSERT would otherwise raise.
    let item_exists = conn
        .query_row(
            "SELECT 1 FROM ingredients WHERE id = ?1",
            [&input.ingredient_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !item_exists {
        return Err(Error::Db("That ingredient no longer exists.".into()));
    }
    let unit = input.unit.as_deref().unwrap_or("");
    let logged_at = input.logged_at.unwrap_or_else(now_ms);

    // Upsert by id (see do_save_ingredient): fetch the row's owner, its amount, and its CURRENT
    // ingredient (to detect a food change) only when an id was supplied.
    let existing: Option<(String, String, String)> = match &input.id {
        Some(id) => conn
            .query_row(
                "SELECT user_id, amount_id, ingredient_id FROM log_entries WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?,
        None => None,
    };

    if let Some((owner, amount_id, prev_ingredient)) = existing {
        let id = input
            .id
            .as_deref()
            .expect("existing row implies a supplied id");
        if owner != user_id {
            return Err(Error::Db("You can only edit your own log entries.".into()));
        }
        upsert_amount(conn, Some(&amount_id), Some(input.grams), unit)?;
        // A re-save makes the row live again (there's no separate restore for logs; a sync replay of a
        // create arrives here as an update).
        conn.execute(
            "UPDATE log_entries SET date = ?2, slot = ?3, ingredient_id = ?4, logged_at = ?5,
             deleted_at = NULL WHERE id = ?1",
            params![
                id,
                date,
                input.slot.as_deref(),
                input.ingredient_id,
                logged_at
            ],
        )?;
        // Re-freeze the snapshot ONLY when the logged food itself changed. A grams/date/slot edit keeps
        // the original frozen profile (grams rescales it in log_day); a passive edit to the source
        // ingredient/recipe never reaches here, so a past day never moves.
        if prev_ingredient != input.ingredient_id {
            snapshot_log_entry_nutrition(conn, id, &input.ingredient_id)?;
        }
        Ok(id.to_string())
    } else {
        let amount_id = upsert_amount(conn, None, Some(input.grams), unit)?
            .expect("grams present ⇒ an amount id is minted");
        let id = input.id.clone().unwrap_or_else(new_id);
        conn.execute(
            "INSERT INTO log_entries(id, user_id, date, slot, ingredient_id, amount_id, logged_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                user_id,
                date,
                input.slot.as_deref(),
                input.ingredient_id,
                amount_id,
                logged_at
            ],
        )?;
        // Freeze the food's nutrition at log time (the immutable half of the record).
        snapshot_log_entry_nutrition(conn, &id, &input.ingredient_id)?;
        Ok(id)
    }
}

/// Soft-delete a diary entry (owner-guarded). Sets deleted_at; the row + its amount survive so an
/// undo/versioning story and sync propagation stay possible. A missing row is a no-op; a foreign row
/// is refused.
pub fn do_delete_log_entry(conn: &Connection, id: &str, user_id: &str) -> Result<(), Error> {
    let owner: Option<String> = conn
        .query_row(
            "SELECT user_id FROM log_entries WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(owner) = owner else {
        return Ok(()); // already gone or never existed
    };
    if owner != user_id {
        return Err(Error::Db(
            "You can only delete your own log entries.".into(),
        ));
    }
    conn.execute(
        "UPDATE log_entries SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
        params![id, now_ms()],
    )?;
    Ok(())
}

/// One day's entries + server-computed nutrient totals for `user_id`, read from each entry's FROZEN
/// snapshot (the per-100g values captured at log time), NOT the live ingredient graph — so a later edit
/// to a recipe/ingredient can't move a past day. Each entry's grams (still editable) rescales its frozen
/// per-100g profile; totals sum across entries keyed by (nutrient name, unit), ordered by name.
pub fn log_day(conn: &Connection, user_id: &str, date: &str) -> Result<DayLog, Error> {
    // 1. The day's entries, enriched with the ingredient's display name + a recipe link when the logged
    // row is itself a recipe. `calories` = the entry's frozen calories_per_100g scaled by grams. (name +
    // recipe link stay LIVE — they're presentational; only the NUMBERS are snapshotted.)
    let mut stmt = conn.prepare(
        "SELECT le.id, le.date, le.slot, le.ingredient_id, i.name, r.id AS recipe_id,
                a.amount, a.unit, a.grams, le.logged_at, le.calories_per_100g
         FROM log_entries le
         JOIN ingredients i ON i.id = le.ingredient_id
         JOIN amounts a ON a.id = le.amount_id
         LEFT JOIN recipes r ON r.as_ingredient_id = le.ingredient_id
         WHERE le.user_id = ?1 AND le.date = ?2 AND le.deleted_at IS NULL
         ORDER BY le.logged_at DESC",
    )?;
    let entries = stmt
        .query_map(params![user_id, date], |row| {
            let grams: f64 = row.get::<_, Option<f64>>(8)?.unwrap_or(0.0);
            let cal_per_100g: Option<f64> = row.get(10)?;
            Ok(LogEntryView {
                id: row.get(0)?,
                date: row.get(1)?,
                slot: row.get(2)?,
                ingredient_id: row.get(3)?,
                name: row.get(4)?,
                recipe_id: row.get(5)?,
                amount: Amount {
                    amount: row.get(6)?,
                    unit: row.get(7)?,
                    grams,
                },
                calories: cal_per_100g.map(|c| c * grams / 100.0),
                logged_at: row.get::<_, Option<i64>>(9)?.unwrap_or(0),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // Day calories = sum of the entries' scaled snapshots; None only when no entry carries calorie data.
    let day_calories = entries
        .iter()
        .filter_map(|e| e.calories)
        .fold(None, |acc, c| Some(acc.unwrap_or(0.0) + c));

    // 2. Day nutrient totals: sum each entry's FROZEN per-nutrient snapshot, scaled by that entry's grams,
    // in one grouped pass. Reads only log_entry_nutrient — never the live graph. Ordered by nutrient name.
    let mut tstmt = conn.prepare(
        "SELECT len.name, len.unit, SUM(len.amount_per_100g * a.grams / 100.0) AS total
         FROM log_entry_nutrient len
         JOIN log_entries le ON le.id = len.log_entry_id
         JOIN amounts a ON a.id = le.amount_id
         WHERE le.user_id = ?1 AND le.date = ?2 AND le.deleted_at IS NULL
         GROUP BY len.name, len.unit
         ORDER BY len.name",
    )?;
    let totals = tstmt
        .query_map(params![user_id, date], |row| {
            Ok(NutrientTotal {
                name: row.get(0)?,
                unit: row.get(1)?,
                amount: row.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // 3. The viewer's personalized targets (profile-driven; generic-adult when unset). The age bracket
    // uses the VIEWED day's year — pure and clock-free (the date is already validated 'YYYY-MM-DD').
    // Supplement coverage is now per-DAY (carry-forward from the last recorded day), so targets reflect
    // whether THIS day's supplements were taken, not a standing profile setting.
    let year: i64 = date.get(0..4).and_then(|y| y.parse().ok()).unwrap_or(1970);
    let supplements = get_day_supplements(conn, user_id, date)?;
    let target_list = targets(&get_nutrition_profile(conn, user_id)?, &supplements, year);

    Ok(DayLog {
        date: date.to_string(),
        entries,
        calories: day_calories,
        totals,
        targets: target_list,
        supplements,
    })
}

/// Distinct ingredients the user has logged, most-recent-logged first (capped at `limit`). Powers the
/// add-flow's personal "recents" that prepend global search; each carries the latest log's amount to
/// prefill a re-log. One window-function pass: rn=1 picks each ingredient's newest entry, the partition
/// COUNT gives its lifetime frequency.
pub fn log_recents(
    conn: &Connection,
    user_id: &str,
    limit: i64,
) -> Result<Vec<RecentIngredient>, Error> {
    let mut stmt = conn.prepare(
        "SELECT ingredient_id, name, recipe_id, cnt, last_grams, last_unit, last_logged_at FROM (
           SELECT le.ingredient_id AS ingredient_id, i.name AS name, r.id AS recipe_id,
                  COUNT(*) OVER (PARTITION BY le.ingredient_id) AS cnt,
                  a.grams AS last_grams, a.unit AS last_unit, le.logged_at AS last_logged_at,
                  ROW_NUMBER() OVER (
                    PARTITION BY le.ingredient_id ORDER BY le.logged_at DESC, le.id DESC
                  ) AS rn
           FROM log_entries le
           JOIN ingredients i ON i.id = le.ingredient_id
           JOIN amounts a ON a.id = le.amount_id
           LEFT JOIN recipes r ON r.as_ingredient_id = le.ingredient_id
           WHERE le.user_id = ?1 AND le.deleted_at IS NULL
         ) WHERE rn = 1
         ORDER BY last_logged_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![user_id, limit], |row| {
            Ok(RecentIngredient {
                ingredient_id: row.get(0)?,
                name: row.get(1)?,
                recipe_id: row.get(2)?,
                count: row.get(3)?,
                last_grams: row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                last_unit: row.get(5)?,
                last_logged_at: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---- authed diary sync (the desktop's local-first cache pull; SEPARATE from the anon content pull) ----

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// One nutrient of a pulled entry's FROZEN snapshot (per 100 g) — applied verbatim, never recomputed.
pub struct LogPullNutrient {
    /// Nutrient name.
    pub name: String,
    /// Frozen per-100g amount.
    pub amount_per_100g: f64,
    /// Display unit.
    pub unit: String,
}

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// One diary entry in the authed pull: everything to rebuild it on a replica VERBATIM — its frozen
/// nutrition snapshot (calories + per-nutrient rows) and its display amount. The snapshot is NOT
/// recomputed on apply; the server's is authoritative.
pub struct LogPullEntry {
    /// Entry id (client ULID, stable cross-replica).
    pub id: String,
    /// The logged ingredient (or a recipe's as-ingredient id).
    pub ingredient_id: String,
    /// User-local calendar date 'YYYY-MM-DD'.
    pub date: String,
    /// Meal slot, if any.
    pub slot: Option<String>,
    /// Amount logged, canonical grams.
    pub grams: f64,
    /// Display unit the user picked, if any.
    pub unit: Option<String>,
    /// Frozen per-100g calories snapshot.
    pub calories_per_100g: Option<f64>,
    /// When it was logged (ms epoch).
    pub logged_at: i64,
    /// The frozen per-nutrient snapshot.
    pub nutrients: Vec<LogPullNutrient>,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug)]
#[serde(rename_all = "camelCase")]
/// A dated supplement record: the supplements taken on one `date`. Doubles as the authed-pull row (to
/// rebuild the day_supplements table on a replica verbatim) and the `save_day_supplements` write payload.
pub struct DaySupplementsRecord {
    /// User-local calendar date 'YYYY-MM-DD' this record pins (incl. the '1970-01-01' migration floor).
    pub date: String,
    /// Took a B12 supplement that day.
    pub b12: bool,
    /// Took a vitamin D supplement that day.
    pub vit_d: bool,
    /// Took an algae-oil (EPA+DHA) supplement that day.
    pub algae_oil: bool,
}

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
/// The viewer's FULL diary for authed device sync — a separate channel from the anonymous content pull,
/// which never carries private log data. The desktop reconciles this into its local cache via
/// `apply_log_pull` (its OWN reconciliation, isolated from `apply_pull`'s content rebuild). Carries the
/// day-supplement records alongside the entries — both are private per-day diary data on one channel.
pub struct LogPull {
    /// Every live entry the viewer owns, newest logged first.
    pub entries: Vec<LogPullEntry>,
    /// Every day-supplement record the viewer owns (all dates), applied verbatim on the replica.
    #[serde(default)]
    pub supplements: Vec<DaySupplementsRecord>,
}

/// Read the viewer's ENTIRE live diary (all dates) for authed device sync — each entry with its frozen
/// snapshot (calories + per-nutrient readings) + display amount. Newest logged first.
pub fn log_pull(conn: &Connection, user_id: &str) -> Result<LogPull, Error> {
    let mut stmt = conn.prepare(
        "SELECT le.id, le.ingredient_id, le.date, le.slot, a.grams, a.unit,
                le.calories_per_100g, le.logged_at
         FROM log_entries le
         JOIN amounts a ON a.id = le.amount_id
         WHERE le.user_id = ?1 AND le.deleted_at IS NULL
         ORDER BY le.logged_at DESC",
    )?;
    let base = stmt
        .query_map([user_id], |row| {
            Ok(LogPullEntry {
                id: row.get(0)?,
                ingredient_id: row.get(1)?,
                date: row.get(2)?,
                slot: row.get(3)?,
                grams: row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                unit: row.get(5)?,
                calories_per_100g: row.get(6)?,
                logged_at: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
                nutrients: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut nstmt = conn.prepare(
        "SELECT name, amount_per_100g, unit FROM log_entry_nutrient WHERE log_entry_id = ?1",
    )?;
    let mut entries = Vec::with_capacity(base.len());
    for mut e in base {
        e.nutrients = nstmt
            .query_map([&e.id], |row| {
                Ok(LogPullNutrient {
                    name: row.get(0)?,
                    amount_per_100g: row.get(1)?,
                    unit: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        entries.push(e);
    }
    // The viewer's day-supplement records (all dates, incl. the migration floor) — private per-day data
    // riding the same authed channel as the entries.
    let mut sstmt = conn.prepare(
        "SELECT date, b12, vit_d, algae_oil FROM day_supplements WHERE user_id = ?1 ORDER BY date",
    )?;
    let supplements = sstmt
        .query_map([user_id], |row| {
            Ok(DaySupplementsRecord {
                date: row.get(0)?,
                b12: row.get::<_, Option<bool>>(1)?.unwrap_or(false),
                vit_d: row.get::<_, Option<bool>>(2)?.unwrap_or(false),
                algae_oil: row.get::<_, Option<bool>>(3)?.unwrap_or(false),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(LogPull {
        entries,
        supplements,
    })
}

/// Replace the viewer's LOCAL diary with the authoritative server pull, applied VERBATIM (the frozen
/// snapshots are inserted as-is, never recomputed — the server's snapshot wins). Wipes ONLY this user's
/// `log_entries` (+ their `amounts`; `log_entry_nutrient` cascades) and re-inserts. Deliberately its own
/// reconciliation, NOT part of `apply_pull`: that wipes the shared `amounts` table wholesale, which the
/// diary references, so folding the two would corrupt it. The caller manages foreign-key enforcement.
pub fn apply_log_pull(conn: &Connection, user_id: &str, pull: &LogPull) -> Result<(), Error> {
    // Capture the old entries' amount ids to clean them up after the rebuild (amounts is shared, so we
    // delete only the ones this diary owned — never the whole table).
    let old_amount_ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT amount_id FROM log_entries WHERE user_id = ?1")?;
        let ids = stmt
            .query_map([user_id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids
    };
    conn.execute("DELETE FROM log_entries WHERE user_id = ?1", [user_id])?; // cascades log_entry_nutrient
    for aid in &old_amount_ids {
        conn.execute("DELETE FROM amounts WHERE id = ?1", [aid])?;
    }
    for e in &pull.entries {
        let amount_id = new_id();
        conn.execute(
            "INSERT INTO amounts(id, grams, unit, amount, preferred) VALUES (?1, ?2, ?3, 1, 'grams')",
            params![amount_id, e.grams, e.unit.as_deref().unwrap_or("")],
        )?;
        conn.execute(
            "INSERT INTO log_entries(id, user_id, date, slot, ingredient_id, amount_id, calories_per_100g, logged_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                e.id,
                user_id,
                e.date,
                e.slot.as_deref(),
                e.ingredient_id,
                amount_id,
                e.calories_per_100g,
                e.logged_at
            ],
        )?;
        for n in &e.nutrients {
            conn.execute(
                "INSERT INTO log_entry_nutrient(id, log_entry_id, name, amount_per_100g, unit)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![new_id(), e.id, n.name, n.amount_per_100g, n.unit],
            )?;
        }
    }
    // Rebuild the viewer's day-supplement records verbatim (server is authoritative) — wipe only this
    // user's rows, then re-insert. `day_supplements` references only the user, so no amounts cleanup.
    conn.execute("DELETE FROM day_supplements WHERE user_id = ?1", [user_id])?;
    let now = now_ms();
    for s in &pull.supplements {
        conn.execute(
            "INSERT INTO day_supplements(user_id, date, b12, vit_d, algae_oil, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![user_id, s.date, s.b12, s.vit_d, s.algae_oil, now],
        )?;
    }
    Ok(())
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
                    i.visibility, i.user_id, i.slug, i.deleted_at IS NOT NULL, u.username, sa.unit
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
                    row.get::<_, Option<String>>(11)?,
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
        serving_unit,
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
            serving_unit,
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

// ================================================================================================
// NUTRITION PROFILE + PERSONALIZED VEGAN-AWARE TARGETS (P1.3)
//
// PRIVATE per-user data, exactly like the diary above: a profile selects which Dietary Reference
// Intake (DRI) column to read, and `targets()` turns it into per-nutrient daily goals. TWO things
// make these targets different from the FDA "%DV" shown on recipe/ingredient labels (a separate,
// standardized concept — NEVER conflate the two):
//   1. They are PERSONALIZED to age / sex / weight / pregnancy / lactation via the IOM/NASEM DRIs.
//   2. They carry a VEGAN OVERLAY that no mainstream tracker ships by default — raising iron and
//      zinc for the lower bioavailability of plant (non-heme / high-phytate) sources, nudging
//      protein for plant digestibility, and modelling B12 / vitamin D / omega-3 as supplement-aware.
//
// EVERY constant below is transcribed from a primary source (NIH Office of Dietary Supplements
// health-professional fact sheets, which tabulate the IOM/NASEM DRIs) and cited inline. Amounts are
// expressed in the SAME unit the ingredient catalog stores that nutrient in (see the NUTRIENTS map in
// crates/usda-importer), so a `NutrientTotal` compares to its target with no unit conversion — e.g.
// vitamin D is in IU because USDA data is, not µg. Verified against the sources on 2026-07-21.
// ================================================================================================

/// Which DRI reference column to read. Male and female adult nutrient requirements genuinely differ
/// (iron, zinc, calcium, …), so a target must know which table applies. This is a NUTRITION parameter,
/// NOT a statement of gender identity: it is optional, falls back to a protective generic tier when
/// unset, and any individual target stays overridable.
#[derive(Serialize, Deserialize, Type, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum DriSex {
    /// Read the male DRI column.
    Male,
    /// Read the female DRI column.
    Female,
}

impl DriSex {
    fn as_str(self) -> &'static str {
        match self {
            DriSex::Male => "male",
            DriSex::Female => "female",
        }
    }
    fn from_db(s: &str) -> Option<Self> {
        match s {
            "male" => Some(DriSex::Male),
            "female" => Some(DriSex::Female),
            _ => None,
        }
    }
}

/// The per-user nutrition profile. Every field is OPTIONAL — an empty profile (or an absent field)
/// yields the generic-adult target tier, so targets ALWAYS exist. PRIVATE: only ever read/written by
/// its owner, never listed or in the anonymous pull. Doubles as the write payload (`save_profile`) and
/// the read shape (`get_profile`); `#[serde(default)]` lets a partial `{}` deserialize.
#[derive(Serialize, Deserialize, Type, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NutritionProfile {
    /// Birth year → coarse DRI age bracket (19–50 / 51–70 / 71+). None ⇒ the 19–50 adult tier.
    pub birth_year: Option<i64>,
    /// Which DRI column to read. None ⇒ the protective generic tier (max of the sexes, per nutrient).
    pub dri_sex: Option<DriSex>,
    /// Body weight (kg) for the protein g/kg target. None ⇒ the reference-weight gram RDA.
    pub weight_kg: Option<f64>,
    /// Pregnancy raises the iron / iodine / zinc / B12 / selenium DRIs; overrides the sex column.
    pub pregnancy: bool,
    /// Lactation DRIs (distinct from pregnancy); overrides the sex column.
    pub lactation: bool,
}

/// The supplements taken on a given DAY — the vegan-critical ones whose targets read as "covered by a
/// supplement" rather than a food gap. This is per-DAY state (part of the day's plan), NOT a standing
/// profile setting: whether you took your B12 today is a fact about today, and coverage should reflect
/// it honestly. Carry-forward (`get_day_supplements`) makes a new day inherit your last day's routine so
/// you don't re-check the same boxes every morning. Every field defaults to false (nothing taken).
#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct DaySupplements {
    /// Took a B12 supplement (or reliably ate fortified foods) ⇒ the B12 target shows as covered.
    pub b12: bool,
    /// Took a vitamin D supplement ⇒ the vitamin D target shows as covered.
    pub vit_d: bool,
    /// Took an algae-oil (EPA+DHA) supplement ⇒ the omega-3 note reflects it.
    pub algae_oil: bool,
}

/// Whether a target is an RDA (meets ~97–98% of the population's needs) or an AI (Adequate Intake,
/// used where the evidence can't set an RDA — e.g. omega-3 ALA). Surfaced so the UI can be honest.
#[derive(Serialize, Deserialize, Type, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TargetBasis {
    /// Recommended Dietary Allowance — meets the needs of ~97–98% of healthy people.
    Rda,
    /// Adequate Intake — used where the evidence can't establish an RDA (e.g. omega-3 ALA).
    Ai,
}

/// One personalized daily target for a nutrient. `name`/`unit` match the ingredient catalog's naming
/// (crates/usda-importer NUTRIENTS) so a day `NutrientTotal` compares to it directly.
#[derive(Serialize, Deserialize, Type, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NutrientTarget {
    /// Canonical nutrient name, matching `NutrientTotal.name` (e.g. "Iron", "Vitamin B12").
    pub name: String,
    /// The personalized daily goal, in `unit`.
    pub amount: f64,
    /// Unit — the catalog's storage unit for this nutrient ("mg" | "µg" | "g" | "IU").
    pub unit: String,
    /// RDA vs AI, for honest labelling.
    pub basis: TargetBasis,
    /// True when the vegan bioavailability overlay raised this above the plain DRI (iron, zinc, protein).
    pub vegan_adjusted: bool,
    /// True when a logged supplement flag covers this nutrient (B12 / vitamin D) — display it as met by
    /// supplement rather than as a food gap.
    pub supplement_covered: bool,
    /// Short guidance note (vegan-specific context). Guidance-toned, never shaming. None = no note.
    pub note: Option<String>,
}

/// Adult DRI age brackets (this app targets adults; a <19 age folds into the 19–50 tier).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AgeBracket {
    A19to50,
    A51to70,
    A71plus,
}

fn age_bracket(birth_year: Option<i64>, current_year: i64) -> AgeBracket {
    match birth_year {
        Some(by) => {
            let age = current_year - by;
            if age >= 71 {
                AgeBracket::A71plus
            } else if age >= 51 {
                AgeBracket::A51to70
            } else {
                AgeBracket::A19to50
            }
        }
        None => AgeBracket::A19to50,
    }
}

/// Round to one decimal place (targets read as e.g. 14.4 mg, not 14.4000000001).
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Compute this profile's personalized daily nutrient targets, with the vegan overlay. `current_year`
/// resolves the age bracket from `birth_year` (callers pass the year of the day in view / today), so
/// the function stays pure and clock-free.
///
/// Coverage is the "vegan-critical" set the roadmap calls out — the nutrients where a plant-based diet
/// meaningfully changes the target or the risk. Nutrients the catalog tracks but that need no personal
/// adjustment (most B-vitamins, magnesium, potassium, …) are intentionally NOT targeted here yet; the
/// FDA %DV panel still covers them. Extending coverage = adding to this list with cited constants.
pub fn targets(
    profile: &NutritionProfile,
    supplements: &DaySupplements,
    current_year: i64,
) -> Vec<NutrientTarget> {
    let sex = profile.dri_sex;
    let bracket = age_bracket(profile.birth_year, current_year);
    let preg = profile.pregnancy;
    let lact = profile.lactation;

    // Pick a sex-specific value. When sex is unset, use the MAX of the two adult values so the generic
    // target never UNDER-recommends for either sex (the protective default for an adequacy tracker).
    // Pregnancy/lactation values, when set, are applied at each nutrient regardless of `sex`.
    let by_sex = |male: f64, female: f64| -> f64 {
        match sex {
            Some(DriSex::Male) => male,
            Some(DriSex::Female) => female,
            None => male.max(female),
        }
    };

    let mut out = Vec::new();

    // IRON — RDA mg/day. IOM (2001) DRI; NIH ODS Iron (Table 2): men 19+ 8; women 19–50 18, women 51+ 8;
    // pregnancy 27; lactation 9. VEGAN OVERLAY ×1.8: "The requirement for iron is 1.8 times higher for
    // people who follow vegetarian diets … because heme iron from meat is more bioavailable than nonheme
    // iron from plant-based foods" (NIH ODS Iron; IOM 2001). A vegan eats only non-heme iron, so it
    // applies to every profile.
    {
        let base = if preg {
            27.0
        } else if lact {
            9.0
        } else if bracket == AgeBracket::A19to50 {
            by_sex(8.0, 18.0)
        } else {
            8.0 // men 8 at every age; women drop to 8 post-menopause (51+)
        };
        out.push(NutrientTarget {
            name: "Iron".into(),
            amount: round1(base * 1.8),
            unit: "mg".into(),
            basis: TargetBasis::Rda,
            vegan_adjusted: true,
            supplement_covered: false,
            note: Some(
                "Plant (non-heme) iron absorbs less readily, so the target is 1.8× the standard RDA. \
                 Pairing iron-rich foods with vitamin C boosts absorption."
                    .into(),
            ),
        });
    }

    // ZINC — RDA mg/day. IOM (2001) DRI; NIH ODS Zinc (Table 1): men 11; women 8; pregnancy 11;
    // lactation 12. VEGAN OVERLAY ×1.5: the IOM (2001) DRI notes zinc requirements may be "as much as
    // 50%" greater for vegetarians whose diets are high in phytate (which binds zinc); NIH ODS Zinc
    // confirms the phytate mechanism and that vegetarians run lower zinc intake/status. Applied to all.
    {
        let base = if preg {
            11.0
        } else if lact {
            12.0
        } else {
            by_sex(11.0, 8.0)
        };
        out.push(NutrientTarget {
            name: "Zinc".into(),
            amount: round1(base * 1.5),
            unit: "mg".into(),
            basis: TargetBasis::Rda,
            vegan_adjusted: true,
            supplement_covered: false,
            note: Some(
                "Phytates in grains and legumes lower zinc absorption, so the target adds ~50% over the \
                 RDA. Soaking, sprouting, and leavening (sourdough) help release it."
                    .into(),
            ),
        });
    }

    // VITAMIN B12 — RDA µg/day. IOM (1998) DRI; NIH ODS Vitamin B12 (Table 1): adults 2.4; pregnancy 2.6;
    // lactation 2.8. NO bioavailability multiplier — the vegan issue is ABSENCE, not absorption ("natural
    // food sources of vitamin B12 are limited to animal foods"; fortified foods/supplements "substantially
    // reduce the risk of deficiency", NIH ODS). We surface supplement coverage instead of inflating it.
    {
        let base = if preg {
            2.6
        } else if lact {
            2.8
        } else {
            2.4
        };
        out.push(NutrientTarget {
            name: "Vitamin B12".into(),
            amount: base,
            unit: "µg".into(),
            basis: TargetBasis::Rda,
            vegan_adjusted: false,
            supplement_covered: supplements.b12,
            note: Some(if supplements.b12 {
                "Covered by your B12 supplement — the non-negotiable one for vegans (no reliable plant \
                 food source)."
                    .into()
            } else {
                "B12 has no reliable plant food source. A supplement or consistently fortified foods \
                 (nutritional yeast, fortified plant milk) are essential on a vegan diet."
                    .into()
            }),
        });
    }

    // CALCIUM — RDA mg/day. IOM (2011) DRI; NIH ODS Calcium (Table 1): men 19–70 1000, men 71+ 1200;
    // women 19–50 1000, women 51+ 1200; pregnancy/lactation 1000 (adults). No vegan multiplier (the DRI
    // is the same) — but plant-based intake often runs low, so we flag it as a watch nutrient.
    {
        let base = if preg || lact {
            1000.0
        } else {
            match (sex, bracket) {
                (_, AgeBracket::A71plus) => 1200.0,
                (Some(DriSex::Female), AgeBracket::A51to70) => 1200.0,
                (None, AgeBracket::A51to70) => 1200.0, // protective generic (women 51–70 = 1200)
                _ => 1000.0,
            }
        };
        out.push(NutrientTarget {
            name: "Calcium".into(),
            amount: base,
            unit: "mg".into(),
            basis: TargetBasis::Rda,
            vegan_adjusted: false,
            supplement_covered: false,
            note: Some(
                "Calcium-set tofu, fortified plant milks, tahini, almonds, and low-oxalate greens (kale, \
                 bok choy) are dependable plant sources."
                    .into(),
            ),
        });
    }

    // IODINE — RDA µg/day. IOM (2001) DRI; NIH ODS Iodine (Table 1): adults 150; pregnancy 220;
    // lactation 290. No multiplier, but vegans are a named at-risk group ("Vegans … might not obtain
    // sufficient amounts of iodine"), and seaweed iodine is wildly variable — iodized salt or a
    // supplement is the reliable route (NIH ODS Iodine). NOTE: USDA plant data carries no iodine, so
    // this target usually reads 0 from food logs today — which is itself honest and a reason to supplement.
    {
        let base = if preg {
            220.0
        } else if lact {
            290.0
        } else {
            150.0
        };
        out.push(NutrientTarget {
            name: "Iodine".into(),
            amount: base,
            unit: "µg".into(),
            basis: TargetBasis::Rda,
            vegan_adjusted: false,
            supplement_covered: false,
            note: Some(
                "Use iodized salt or a supplement — seaweed iodine varies enormously and most plant \
                 foods carry little."
                    .into(),
            ),
        });
    }

    // VITAMIN D — RDA IU/day (the catalog/USDA store vitamin D in IU; 1 µg = 40 IU). IOM (2011) DRI;
    // NIH ODS Vitamin D: ages 19–70 600 IU (15 µg); 71+ 800 IU (20 µg); pregnancy/lactation 600 IU. Few
    // plant foods carry vitamin D (sun, fortified foods, or a supplement), so it's supplement-aware.
    {
        let base = if bracket == AgeBracket::A71plus && !preg && !lact {
            800.0
        } else {
            600.0
        };
        out.push(NutrientTarget {
            name: "Vitamin D".into(),
            amount: base,
            unit: "IU".into(),
            basis: TargetBasis::Rda,
            vegan_adjusted: false,
            supplement_covered: supplements.vit_d,
            note: Some(if supplements.vit_d {
                "Covered by your vitamin D supplement (a vegan D3 from lichen, or D2)."
                    .into()
            } else {
                "Sunlight, fortified foods, or a vegan D3 (lichen) / D2 supplement — plant foods carry \
                 little vitamin D."
                    .into()
            }),
        });
    }

    // SELENIUM — RDA µg/day. IOM (2000) DRI; NIH ODS Selenium (Table 1): adults 55; pregnancy 60;
    // lactation 70. No vegan multiplier; intake tracks soil selenium, and a couple of Brazil nuts cover it.
    {
        let base = if preg {
            60.0
        } else if lact {
            70.0
        } else {
            55.0
        };
        out.push(NutrientTarget {
            name: "Selenium".into(),
            amount: base,
            unit: "µg".into(),
            basis: TargetBasis::Rda,
            vegan_adjusted: false,
            supplement_covered: false,
            note: Some("A couple of Brazil nuts a day easily covers selenium.".into()),
        });
    }

    // OMEGA-3 (ALA) — AI g/day (an Adequate Intake, not an RDA — the evidence can't set an RDA). IOM
    // (2002/2005) Macronutrients DRI; NIH ODS Omega-3 (Table 1): men 1.6; women 1.1; pregnancy 1.4;
    // lactation 1.3. ALA→EPA/DHA conversion is limited (<15%, NIH ODS), so we surface an algae-oil
    // (direct EPA+DHA) note. Target name matches the USDA nutrient "Omega-3 Fatty Acids" (18:3 n-3, ALA).
    {
        let base = if preg {
            1.4
        } else if lact {
            1.3
        } else {
            by_sex(1.6, 1.1)
        };
        out.push(NutrientTarget {
            name: "Omega-3 Fatty Acids".into(),
            amount: base,
            unit: "g".into(),
            basis: TargetBasis::Ai,
            vegan_adjusted: false,
            supplement_covered: supplements.algae_oil,
            note: Some(if supplements.algae_oil {
                "ALA from flax, chia, hemp, and walnuts, plus your algae-oil supplement for direct EPA+DHA."
                    .into()
            } else {
                "Flax, chia, hemp, and walnuts supply ALA; the body converts little to EPA/DHA, so many \
                 vegans add algae oil."
                    .into()
            }),
        });
    }

    // PROTEIN — RDA. IOM (2005) Macronutrients DRI: 0.8 g/kg body weight/day for adults (reference-weight
    // RDAs 56 g men / 46 g women). VEGAN OVERLAY: plant proteins average lower digestibility (DIAAS) and
    // some run low in lysine, so a modest bump is prudent — we target 1.0 g/kg when a weight is known (the
    // conservative low end of the commonly-cited 1.0–1.2 g/kg plant range; the 0.8 RDA assumes mixed
    // high-quality protein). Without a weight we fall back to the reference gram RDA and DON'T claim the
    // vegan adjustment (no g/kg is possible).
    {
        let (amount, vegan_adjusted, note) = match profile.weight_kg {
            Some(kg) if kg > 0.0 => (
                round1(kg * 1.0),
                true,
                "Set to 1.0 g/kg — a little above the 0.8 g/kg RDA to offset plant protein's lower \
                 digestibility. Legumes, soy, seitan, and grains together cover all amino acids."
                    .to_string(),
            ),
            _ => (
                by_sex(56.0, 46.0),
                false,
                "Add your weight in Settings for a body-weight-based protein target (plant-adjusted). \
                 Legumes, soy, and grains together supply complete protein."
                    .to_string(),
            ),
        };
        out.push(NutrientTarget {
            name: "Protein".into(),
            amount,
            unit: "g".into(),
            basis: TargetBasis::Rda,
            vegan_adjusted,
            supplement_covered: false,
            note: Some(note),
        });
    }

    out
}

/// Read a user's nutrition profile. Returns a default (all-None/false ⇒ generic-adult targets) when no
/// row exists, so callers never branch on presence. PRIVATE — owner-scoped by the caller's auth.
pub fn get_nutrition_profile(conn: &Connection, user_id: &str) -> Result<NutritionProfile, Error> {
    let p = conn
        .query_row(
            "SELECT birth_year, dri_sex, weight_kg, pregnancy, lactation
             FROM profiles WHERE user_id = ?1",
            [user_id],
            |row| {
                Ok(NutritionProfile {
                    birth_year: row.get(0)?,
                    dri_sex: row
                        .get::<_, Option<String>>(1)?
                        .as_deref()
                        .and_then(DriSex::from_db),
                    weight_kg: row.get(2)?,
                    pregnancy: row.get::<_, Option<bool>>(3)?.unwrap_or(false),
                    lactation: row.get::<_, Option<bool>>(4)?.unwrap_or(false),
                })
            },
        )
        .optional()?;
    Ok(p.unwrap_or_default())
}

/// Upsert a user's nutrition profile (the single row per user). Owner-scoped by construction — the
/// caller passes the authenticated `user_id`.
pub fn save_nutrition_profile(
    conn: &Connection,
    user_id: &str,
    input: &NutritionProfile,
) -> Result<(), Error> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO profiles
            (user_id, birth_year, dri_sex, weight_kg, pregnancy, lactation, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(user_id) DO UPDATE SET
            birth_year = excluded.birth_year,
            dri_sex = excluded.dri_sex,
            weight_kg = excluded.weight_kg,
            pregnancy = excluded.pregnancy,
            lactation = excluded.lactation,
            updated_at = excluded.updated_at",
        params![
            user_id,
            input.birth_year,
            input.dri_sex.map(|s| s.as_str()),
            input.weight_kg,
            input.pregnancy,
            input.lactation,
            now,
        ],
    )?;
    Ok(())
}

/// The effective supplements for a user on a date, with CARRY-FORWARD: the row for exactly `date` if one
/// exists, else the most recent row on an EARLIER date (your standing routine), else all-false. So a new
/// day inherits what you last recorded — no re-checking the same boxes daily — and an explicit edit to a
/// day sets that day (and, by carry-forward, later days without their own row). The migration seeds a
/// floor row (date '1970-01-01') from any old profile supplement flags, so pre-existing coverage holds.
/// PRIVATE — owner-scoped by the caller's auth, exactly like the diary.
pub fn get_day_supplements(
    conn: &Connection,
    user_id: &str,
    date: &str,
) -> Result<DaySupplements, Error> {
    let s = conn
        .query_row(
            "SELECT b12, vit_d, algae_oil FROM day_supplements
             WHERE user_id = ?1 AND date <= ?2 ORDER BY date DESC LIMIT 1",
            params![user_id, date],
            |row| {
                Ok(DaySupplements {
                    b12: row.get::<_, Option<bool>>(0)?.unwrap_or(false),
                    vit_d: row.get::<_, Option<bool>>(1)?.unwrap_or(false),
                    algae_oil: row.get::<_, Option<bool>>(2)?.unwrap_or(false),
                })
            },
        )
        .optional()?;
    Ok(s.unwrap_or_default())
}

/// Upsert the supplements taken on a specific `date` for a user (one row per user+date). Owner-scoped by
/// construction — the caller passes the authenticated `user_id`. Writing an explicit row for a date
/// pins that day (carry-forward flows from it to later days without their own row).
pub fn save_day_supplements(
    conn: &Connection,
    user_id: &str,
    date: &str,
    input: &DaySupplements,
) -> Result<(), Error> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO day_supplements
            (user_id, date, b12, vit_d, algae_oil, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(user_id, date) DO UPDATE SET
            b12 = excluded.b12,
            vit_d = excluded.vit_d,
            algae_oil = excluded.algae_oil,
            updated_at = excluded.updated_at",
        params![user_id, date, input.b12, input.vit_d, input.algae_oil, now,],
    )?;
    Ok(())
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
    let rows: Vec<(String, String, Option<f64>, Option<String>)> = {
        let mut stmt = conn.prepare(
            "SELECT i.id, i.name, sa.grams, sa.unit
             FROM ingredients i
             LEFT JOIN amounts sa ON sa.id = i.serving_size_id
             WHERE i.name LIKE ?1 AND i.deleted_at IS NULL
               AND (i.visibility = 'public' OR i.user_id = ?2)
             ORDER BY i.name LIMIT 20",
        )?;
        let v = stmt
            .query_map(params![like, viewer], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    let mut out = Vec::new();
    for (id, name, serving_grams, serving_unit) in rows {
        let nut = aggregate_per100g(conn, &id)?;
        out.push(IngredientSearchResult {
            id,
            name,
            serving_grams,
            serving_unit,
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

    let rows: Vec<(String, String, f64, f64, Option<String>, String)> = {
        let mut stmt = conn.prepare(
            "SELECT i.id, i.name, a.grams, a.amount, a.unit, a.preferred
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
                    r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                    r.get(4)?,
                    r.get::<_, Option<String>>(5)?
                        .unwrap_or_else(|| "grams".into()),
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    let mut items = Vec::new();
    for (ingredient_id, iname, grams, amount, unit, preferred) in rows {
        let nut = aggregate_per100g(conn, &ingredient_id)?;
        items.push(RecipeEditItem {
            ingredient_id,
            name: iname,
            grams,
            amount,
            unit,
            preferred,
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
                serving_unit: None,
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
                    amount: None,
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp, missing_docs)] // test code: the asserts ARE the checks
mod log_tests {
    use super::*;

    /// The REAL client schema (drift-test-pinned to the drizzle source), so the test DB has log_entries.
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

    fn ingredient_with(
        c: &Connection,
        name: &str,
        calories: Option<f64>,
        nutrients: Vec<(&str, f64, &str)>,
    ) -> String {
        do_save_ingredient(
            c,
            &SaveIngredientInput {
                id: None,
                visibility: Some(Visibility::Public),
                name: name.into(),
                description: None,
                price: None,
                calories_per_100g: calories,
                serving_grams: None,
                serving_unit: None,
                package_grams: None,
                nutrients: nutrients
                    .into_iter()
                    .map(|(n, a, u)| IngredientNutrientInput {
                        name: n.into(),
                        amount_per_100g: a,
                        unit: u.into(),
                    })
                    .collect(),
                slug: None,
            },
            Some("u1"),
        )
        .unwrap()
    }

    fn log(c: &Connection, ingredient_id: &str, date: &str, grams: f64, logged_at: i64) -> String {
        do_save_log_entry(
            c,
            &SaveLogEntryInput {
                id: None,
                ingredient_id: ingredient_id.into(),
                date: date.into(),
                slot: None,
                grams,
                unit: None,
                logged_at: Some(logged_at),
            },
            "u1",
        )
        .unwrap()
    }

    fn total(day: &DayLog, name: &str) -> f64 {
        day.totals
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.amount)
            .unwrap_or(0.0)
    }

    #[test]
    fn day_totals_sum_a_leaf_and_a_nested_recipe_entry() {
        let c = conn();
        // Tofu: 100 cal/100g, 10 g protein/100g, 5 mg iron/100g. Oil: 900 cal/100g, no micros.
        let tofu = ingredient_with(
            &c,
            "Tofu",
            Some(100.0),
            vec![("Protein", 10.0, "g"), ("Iron", 5.0, "mg")],
        );
        let oil = ingredient_with(&c, "Oil", Some(900.0), vec![]);
        // Scramble = 100 g Tofu + 100 g Oil (batch 200 g) ⇒ per-100g: 500 cal, 5 g protein, 2.5 mg iron.
        let scramble = do_save_recipe(
            &c,
            &SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: Some(Visibility::Public),
                name: "Scramble".into(),
                subtitle: None,
                directions: None,
                serving_grams: None,
                batch_grams: None,
                items: vec![
                    RecipeItemInput {
                        ingredient_id: tofu.clone(),
                        grams: 100.0,
                        unit: None,
                        amount: None,
                    },
                    RecipeItemInput {
                        ingredient_id: oil.clone(),
                        grams: 100.0,
                        unit: None,
                        amount: None,
                    },
                ],
                slug: None,
            },
            Some("u1"),
        )
        .unwrap();
        let scramble_card: String = c
            .query_row(
                "SELECT as_ingredient_id FROM recipes WHERE id = ?1",
                [&scramble],
                |r| r.get(0),
            )
            .unwrap();

        // Log 150 g Tofu (leaf) and 200 g Scramble (nested recipe) on the same day.
        log(&c, &tofu, "2026-07-20", 150.0, 1000);
        log(&c, &scramble_card, "2026-07-20", 200.0, 2000);

        let day = log_day(&c, "u1", "2026-07-20").unwrap();

        // Newest-logged first: the scramble (logged_at 2000) precedes the tofu (1000).
        assert_eq!(day.entries.len(), 2);
        assert_eq!(day.entries[0].ingredient_id, scramble_card);
        assert_eq!(day.entries[0].name, "Scramble");
        assert_eq!(day.entries[0].recipe_id.as_deref(), Some(scramble.as_str()));
        assert_eq!(day.entries[1].name, "Tofu");
        assert!(day.entries[1].recipe_id.is_none());

        // Hand-computed: Tofu 150 g → 150 cal / 15 g protein / 7.5 mg iron;
        //                Scramble 200 g → 1000 cal / 10 g protein / 5 mg iron.
        let approx = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert!(
            approx(day.entries[0].calories.unwrap(), 1000.0),
            "scramble entry calories"
        );
        assert!(
            approx(day.entries[1].calories.unwrap(), 150.0),
            "tofu entry calories"
        );
        assert!(
            approx(day.calories.unwrap(), 1150.0),
            "day calories = 1150, got {:?}",
            day.calories
        );
        assert!(
            approx(total(&day, "Protein"), 25.0),
            "protein = 25 g, got {}",
            total(&day, "Protein")
        );
        assert!(
            approx(total(&day, "Iron"), 12.5),
            "iron = 12.5 mg, got {}",
            total(&day, "Iron")
        );
        // Totals come out ordered by nutrient name (the BTreeMap key order).
        assert_eq!(
            day.totals
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Iron", "Protein"]
        );
        // A different day is empty.
        assert!(log_day(&c, "u1", "2026-07-21").unwrap().entries.is_empty());
    }

    #[test]
    fn recents_are_distinct_newest_first_with_frequency_and_last_amount() {
        let c = conn();
        let tofu = ingredient_with(&c, "Tofu", Some(100.0), vec![]);
        let rice = ingredient_with(&c, "Rice", Some(130.0), vec![]);
        // Log tofu twice, rice once; tofu's latest is the newest overall.
        log(&c, &tofu, "2026-07-19", 100.0, 1000);
        log(&c, &rice, "2026-07-19", 200.0, 1500);
        log(&c, &tofu, "2026-07-20", 175.0, 3000);

        let recents = log_recents(&c, "u1", 10).unwrap();
        assert_eq!(recents.len(), 2, "distinct ingredients");
        assert_eq!(recents[0].ingredient_id, tofu, "tofu logged most recently");
        assert_eq!(recents[0].count, 2, "tofu logged twice");
        assert_eq!(recents[0].last_grams, 175.0, "prefill from the latest log");
        assert_eq!(recents[1].ingredient_id, rice);
        assert_eq!(recents[1].count, 1);
        assert_eq!(
            log_recents(&c, "u1", 1).unwrap().len(),
            1,
            "limit is honored"
        );
    }

    #[test]
    fn entries_are_owner_guarded_and_soft_deleted() {
        let c = conn();
        c.execute(
            "INSERT INTO users (id, name, email, username) VALUES ('u2','Bob','b@x','bob')",
            [],
        )
        .unwrap();
        let tofu = ingredient_with(&c, "Tofu", Some(100.0), vec![]);
        let id = log(&c, &tofu, "2026-07-20", 100.0, 1000);

        // Another user can neither edit nor delete it.
        let hijack = do_save_log_entry(
            &c,
            &SaveLogEntryInput {
                id: Some(id.clone()),
                ingredient_id: tofu.clone(),
                date: "2026-07-20".into(),
                slot: None,
                grams: 999.0,
                unit: None,
                logged_at: Some(1000),
            },
            "u2",
        );
        assert!(hijack.is_err(), "only the owner edits");
        assert!(
            do_delete_log_entry(&c, &id, "u2").is_err(),
            "only the owner deletes"
        );

        // Owner soft-deletes → it leaves the day, but the row survives tombstoned.
        do_delete_log_entry(&c, &id, "u1").unwrap();
        assert!(
            log_day(&c, "u1", "2026-07-20").unwrap().entries.is_empty(),
            "deleted entry leaves the day"
        );
        let tomb: bool = c
            .query_row(
                "SELECT deleted_at IS NOT NULL FROM log_entries WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(tomb, "soft-deleted, not destroyed");

        // Re-saving with the same id restores it (there's no separate restore for logs).
        do_save_log_entry(
            &c,
            &SaveLogEntryInput {
                id: Some(id.clone()),
                ingredient_id: tofu,
                date: "2026-07-20".into(),
                slot: None,
                grams: 120.0,
                unit: None,
                logged_at: Some(1000),
            },
            "u1",
        )
        .unwrap();
        assert_eq!(
            log_day(&c, "u1", "2026-07-20").unwrap().entries.len(),
            1,
            "re-save makes it live again"
        );
    }

    #[test]
    fn logging_rejects_bad_input_and_deleting_a_logged_ingredient_tombstones_it() {
        let c = conn();
        let tofu = ingredient_with(&c, "Tofu", Some(100.0), vec![]);
        let bad = |grams: f64, date: &str, ing: &str| {
            do_save_log_entry(
                &c,
                &SaveLogEntryInput {
                    id: None,
                    ingredient_id: ing.into(),
                    date: date.into(),
                    slot: None,
                    grams,
                    unit: None,
                    logged_at: None,
                },
                "u1",
            )
            .is_err()
        };
        assert!(bad(0.0, "2026-07-20", &tofu), "non-positive grams rejected");
        assert!(bad(100.0, "  ", &tofu), "empty date rejected");
        assert!(
            bad(100.0, "2026-07-20", "nope"),
            "unknown ingredient rejected"
        );

        // A logged (but recipe-unused) ingredient must SOFT-delete: the log_entries RESTRICT FK would
        // otherwise block a hard delete. History still resolves the name afterward.
        log(&c, &tofu, "2026-07-20", 100.0, 1000);
        do_delete_ingredient(&c, &tofu, Some("u1")).unwrap();
        let (alive, tomb): (i64, bool) = c
            .query_row(
                "SELECT COUNT(*), MAX(deleted_at IS NOT NULL) FROM ingredients WHERE id = ?1",
                [&tofu],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (alive, tomb),
            (1, true),
            "logged ingredient tombstones, not destroyed"
        );
        assert_eq!(
            log_day(&c, "u1", "2026-07-20").unwrap().entries[0].name,
            "Tofu",
            "history still resolves the tombstoned ingredient"
        );
    }

    fn edit_ingredient(
        c: &Connection,
        id: &str,
        name: &str,
        calories: Option<f64>,
        nutrients: Vec<(&str, f64, &str)>,
    ) {
        do_save_ingredient(
            c,
            &SaveIngredientInput {
                id: Some(id.to_string()),
                visibility: Some(Visibility::Public),
                name: name.into(),
                description: None,
                price: None,
                calories_per_100g: calories,
                serving_grams: None,
                serving_unit: None,
                package_grams: None,
                nutrients: nutrients
                    .into_iter()
                    .map(|(n, a, u)| IngredientNutrientInput {
                        name: n.into(),
                        amount_per_100g: a,
                        unit: u.into(),
                    })
                    .collect(),
                slug: None,
            },
            Some("u1"),
        )
        .unwrap();
    }

    #[test]
    fn a_source_edit_never_moves_a_past_days_snapshot() {
        let c = conn();
        // Oats: 100 cal/100g, 5 mg iron/100g. Log 200 g → 200 cal, 10 mg iron (frozen at log time).
        let oats = ingredient_with(&c, "Oats", Some(100.0), vec![("Iron", 5.0, "mg")]);
        log(&c, &oats, "2026-07-20", 200.0, 1000);
        let before = log_day(&c, "u1", "2026-07-20").unwrap();
        let approx = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert!(approx(before.calories.unwrap(), 200.0));
        assert!(approx(total(&before, "Iron"), 10.0));

        // Passively rewrite the SOURCE ingredient: 100→500 cal, iron 5→50 mg.
        edit_ingredient(&c, &oats, "Oats", Some(500.0), vec![("Iron", 50.0, "mg")]);

        // THE FIX: the past day is UNCHANGED — it reads the frozen snapshot, not the live ingredient.
        let after = log_day(&c, "u1", "2026-07-20").unwrap();
        assert!(
            approx(after.calories.unwrap(), 200.0),
            "calories held at 200, got {:?}",
            after.calories
        );
        assert!(
            approx(total(&after, "Iron"), 10.0),
            "iron held at 10 mg, got {}",
            total(&after, "Iron")
        );

        // A NEW log of the same food captures the CURRENT values (each entry snapshots at its own log time).
        log(&c, &oats, "2026-07-21", 100.0, 2000);
        let fresh = log_day(&c, "u1", "2026-07-21").unwrap();
        assert!(
            approx(fresh.calories.unwrap(), 500.0),
            "a fresh log uses the current values"
        );
        assert!(approx(total(&fresh, "Iron"), 50.0));
    }

    #[test]
    fn editing_grams_rescales_the_frozen_snapshot_but_changing_the_food_re_snapshots() {
        let c = conn();
        let oats = ingredient_with(&c, "Oats", Some(100.0), vec![("Iron", 5.0, "mg")]);
        let rice = ingredient_with(&c, "Rice", Some(130.0), vec![("Iron", 1.0, "mg")]);
        let id = log(&c, &oats, "2026-07-20", 200.0, 1000); // frozen at 100 cal, 5 mg iron / 100 g
        let approx = |a: f64, b: f64| (a - b).abs() < 1e-6;

        // Poison the source, THEN edit ONLY the grams (200 → 50). The frozen profile is used, not the
        // poisoned live one: 50 g → 50 cal, 2.5 mg iron.
        edit_ingredient(&c, &oats, "Oats", Some(999.0), vec![("Iron", 99.0, "mg")]);
        do_save_log_entry(
            &c,
            &SaveLogEntryInput {
                id: Some(id.clone()),
                ingredient_id: oats.clone(),
                date: "2026-07-20".into(),
                slot: None,
                grams: 50.0,
                unit: None,
                logged_at: Some(1000),
            },
            "u1",
        )
        .unwrap();
        let d = log_day(&c, "u1", "2026-07-20").unwrap();
        assert!(
            approx(d.calories.unwrap(), 50.0),
            "a grams edit rescales the FROZEN profile, got {:?}",
            d.calories
        );
        assert!(approx(total(&d, "Iron"), 2.5), "iron {}", total(&d, "Iron"));

        // Now change the FOOD itself (oats → rice). THAT re-snapshots to rice's current values:
        // 100 g → 130 cal, 1 mg iron.
        do_save_log_entry(
            &c,
            &SaveLogEntryInput {
                id: Some(id.clone()),
                ingredient_id: rice.clone(),
                date: "2026-07-20".into(),
                slot: None,
                grams: 100.0,
                unit: None,
                logged_at: Some(1000),
            },
            "u1",
        )
        .unwrap();
        let d2 = log_day(&c, "u1", "2026-07-20").unwrap();
        assert!(
            approx(d2.calories.unwrap(), 130.0),
            "changing the food re-snapshots, got {:?}",
            d2.calories
        );
        assert!(approx(total(&d2, "Iron"), 1.0));
    }

    #[test]
    fn diary_pull_round_trips_and_applies_the_snapshot_verbatim() {
        let c = conn();
        let beans = ingredient_with(&c, "Beans", Some(100.0), vec![("Iron", 5.0, "mg")]);
        log(&c, &beans, "2026-07-20", 200.0, 1000); // frozen: 100 cal, 5 mg iron / 100 g

        // Pull the whole diary — it carries the entry + its FROZEN snapshot.
        let pull = log_pull(&c, "u1").unwrap();
        assert_eq!(pull.entries.len(), 1);
        assert_eq!(pull.entries[0].ingredient_id, beans);
        assert_eq!(pull.entries[0].calories_per_100g, Some(100.0));
        assert_eq!(pull.entries[0].nutrients.len(), 1, "iron snapshot carried");

        // Poison the SOURCE ingredient AFTER the pull was taken.
        edit_ingredient(&c, &beans, "Beans", Some(999.0), vec![("Iron", 99.0, "mg")]);

        // Apply the pull VERBATIM (as a replica would). It must use the pull's frozen snapshot, NOT
        // recompute from the now-poisoned live ingredient.
        apply_log_pull(&c, "u1", &pull).unwrap();
        let day = log_day(&c, "u1", "2026-07-20").unwrap();
        let approx = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert_eq!(day.entries.len(), 1, "the entry round-tripped");
        assert!(
            approx(day.calories.unwrap(), 200.0),
            "verbatim snapshot (200 g × 100/100), NOT the poisoned 1998: {:?}",
            day.calories
        );
        assert!(
            approx(total(&day, "Iron"), 10.0),
            "verbatim iron (200 g × 5/100), NOT the poisoned 198: {}",
            total(&day, "Iron")
        );

        // An empty pull clears the viewer's diary (a signed-out/empty server view, or a wipe).
        apply_log_pull(
            &c,
            "u1",
            &LogPull {
                entries: vec![],
                supplements: vec![],
            },
        )
        .unwrap();
        assert!(
            log_day(&c, "u1", "2026-07-20").unwrap().entries.is_empty(),
            "an empty pull clears the diary"
        );
    }

    #[test]
    fn day_supplements_carry_forward_and_round_trip_the_pull() {
        let c = conn();
        // No records yet ⇒ nothing taken on any day.
        assert_eq!(
            get_day_supplements(&c, "u1", "2026-07-20").unwrap(),
            DaySupplements::default()
        );

        // Record B12 on the 18th. It carries FORWARD to later days without their own row…
        save_day_supplements(
            &c,
            "u1",
            "2026-07-18",
            &DaySupplements {
                b12: true,
                vit_d: false,
                algae_oil: false,
            },
        )
        .unwrap();
        assert!(get_day_supplements(&c, "u1", "2026-07-20").unwrap().b12);
        // …but NOT backward to an earlier day.
        assert!(!get_day_supplements(&c, "u1", "2026-07-17").unwrap().b12);

        // An explicit later row pins that day and overrides the carry-forward from there on.
        save_day_supplements(
            &c,
            "u1",
            "2026-07-20",
            &DaySupplements {
                b12: false,
                vit_d: true,
                algae_oil: false,
            },
        )
        .unwrap();
        let d20 = get_day_supplements(&c, "u1", "2026-07-20").unwrap();
        assert!(!d20.b12 && d20.vit_d);
        // The 19th still carries the 18th's B12 (no row of its own).
        assert!(get_day_supplements(&c, "u1", "2026-07-19").unwrap().b12);

        // The pull carries every record; apply rebuilds them verbatim on a replica.
        let pull = log_pull(&c, "u1").unwrap();
        assert_eq!(pull.supplements.len(), 2);
        let replica = conn();
        apply_log_pull(&replica, "u1", &pull).unwrap();
        assert!(
            get_day_supplements(&replica, "u1", "2026-07-19")
                .unwrap()
                .b12
        );
        assert!(
            get_day_supplements(&replica, "u1", "2026-07-20")
                .unwrap()
                .vit_d
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp, clippy::panic, missing_docs)] // test code: the asserts ARE the checks
mod targets_tests {
    use super::*;

    /// The real client schema (drift-pinned to Drizzle) now carries the `profiles` table too.
    const CLIENT_SCHEMA: &str = include_str!("../../../apps/desktop/src-tauri/schema.sql");

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        c.execute_batch(CLIENT_SCHEMA).unwrap();
        c.execute(
            "INSERT INTO users (id, name, email) VALUES ('u1', 'Ada', 'a@x')",
            [],
        )
        .unwrap();
        c
    }

    fn find<'a>(ts: &'a [NutrientTarget], name: &str) -> &'a NutrientTarget {
        ts.iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("target {name} present"))
    }
    fn amount(ts: &[NutrientTarget], name: &str) -> f64 {
        find(ts, name).amount
    }
    /// Float compare with a tiny epsilon (targets are rounded to 0.1).
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    // A profile that only sets what a test cares about; everything else is the generic default.
    fn profile(f: impl FnOnce(&mut NutritionProfile)) -> NutritionProfile {
        let mut p = NutritionProfile::default();
        f(&mut p);
        p
    }

    #[test]
    fn generic_empty_profile_uses_protective_defaults() {
        // Unknown sex ⇒ the higher of the two adult DRIs per nutrient (never under-recommend).
        let t = targets(
            &NutritionProfile::default(),
            &DaySupplements::default(),
            2026,
        );
        assert!(
            close(amount(&t, "Iron"), 32.4),
            "18 (female 19–50) × 1.8 vegan"
        );
        assert!(close(amount(&t, "Zinc"), 16.5), "11 (male) × 1.5 vegan");
        assert!(close(amount(&t, "Calcium"), 1000.0));
        assert!(close(amount(&t, "Iodine"), 150.0));
        assert!(close(amount(&t, "Selenium"), 55.0));
        assert!(close(amount(&t, "Vitamin B12"), 2.4));
        assert!(close(amount(&t, "Vitamin D"), 600.0), "IU, ages 19–70");
        assert!(
            close(amount(&t, "Omega-3 Fatty Acids"), 1.6),
            "max(1.6, 1.1) AI"
        );
        assert!(
            close(amount(&t, "Protein"), 56.0),
            "reference male gram RDA (no weight)"
        );
        assert_eq!(t.len(), 9, "the nine vegan-critical targets");
    }

    #[test]
    fn adult_male_gets_lower_iron_and_weight_based_protein() {
        let p = profile(|p| {
            p.dri_sex = Some(DriSex::Male);
            p.birth_year = Some(1990); // age 36 in 2026
            p.weight_kg = Some(80.0);
        });
        let t = targets(&p, &DaySupplements::default(), 2026);
        assert!(close(amount(&t, "Iron"), 14.4), "8 (male) × 1.8");
        assert!(close(amount(&t, "Zinc"), 16.5), "11 × 1.5");
        assert!(close(amount(&t, "Calcium"), 1000.0));
        assert!(close(amount(&t, "Vitamin D"), 600.0));
        let protein = find(&t, "Protein");
        assert!(close(protein.amount, 80.0), "1.0 g/kg × 80 kg");
        assert!(
            protein.vegan_adjusted,
            "g/kg target claims the plant adjustment"
        );
    }

    #[test]
    fn premenopausal_female_high_iron_low_zinc_reference_protein() {
        let p = profile(|p| {
            p.dri_sex = Some(DriSex::Female);
            p.birth_year = Some(1996); // age 30
        });
        let t = targets(&p, &DaySupplements::default(), 2026);
        assert!(close(amount(&t, "Iron"), 32.4), "18 × 1.8");
        assert!(close(amount(&t, "Zinc"), 12.0), "8 (female) × 1.5");
        assert!(close(amount(&t, "Omega-3 Fatty Acids"), 1.1), "female AI");
        let protein = find(&t, "Protein");
        assert!(
            close(protein.amount, 46.0),
            "reference female gram RDA (no weight)"
        );
        assert!(
            !protein.vegan_adjusted,
            "no g/kg without a weight ⇒ no adjustment claim"
        );
    }

    #[test]
    fn postmenopausal_female_iron_drops_calcium_rises() {
        let p = profile(|p| {
            p.dri_sex = Some(DriSex::Female);
            p.birth_year = Some(1966); // age 60 → 51–70 bracket
        });
        let t = targets(&p, &DaySupplements::default(), 2026);
        assert!(close(amount(&t, "Iron"), 14.4), "8 × 1.8 post-menopause");
        assert!(close(amount(&t, "Calcium"), 1200.0), "women 51+ = 1200");
    }

    #[test]
    fn elderly_male_vitamin_d_and_calcium_rise() {
        let p = profile(|p| {
            p.dri_sex = Some(DriSex::Male);
            p.birth_year = Some(1950); // age 76 → 71+
        });
        let t = targets(&p, &DaySupplements::default(), 2026);
        assert!(close(amount(&t, "Vitamin D"), 800.0), "71+ = 800 IU");
        assert!(close(amount(&t, "Calcium"), 1200.0), "men 71+ = 1200");
    }

    #[test]
    fn pregnancy_raises_iron_iodine_selenium_b12() {
        let p = profile(|p| {
            p.dri_sex = Some(DriSex::Female);
            p.pregnancy = true;
        });
        let t = targets(&p, &DaySupplements::default(), 2026);
        assert!(close(amount(&t, "Iron"), 48.6), "27 × 1.8");
        assert!(close(amount(&t, "Iodine"), 220.0));
        assert!(close(amount(&t, "Selenium"), 60.0));
        assert!(close(amount(&t, "Zinc"), 16.5), "11 × 1.5");
        assert!(close(amount(&t, "Vitamin B12"), 2.6));
    }

    #[test]
    fn lactation_values() {
        let p = profile(|p| {
            p.dri_sex = Some(DriSex::Female);
            p.lactation = true;
        });
        let t = targets(&p, &DaySupplements::default(), 2026);
        assert!(close(amount(&t, "Iron"), 16.2), "9 × 1.8");
        assert!(close(amount(&t, "Iodine"), 290.0));
        assert!(close(amount(&t, "Selenium"), 70.0));
        assert!(close(amount(&t, "Zinc"), 18.0), "12 × 1.5");
        assert!(close(amount(&t, "Vitamin B12"), 2.8));
        assert!(close(amount(&t, "Omega-3 Fatty Acids"), 1.3));
    }

    #[test]
    fn supplement_flags_mark_covered() {
        // Supplement coverage is now a per-DAY input, not a profile field.
        let taken = DaySupplements {
            b12: true,
            vit_d: true,
            algae_oil: true,
        };
        let t = targets(&NutritionProfile::default(), &taken, 2026);
        assert!(find(&t, "Vitamin B12").supplement_covered);
        assert!(find(&t, "Vitamin D").supplement_covered);
        assert!(find(&t, "Omega-3 Fatty Acids").supplement_covered);
        // A nutrient with no supplement concept is never "covered".
        assert!(!find(&t, "Iron").supplement_covered);
        // With nothing taken, the same nutrients read as a gap (not covered).
        let none = targets(
            &NutritionProfile::default(),
            &DaySupplements::default(),
            2026,
        );
        assert!(!find(&none, "Vitamin B12").supplement_covered);
        assert!(!find(&none, "Vitamin D").supplement_covered);
        assert!(!find(&none, "Omega-3 Fatty Acids").supplement_covered);
    }

    #[test]
    fn overlay_flags_and_basis_are_honest() {
        let t = targets(
            &NutritionProfile::default(),
            &DaySupplements::default(),
            2026,
        );
        assert!(find(&t, "Iron").vegan_adjusted);
        assert!(find(&t, "Zinc").vegan_adjusted);
        assert!(
            !find(&t, "Calcium").vegan_adjusted,
            "calcium DRI is unchanged for vegans"
        );
        assert!(!find(&t, "Iodine").vegan_adjusted);
        assert_eq!(
            find(&t, "Omega-3 Fatty Acids").basis,
            TargetBasis::Ai,
            "ALA is an AI"
        );
        assert_eq!(find(&t, "Iron").basis, TargetBasis::Rda);
    }

    #[test]
    fn every_target_name_matches_a_catalog_nutrient() {
        // The matching contract: a target's `name` must equal a nutrient name the catalog uses (the
        // USDA importer NUTRIENTS map), so a day total lines up with its target by name. Iodine is a
        // valid catalog name even though USDA plant data doesn't populate it yet.
        let catalog = [
            "Iron",
            "Zinc",
            "Vitamin B12",
            "Calcium",
            "Iodine",
            "Vitamin D",
            "Selenium",
            "Omega-3 Fatty Acids",
            "Protein",
        ];
        for t in targets(
            &NutritionProfile::default(),
            &DaySupplements::default(),
            2026,
        ) {
            assert!(
                catalog.contains(&t.name.as_str()),
                "unknown target name {}",
                t.name
            );
        }
    }

    #[test]
    fn save_and_get_nutrition_profile_round_trips() {
        let c = conn();
        // Absent profile ⇒ the generic default (all None/false).
        let empty = get_nutrition_profile(&c, "u1").unwrap();
        assert_eq!(empty.birth_year, None);
        assert!(!empty.pregnancy);

        let p = profile(|p| {
            p.birth_year = Some(1990);
            p.dri_sex = Some(DriSex::Male);
            p.weight_kg = Some(78.5);
        });
        save_nutrition_profile(&c, "u1", &p).unwrap();
        let got = get_nutrition_profile(&c, "u1").unwrap();
        assert_eq!(got.birth_year, Some(1990));
        assert_eq!(got.dri_sex, Some(DriSex::Male));
        assert!(close(got.weight_kg.unwrap(), 78.5));

        // Upsert replaces (single row per user).
        let p2 = profile(|p| p.dri_sex = Some(DriSex::Female));
        save_nutrition_profile(&c, "u1", &p2).unwrap();
        let got2 = get_nutrition_profile(&c, "u1").unwrap();
        assert_eq!(got2.dri_sex, Some(DriSex::Female));
        assert_eq!(
            got2.birth_year, None,
            "upsert cleared the previously-set year"
        );
        let count: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM profiles WHERE user_id = 'u1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "one row per user");
    }
}
