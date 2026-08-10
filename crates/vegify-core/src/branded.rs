//! P2.1 / **gate D1** — the SEPARABLE branded-foods store.
//!
//! Decided by John on 2026-08-07 (option 2 of the D1 brief): branded/UPC food data is fetched from
//! third parties ON DEMAND, cached here, and PROMOTED into the communal `ingredients` catalog only
//! when someone actually uses a result. No ~2M-row boot ingest — the t4g.nano's boot-ingest model
//! exists for a ~2k-row curated set, and 99%+ of the USDA Branded catalog is rows nobody here will
//! ever touch.
//!
//! # What "separable" means, precisely
//!
//! `branded_foods` must be **droppable and rebuildable without touching a single byte of user data**.
//! That is a structural property, not a promise, and it is enforced three ways:
//!
//! 1. **No foreign keys, in either direction.** `branded_foods.ingredient_id` is a plain `TEXT`
//!    column, deliberately NOT `REFERENCES ingredients(id)`. A real FK would make this table an edge
//!    in the schema graph: `DROP TABLE branded_foods` would then be a schema change other tables can
//!    observe, and (worse) a delete/restore on `ingredients` could be blocked or cascaded by a cache
//!    of somebody else's data. Nothing user-owned points at this table either, so the drop is
//!    invisible to `ingredients`, `recipes`, `log_entries`, `profiles` — everything.
//! 2. **No user data lives here.** No `user_id`, no diary reference, no ownership, no authored text.
//!    Every column is either third-party payload or cache bookkeeping (`fetched_at`, `promoted_at`).
//!    Losing the whole table costs exactly one re-fetch per food someone looks up again.
//! 3. **One table, no children.** The per-100g readings ride as a JSON blob in `nutrients_json`
//!    rather than a child table, so there is no orphan set to clean up after a drop.
//!
//! `branded_store_is_separable` (below) proves all of this against a real schema: it promotes a
//! branded food, logs a diary entry against the promoted ingredient, drops the table, and asserts the
//! diary and catalog still read.
//!
//! # Promotion
//!
//! A PROMOTED row is an ordinary unowned communal catalog ingredient — the same kind of row the USDA
//! plants boot ingest already creates (`services/api/server/src/usda.rs`): `user_id` NULL so no one
//! can edit it, `visibility = 'public'`, slugged by the DAL, and stamped with a provenance `source`.
//! That is what makes promotion cheap on the client side: promoted rows flow to every device through
//! the EXISTING `/api/content/pull`, land in the FTS5 index through the existing `ingredients`
//! trigger, and are loggable through the existing diary path. No client learns a new table.
//!
//! Promotion is also what keeps the ODbL story clean (P2.2): Open Food Facts rows carry
//! `source = OFF_SOURCE`, so a share-alike-derived row is always identifiable and never entangled
//! with user-authored content. USDA is CC0 and carries no such constraint.
//!
//! # Where the fetching lives
//!
//! Not here. This module is storage + promotion only (vegify-core has no HTTP by design); the FDC and
//! OFF clients live in the server (`services/api/server/src/fdc.rs`, `off.rs`), which keeps the API
//! key server-side and leaves the desktop a thin cache of whatever the server already promoted.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    animal_derived_flags, do_save_ingredient, DietFlag, Error, IngredientNutrientInput,
    SaveIngredientInput, Visibility,
};

/// Provenance stamp for a promoted USDA FoodData Central **Branded** row. Deliberately DIFFERENT from
/// `usda::SOURCE` ("USDA FoodData Central", the Foundation/SR plants catalog): the boot ingest gates
/// itself on that exact string, so sharing it would make a single promoted branded food convince the
/// server the whole plant catalog was already seeded. (P2.5 normalizes this vocabulary to
/// `usda_foundation` / `usda_sr` / `usda_branded` / `off` / `user`; until then the display strings are
/// the convention, matching the row already in production.)
pub const USDA_BRANDED_SOURCE: &str = "USDA FoodData Central (Branded)";

/// Provenance stamp for a promoted Open Food Facts row. ODbL share-alike: this stamp is the
/// architectural isolation the spec requires — every OFF-derived row is identifiable by it forever.
pub const OFF_SOURCE: &str = "Open Food Facts (ODbL)";

/// Which third party a cached branded row came from. Half of the store's primary key, and the thing
/// that decides both the provenance stamp and the licensing lane a promoted row lands in.
#[derive(Serialize, Deserialize, Type, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum BrandedSource {
    /// USDA FoodData Central, Branded Foods dataset. Public domain (CC0) — no downstream constraint.
    UsdaBranded,
    /// Open Food Facts. **ODbL, share-alike** — rows stay identifiable and attributed forever.
    Off,
}

impl BrandedSource {
    /// The stored discriminant (also the `source` half of the store's primary key).
    pub fn as_str(self) -> &'static str {
        match self {
            BrandedSource::UsdaBranded => "usda-branded",
            BrandedSource::Off => "off",
        }
    }

    /// Parse the stored discriminant back. Unknown values are refused rather than defaulted — a row
    /// from a source this build doesn't understand must not be silently attributed to one it does.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "usda-branded" => Some(BrandedSource::UsdaBranded),
            "off" => Some(BrandedSource::Off),
            _ => None,
        }
    }

    /// The provenance string stamped onto `ingredients.source` when a row from this source is
    /// promoted into the communal catalog.
    pub fn provenance(self) -> &'static str {
        match self {
            BrandedSource::UsdaBranded => USDA_BRANDED_SOURCE,
            BrandedSource::Off => OFF_SOURCE,
        }
    }

    /// The attribution line a client MUST render beside a result from this source. For OFF this is a
    /// licensing obligation, not a courtesy.
    pub fn attribution(self) -> &'static str {
        match self {
            BrandedSource::UsdaBranded => {
                "Data from USDA FoodData Central (Branded Foods), public domain (CC0)."
            }
            BrandedSource::Off => {
                "Data from Open Food Facts, licensed under the Open Database License (ODbL)."
            }
        }
    }
}

/// One branded food: the third-party record as fetched, plus the cache/promotion bookkeeping. This is
/// BOTH the row shape of the separable store and the wire shape the lookup endpoints return, on
/// purpose — there is no server-side transformation worth a second type, and one type means the
/// promotion contract (`source` + `externalId`) is the same identifier the client just received.
#[derive(Serialize, Deserialize, Type, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrandedFood {
    /// Which third party this came from.
    pub source: BrandedSource,
    /// That source's own id for the food — an FDC `fdcId`, or the OFF barcode. With `source`, the
    /// store's primary key and the promotion request's whole payload.
    pub external_id: String,
    /// Barcode (GTIN/UPC/EAN) when the source carries one — the key P2.2's scan path looks up by.
    pub gtin: Option<String>,
    /// Product name as the source states it.
    pub name: String,
    /// Brand owner / brand name, when stated.
    pub brand: Option<String>,
    /// Declared serving size in grams, when stated.
    pub serving_grams: Option<f64>,
    /// The serving's unit name as stated (e.g. "cup", "bar"); None ⇒ grams.
    pub serving_unit: Option<String>,
    /// The label's ingredient statement, verbatim. The input to [`animal_derived_flags`] — and the
    /// reason word-aware matching is not optional (see `crate::diet`).
    pub ingredients_text: Option<String>,
    /// Calories per 100 g, when stated.
    pub calories_per_100g: Option<f64>,
    /// Per-100g nutrient readings, already mapped into the catalog's nutrient vocabulary.
    pub nutrients: Vec<IngredientNutrientInput>,
    /// The promoted catalog ingredient's id, once someone has used this food. None ⇒ cached only.
    pub ingredient_id: Option<String>,
    /// Advisory animal-derived terms found in the product NAME and `ingredients_text`, matched as
    /// WORDS with plant qualifiers suppressed (see [`branded_diet_flags`]). **Never a vegan
    /// certification** — an empty list means "nothing matched", not "this is vegan"; see
    /// `crate::diet`.
    pub diet_flags: Vec<DietFlag>,
    /// The attribution line the client must render (see [`BrandedSource::attribution`]).
    pub attribution: String,
}

/// THE text a branded food's advisory flags are read from: the product NAME plus the label's
/// ingredient statement, scanned as one word-aware pass (see [`crate::diet`] — tokens, never
/// substrings, with plant qualifiers and vegan collocations suppressed).
///
/// One function because it has to be one answer. The fetch clients (`services/api/server/src/fdc.rs`,
/// `off.rs`) stamp flags onto a freshly fetched food, and [`BrandedFood::with_derived`] recomputes
/// them on every cache read; if those two scanned different text the SAME food would warn on the cold
/// lookup and go quiet on the warm one — the worst possible behaviour for a trust signal, and
/// precisely the kind of inconsistency that makes people stop believing the flag panel.
///
/// The product name is in scope deliberately: a bar whose name is "MILK CHOCOLATE" and which
/// publishes no ingredient statement still deserves the dairy flag. Word-awareness is what makes
/// including the name safe — "OAT MILK", "COCONUT MILK" and "MILK THISTLE POWDER" all suppress, while
/// "MILK CHOCOLATE" does not.
pub fn branded_diet_flags(name: &str, ingredients_text: Option<&str>) -> Vec<DietFlag> {
    match ingredients_text {
        Some(text) => animal_derived_flags(&format!("{name} {text}")),
        None => animal_derived_flags(name),
    }
}

impl BrandedFood {
    /// Recompute the derived fields (diet flags + attribution) from the stored payload. Called on
    /// every read so a change to the term tables or the attribution text takes effect for already
    /// cached rows without a store migration — another reason the cache is safe to keep thin.
    fn with_derived(mut self) -> Self {
        self.diet_flags = branded_diet_flags(&self.name, self.ingredients_text.as_deref());
        self.attribution = self.source.attribution().to_string();
        self
    }

    /// The catalog name a promoted row gets: brand-prefixed unless the product name already carries
    /// the brand ("Oatly Oat Drink" stays as-is; "Oat Drink" + brand "Oatly" becomes "Oatly Oat
    /// Drink"). Branded search is brand-led, so the prefix is what makes the promoted row findable.
    fn catalog_name(&self) -> String {
        match self
            .brand
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
        {
            Some(brand) if !self.name.to_lowercase().contains(&brand.to_lowercase()) => {
                format!("{brand} {}", self.name)
            }
            _ => self.name.clone(),
        }
    }
}

/// Idempotent creation of the separable branded store. Called from the server's `ensure_schema`.
///
/// Deliberately **server-only** and deliberately **not** a Drizzle-tracked table (the same call the
/// FTS5 index makes, for the same reason): this is a cache of third-party data, not user data or app
/// content. The desktop never needs it — promoted rows reach every client as ordinary ingredients on
/// the existing content pull — so there is exactly one home for it, and dropping that one table is the
/// entire teardown.
///
/// See the module docs for why `ingredient_id` carries no foreign key.
pub fn ensure_branded_store(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS branded_foods (
            source TEXT NOT NULL,
            external_id TEXT NOT NULL,
            gtin TEXT,
            name TEXT NOT NULL,
            brand TEXT,
            serving_grams REAL,
            serving_unit TEXT,
            ingredients_text TEXT,
            calories_per_100g REAL,
            nutrients_json TEXT NOT NULL,
            ingredient_id TEXT,
            fetched_at INTEGER NOT NULL,
            promoted_at INTEGER,
            PRIMARY KEY (source, external_id)
         );
         CREATE INDEX IF NOT EXISTS branded_foods_gtin_idx ON branded_foods(gtin);
         CREATE INDEX IF NOT EXISTS branded_foods_name_idx ON branded_foods(name);
         CREATE INDEX IF NOT EXISTS branded_foods_promoted_idx ON branded_foods(ingredient_id);",
    )
}

/// Current time in Unix milliseconds (cache bookkeeping).
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Read one cached row out of a query result set.
fn row_to_food(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<BrandedFood>> {
    let source: String = row.get("source")?;
    let Some(source) = BrandedSource::parse(&source) else {
        return Ok(None); // a source this build doesn't know — skip, never guess
    };
    let nutrients_json: String = row.get("nutrients_json")?;
    let nutrients: Vec<IngredientNutrientInput> =
        serde_json::from_str(&nutrients_json).unwrap_or_default();
    Ok(Some(
        BrandedFood {
            source,
            external_id: row.get("external_id")?,
            gtin: row.get("gtin")?,
            name: row.get("name")?,
            brand: row.get("brand")?,
            serving_grams: row.get("serving_grams")?,
            serving_unit: row.get("serving_unit")?,
            ingredients_text: row.get("ingredients_text")?,
            calories_per_100g: row.get("calories_per_100g")?,
            nutrients,
            ingredient_id: row.get("ingredient_id")?,
            diet_flags: Vec::new(),
            attribution: String::new(),
        }
        .with_derived(),
    ))
}

/// Upsert a freshly fetched food into the cache. `ingredient_id`/`promoted_at` are PRESERVED across a
/// re-fetch (a refreshed payload must never un-promote a row the catalog already links to); every
/// other column is replaced with the newer data, which is what makes "always current" true — each
/// miss is a live fetch and each hit can be refreshed in place.
pub fn cache_branded_food(conn: &Connection, food: &BrandedFood) -> Result<(), Error> {
    let nutrients_json =
        serde_json::to_string(&food.nutrients).map_err(|e| Error::Db(e.to_string()))?;
    conn.execute(
        "INSERT INTO branded_foods
           (source, external_id, gtin, name, brand, serving_grams, serving_unit,
            ingredients_text, calories_per_100g, nutrients_json, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(source, external_id) DO UPDATE SET
           gtin = excluded.gtin,
           name = excluded.name,
           brand = excluded.brand,
           serving_grams = excluded.serving_grams,
           serving_unit = excluded.serving_unit,
           ingredients_text = excluded.ingredients_text,
           calories_per_100g = excluded.calories_per_100g,
           nutrients_json = excluded.nutrients_json,
           fetched_at = excluded.fetched_at",
        params![
            food.source.as_str(),
            food.external_id,
            food.gtin,
            food.name,
            food.brand,
            food.serving_grams,
            food.serving_unit,
            food.ingredients_text,
            food.calories_per_100g,
            nutrients_json,
            now_ms(),
        ],
    )?;
    Ok(())
}

/// One cached food by its source-scoped id (the store's primary key).
pub fn cached_branded_food(
    conn: &Connection,
    source: BrandedSource,
    external_id: &str,
) -> Result<Option<BrandedFood>, Error> {
    let found = conn
        .query_row(
            "SELECT * FROM branded_foods WHERE source = ?1 AND external_id = ?2",
            params![source.as_str(), external_id],
            row_to_food,
        )
        .optional()?;
    Ok(found.flatten())
}

/// One cached food by barcode, newest fetch first. USDA Branded is preferred over OFF when both have
/// the GTIN: USDA is CC0 and analytically curated, and preferring it keeps the ODbL lane as small as
/// it can be (the same order the live lookup tries the two APIs in).
pub fn cached_branded_food_by_gtin(
    conn: &Connection,
    gtin: &str,
) -> Result<Option<BrandedFood>, Error> {
    let found = conn
        .query_row(
            "SELECT * FROM branded_foods WHERE gtin = ?1
             ORDER BY CASE source WHEN 'usda-branded' THEN 0 ELSE 1 END, fetched_at DESC
             LIMIT 1",
            [gtin],
            row_to_food,
        )
        .optional()?;
    Ok(found.flatten())
}

/// Cached branded foods matching `query`, ranked the same way the catalog search ranks
/// (`crate::MatchTier`: exact prefix, then word prefix, then substring) so a branded hit list and a
/// catalog hit list are ordered by the same rules — including the word-aware discipline, which is the
/// whole reason P2.4's ranking was worth building before this landed.
///
/// This is a CACHE read: it answers instantly and offline, and the caller tops it up with a live
/// third-party fetch when it under-fills. Brand and product name are both searched, since branded
/// search is brand-led ("oatly" must find "Oat Drink").
pub fn search_cached_branded(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<BrandedFood>, Error> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let like = format!("%{}%", q.replace(['%', '_'], ""));
    let mut stmt = conn.prepare(
        "SELECT * FROM branded_foods
         WHERE lower(name) LIKE ?1 OR lower(COALESCE(brand, '')) LIKE ?1",
    )?;
    let rows = stmt
        .query_map([&like], row_to_food)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut hits: Vec<(crate::MatchTier, String, BrandedFood)> = rows
        .into_iter()
        .flatten()
        .map(|f| {
            let haystack = match f.brand.as_deref() {
                Some(b) => format!("{b} {}", f.name),
                None => f.name.clone(),
            };
            (crate::match_tier(&haystack, &q), haystack, f)
        })
        .collect();
    hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    hits.truncate(limit);
    Ok(hits.into_iter().map(|(_, _, f)| f).collect())
}

/// **Promote-on-first-use**: join a cached branded food into the communal `ingredients` catalog and
/// return the catalog id. Idempotent — a food already promoted (and whose ingredient still exists)
/// returns the existing id without creating a second row, so "first use" really is once, no matter how
/// many users select the same food or how often a client retries.
///
/// The created row is unowned (`user_id` NULL ⇒ the DAL's owner gate makes it uneditable), public,
/// DAL-slugged, and stamped with the source's provenance — identical in kind to the USDA plants the
/// boot ingest creates. From here on it is an ordinary catalog ingredient: it syncs on the content
/// pull, indexes in FTS5 via the existing trigger, and logs through the existing diary path.
pub fn promote_branded_food(
    conn: &Connection,
    source: BrandedSource,
    external_id: &str,
) -> Result<String, Error> {
    let food = cached_branded_food(conn, source, external_id)?.ok_or_else(|| {
        Error::Db("That branded food isn't cached — look it up again before adding it.".into())
    })?;

    // Already promoted AND the catalog row still exists ⇒ done. (The existence check matters: the
    // store carries no FK, by design, so a removed ingredient leaves a stale link here rather than
    // cascading — re-promoting is the correct repair.)
    if let Some(existing) = food.ingredient_id.as_deref() {
        let live: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ingredients WHERE id = ?1",
            [existing],
            |r| r.get(0),
        )?;
        if live > 0 {
            return Ok(existing.to_string());
        }
    }

    let input = SaveIngredientInput {
        id: None,
        visibility: Some(Visibility::Public),
        name: food.catalog_name(),
        // The label's own ingredient statement, so the catalog page shows what the flags were read
        // from. Nothing is paraphrased into it.
        description: food.ingredients_text.clone(),
        price: None,
        calories_per_100g: food.calories_per_100g,
        serving_grams: food.serving_grams,
        serving_unit: food.serving_unit.clone(),
        package_grams: None,
        nutrients: food.nutrients.clone(),
        slug: None, // the DAL generates a unique catalog slug, like any create
    };
    let ingredient_id = do_save_ingredient(conn, &input, None)?;
    conn.execute(
        "UPDATE ingredients SET source = ?1 WHERE id = ?2",
        params![source.provenance(), ingredient_id],
    )?;
    conn.execute(
        "UPDATE branded_foods SET ingredient_id = ?1, promoted_at = ?2
         WHERE source = ?3 AND external_id = ?4",
        params![ingredient_id, now_ms(), source.as_str(), external_id],
    )?;
    Ok(ingredient_id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod tests {
    use super::*;

    /// The REAL client schema (drift-test-pinned to the drizzle source in the desktop crate), so these
    /// fixtures can't silently diverge from production tables.
    const CLIENT_SCHEMA: &str = include_str!("../../../apps/desktop/src-tauri/schema.sql");

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        c.execute_batch(CLIENT_SCHEMA).unwrap();
        crate::ensure_search_index(&c).unwrap();
        ensure_branded_store(&c).unwrap();
        c
    }

    fn food(external_id: &str, name: &str, brand: Option<&str>) -> BrandedFood {
        BrandedFood {
            source: BrandedSource::UsdaBranded,
            external_id: external_id.into(),
            gtin: Some("0123456789012".into()),
            name: name.into(),
            brand: brand.map(Into::into),
            serving_grams: Some(240.0),
            serving_unit: Some("cup".into()),
            ingredients_text: Some("Water, oats, sea salt".into()),
            calories_per_100g: Some(50.0),
            nutrients: vec![IngredientNutrientInput {
                name: "Calcium".into(),
                amount_per_100g: 120.0,
                unit: "mg".into(),
            }],
            ingredient_id: None,
            diet_flags: Vec::new(),
            attribution: String::new(),
        }
    }

    #[test]
    fn ensure_branded_store_is_idempotent() {
        let c = conn();
        ensure_branded_store(&c).unwrap();
        ensure_branded_store(&c).unwrap();
    }

    #[test]
    fn cache_roundtrips_and_derives_flags_and_attribution() {
        let c = conn();
        let mut f = food("1", "Oat Drink", Some("Oatly"));
        f.ingredients_text = Some("Oat base, rapeseed oil, calcium carbonate".into());
        cache_branded_food(&c, &f).unwrap();
        let got = cached_branded_food(&c, BrandedSource::UsdaBranded, "1")
            .unwrap()
            .unwrap();
        assert_eq!(got.name, "Oat Drink");
        assert_eq!(got.brand.as_deref(), Some("Oatly"));
        assert_eq!(got.nutrients.len(), 1);
        assert!(got.diet_flags.is_empty());
        assert!(got.attribution.contains("USDA"));
        assert!(got.ingredient_id.is_none(), "cached ≠ promoted");
    }

    /// Derived fields are recomputed on READ, never stored — so a dairy statement flags even though
    /// the cache row holds no flag column.
    #[test]
    fn diet_flags_are_derived_on_read() {
        let c = conn();
        let mut f = food("2", "Milk Chocolate Bar", Some("Acme"));
        f.ingredients_text = Some("Sugar, cocoa butter, whole milk powder".into());
        cache_branded_food(&c, &f).unwrap();
        let got = cached_branded_food(&c, BrandedSource::UsdaBranded, "2")
            .unwrap()
            .unwrap();
        let terms: Vec<_> = got.diet_flags.iter().map(|d| d.term.as_str()).collect();
        assert_eq!(terms, ["milk"], "cocoa butter must not flag; milk must");
    }

    /// **The product NAME is scanned too.** FDC/OFF rows routinely publish a name and no ingredient
    /// statement; a bar called "MILK CHOCOLATE" with no statement must still warn. Regression guard:
    /// the read path once scanned `ingredients_text` alone while the fetch clients scanned name +
    /// statement, so this exact food warned on the cold lookup and went silent on every warm one.
    #[test]
    fn the_product_name_is_scanned_when_the_label_publishes_no_statement() {
        let c = conn();
        let mut f = food("name-only", "MILK CHOCOLATE", Some("Acme"));
        f.ingredients_text = None;
        cache_branded_food(&c, &f).unwrap();
        let got = cached_branded_food(&c, BrandedSource::UsdaBranded, "name-only")
            .unwrap()
            .unwrap();
        let terms: Vec<_> = got.diet_flags.iter().map(|d| d.term.as_str()).collect();
        assert_eq!(terms, ["milk"], "a name-only dairy label must still warn");
        assert_eq!(
            got.diet_flags[0].matched, "MILK",
            "quotes the label's casing"
        );
    }

    /// The cold-fetch stamp and the warm cache read are the SAME scan, by construction — both call
    /// `branded_diet_flags`. Pinned as a test because the two live in different crates (the fetch
    /// clients are in the server), which is exactly how they drifted apart the first time.
    #[test]
    fn cold_fetch_flags_and_warm_cache_flags_agree() {
        let c = conn();
        for (name, statement) in [
            ("MILK CHOCOLATE", None),
            (
                "OAT MILK CHAI",
                Some("OAT MILK POWDER, MILK THISTLE POWDER"),
            ),
            ("Cheddar Style Shreds", Some("Coconut oil, potato starch")),
            ("Greek Yogurt", Some("Cultured pasteurized milk, honey")),
        ] {
            let statement: Option<String> = statement.map(Into::into);
            // What a fetch client stamps onto the freshly fetched food…
            let cold = branded_diet_flags(name, statement.as_deref());
            // …and what the store derives when the same row comes back out of the cache.
            let mut f = food("agree", name, None);
            f.name = name.into();
            f.ingredients_text = statement.clone();
            cache_branded_food(&c, &f).unwrap();
            let warm = cached_branded_food(&c, BrandedSource::UsdaBranded, "agree")
                .unwrap()
                .unwrap()
                .diet_flags;
            let terms =
                |v: &[DietFlag]| -> Vec<String> { v.iter().map(|d| d.term.clone()).collect() };
            assert_eq!(
                terms(&cold),
                terms(&warm),
                "cold vs warm disagreed on {name}"
            );
        }
    }

    /// **Word-aware, not substring — proven through the whole branded path** (cache write → ranked
    /// search read → derived flags), which is where it actually has to hold. The plant-milk rows are
    /// the competitor bug this layer exists to not repeat; the dairy rows are the other half of the
    /// contract, because a matcher that never flags is just as useless as one that always does.
    #[test]
    fn branded_search_results_carry_word_aware_flags() {
        let c = conn();
        let rows: &[(&str, &str, Option<&str>, &[&str])] = &[
            // (external_id, product name, ingredient statement, expected flag terms)
            (
                "w1",
                "CHOCOLATE OAT MILK",
                Some("OAT BASE (WATER, OATS), COCOA BUTTER, SEA SALT"),
                &[],
            ),
            (
                "w2",
                "OAT MILK CHAI SUPERFOOD LATTE BLEND",
                Some("OAT MILK POWDER, MILK THISTLE POWDER, GINGER ROOT"),
                &[],
            ),
            (
                "w3",
                "BUTTERNUT SQUASH SOUP",
                Some("Butternut squash, water, sea salt"),
                &[],
            ),
            (
                "w4",
                "PLANT BASED CHEESE SHREDS",
                Some("Coconut oil, potato starch, plant based cheese cultures"),
                &[],
            ),
            (
                "w5",
                "MILK CHOCOLATE ALMONDS",
                Some("SUGAR, WHOLE MILK POWDER, ALMONDS, COCOA BUTTER"),
                &["milk"],
            ),
            (
                "w6",
                "SHARP CHEDDAR CHEESE",
                Some("PASTEURIZED MILK, SALT, CHEESE CULTURES, RENNET"),
                &["cheese", "milk", "rennet"],
            ),
        ];
        for (id, name, statement, _) in rows {
            let mut f = food(id, name, Some("Acme"));
            f.ingredients_text = statement.map(Into::into);
            cache_branded_food(&c, &f).unwrap();
        }
        for (id, name, _, expected) in rows {
            // Read it back the way a client actually does: through the ranked search the endpoint uses.
            let hit = search_cached_branded(&c, name, 10)
                .unwrap()
                .into_iter()
                .find(|f| f.external_id == *id)
                .expect("the row must come back from a search for its own name");
            let mut terms: Vec<&str> = hit.diet_flags.iter().map(|d| d.term.as_str()).collect();
            terms.sort_unstable();
            let mut want: Vec<&str> = expected.to_vec();
            want.sort_unstable();
            assert_eq!(terms, want, "wrong flags for {name}");
        }
    }

    /// An empty flag list is NOT a vegan claim, and the type makes that unfakeable: there is no
    /// "vegan" variant, no boolean, nothing to invert. The only thing a caller can learn is which
    /// terms were FOUND. Pinned so a future convenience field can't quietly turn the panel into a
    /// certification.
    #[test]
    fn flags_are_one_directional_there_is_no_vegan_verdict() {
        let c = conn();
        let mut f = food("one-way", "Coconut Milk", Some("Acme"));
        f.ingredients_text = Some("Coconut extract, water, guar gum".into());
        cache_branded_food(&c, &f).unwrap();
        let got = cached_branded_food(&c, BrandedSource::UsdaBranded, "one-way")
            .unwrap()
            .unwrap();
        assert!(got.diet_flags.is_empty(), "coconut milk is not dairy");
        // The advisory carries no verdict of its own — the caller only ever sees found terms, and the
        // UI is required to say "nothing matched", never "vegan" (packages/ui/src/branded.tsx).
        let json = serde_json::to_string(&got).unwrap();
        assert!(!json.contains("\"vegan\""), "no vegan verdict on the wire");
    }

    #[test]
    fn refetch_updates_payload_but_never_un_promotes() {
        let c = conn();
        cache_branded_food(&c, &food("3", "Old Name", Some("Acme"))).unwrap();
        let id = promote_branded_food(&c, BrandedSource::UsdaBranded, "3").unwrap();
        let mut fresher = food("3", "New Name", Some("Acme"));
        fresher.calories_per_100g = Some(77.0);
        cache_branded_food(&c, &fresher).unwrap();
        let got = cached_branded_food(&c, BrandedSource::UsdaBranded, "3")
            .unwrap()
            .unwrap();
        assert_eq!(got.name, "New Name");
        assert_eq!(got.calories_per_100g, Some(77.0));
        assert_eq!(got.ingredient_id.as_deref(), Some(id.as_str()));
    }

    #[test]
    fn promote_creates_an_unowned_public_sourced_slugged_row_and_is_idempotent() {
        let c = conn();
        cache_branded_food(&c, &food("4", "Oat Drink", Some("Oatly"))).unwrap();
        let id = promote_branded_food(&c, BrandedSource::UsdaBranded, "4").unwrap();
        let again = promote_branded_food(&c, BrandedSource::UsdaBranded, "4").unwrap();
        assert_eq!(id, again, "promote-on-FIRST-use: never a second row");

        let (name, user_id, visibility, source, slug, calories): (
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<f64>,
        ) = c
            .query_row(
                "SELECT name, user_id, visibility, source, slug, calories_per_100g
                 FROM ingredients WHERE id = ?1",
                [&id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(name, "Oatly Oat Drink", "brand-prefixed catalog name");
        assert_eq!(
            user_id, None,
            "unowned ⇒ uneditable by the DAL's owner gate"
        );
        assert_eq!(visibility, "public");
        assert_eq!(source.as_deref(), Some(USDA_BRANDED_SOURCE));
        assert!(slug.is_some_and(|s| !s.is_empty()));
        assert_eq!(calories, Some(50.0));

        // The nutrient readings came across, and the promoted row is in the catalog's FTS5 index
        // (via the existing ingredients trigger — the client never learns a new table).
        let readings: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM ingredient_nutrient WHERE ingredient_id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(readings, 1);
        let indexed: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM content_search WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(indexed, 1, "promoted rows ride the existing search index");
    }

    /// The branded provenance stamp must NOT collide with the plants catalog's marker
    /// (`usda::SOURCE`), or one promoted branded food would convince the server's boot ingest that
    /// the whole ~2k-row plant catalog was already seeded.
    #[test]
    fn branded_provenance_differs_from_the_plants_catalog_marker() {
        assert_ne!(USDA_BRANDED_SOURCE, "USDA FoodData Central");
        assert_ne!(OFF_SOURCE, USDA_BRANDED_SOURCE);
    }

    #[test]
    fn barcode_lookup_prefers_usda_over_off() {
        let c = conn();
        let mut off = food("off-1", "Store Brand Oat Drink", Some("Store"));
        off.source = BrandedSource::Off;
        cache_branded_food(&c, &off).unwrap();
        cache_branded_food(&c, &food("usda-1", "Oat Drink", Some("Oatly"))).unwrap();
        let got = cached_branded_food_by_gtin(&c, "0123456789012")
            .unwrap()
            .unwrap();
        assert_eq!(got.source, BrandedSource::UsdaBranded, "CC0 lane wins");
        assert!(got.attribution.contains("public domain"));
    }

    #[test]
    fn off_rows_carry_the_odbl_stamp_when_promoted() {
        let c = conn();
        let mut off = food("off-2", "Hummus", Some("Store"));
        off.source = BrandedSource::Off;
        cache_branded_food(&c, &off).unwrap();
        let id = promote_branded_food(&c, BrandedSource::Off, "off-2").unwrap();
        let source: Option<String> = c
            .query_row("SELECT source FROM ingredients WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            source.as_deref(),
            Some(OFF_SOURCE),
            "ODbL isolation is a stamp that survives on the row forever"
        );
    }

    #[test]
    fn cached_search_ranks_exact_prefix_first_and_searches_brand() {
        let c = conn();
        cache_branded_food(&c, &food("a", "Chocolate Oat Drink", Some("Acme"))).unwrap();
        cache_branded_food(&c, &food("b", "Oat Drink", Some("Oatly"))).unwrap();
        let hits = search_cached_branded(&c, "oatly", 10).unwrap();
        assert_eq!(hits.len(), 1, "brand is searchable");
        assert_eq!(hits[0].external_id, "b");

        let hits = search_cached_branded(&c, "acme", 10).unwrap();
        assert_eq!(hits[0].external_id, "a");
        assert!(search_cached_branded(&c, "", 10).unwrap().is_empty());
    }

    #[test]
    fn promoting_an_uncached_food_is_refused() {
        let c = conn();
        assert!(promote_branded_food(&c, BrandedSource::UsdaBranded, "nope").is_err());
    }

    /// **The D1 separability guarantee, proven rather than asserted.** Promote a branded food, log a
    /// diary entry against the promoted ingredient, then DROP the entire branded store — the catalog
    /// row, the diary entry, and the day's totals must all survive and still read. This is what makes
    /// the branded layer droppable/rebuildable without touching user data: no FK edge in either
    /// direction, no user data inside it, and no child tables to orphan.
    #[test]
    fn branded_store_is_separable() {
        let c = conn();
        c.execute(
            "INSERT INTO users (id, name, email) VALUES ('u1', 'Ada', 'a@x')",
            [],
        )
        .unwrap();
        cache_branded_food(&c, &food("sep", "Oat Drink", Some("Oatly"))).unwrap();
        let ingredient_id = promote_branded_food(&c, BrandedSource::UsdaBranded, "sep").unwrap();

        let entry_id = crate::do_save_log_entry(
            &c,
            &crate::SaveLogEntryInput {
                id: None,
                ingredient_id: ingredient_id.clone(),
                date: "2026-08-07".into(),
                slot: None,
                grams: 240.0,
                unit: None,
                logged_at: None,
            },
            "u1",
        )
        .unwrap();

        // The drop must not be blocked by a foreign key and must not cascade into anything.
        c.execute_batch("DROP TABLE branded_foods;").unwrap();

        let day = crate::log_day(&c, "u1", "2026-08-07").unwrap();
        assert_eq!(day.entries.len(), 1, "the diary entry survives the drop");
        assert_eq!(day.entries[0].id, entry_id);
        let still_there: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM ingredients WHERE id = ?1",
                [&ingredient_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(still_there, 1, "the promoted catalog row survives the drop");

        // …and the store rebuilds from nothing but a re-fetch.
        ensure_branded_store(&c).unwrap();
        assert!(cached_branded_food(&c, BrandedSource::UsdaBranded, "sep")
            .unwrap()
            .is_none());
        cache_branded_food(&c, &food("sep", "Oat Drink", Some("Oatly"))).unwrap();
        assert!(cached_branded_food(&c, BrandedSource::UsdaBranded, "sep")
            .unwrap()
            .is_some());
    }
}
