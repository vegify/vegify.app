//! P2.2 — the **Open Food Facts** barcode client: the fallback lane when a scanned GTIN isn't in USDA
//! Branded. OFF carries ~900k US products (and far better international coverage), which is exactly
//! the gap USDA leaves on a store shelf.
//!
//! # ODbL isolation is architectural, not a footnote
//!
//! OFF is licensed under the **Open Database License — share-alike**. USDA FDC is CC0 and carries no
//! downstream constraint; conflating the two would quietly put a share-alike obligation on this
//! repo's open-source licensing story and on user-authored content. So the isolation is built into the
//! data model rather than documented next to it:
//!
//! - every cached row is keyed `source = 'off'` (`vegify_core::BrandedSource::Off`), permanently
//!   distinguishable from the CC0 lane;
//! - every promoted row is stamped `ingredients.source = OFF_SOURCE` ("Open Food Facts (ODbL)"), so a
//!   share-alike-derived catalog row can always be found — and, if it ever had to be, removed;
//! - every response carries the attribution line the client must render;
//! - USDA is tried FIRST on every barcode, both in the live lookup and in the cache read, so the ODbL
//!   lane stays as small as the data allows.
//!
//! OFF-derived rows are never merged into user-authored recipes or ingredients: promotion creates a
//! separate unowned catalog row, and a user who edits it gets the DAL's owner gate (it is unowned, so
//! nobody can edit it in place at all).
//!
//! # Etiquette
//!
//! OFF asks API consumers for a descriptive, contactable User-Agent and reasonable request volume. We
//! send one, and the endpoint that calls this is rate-limited per IP on top of the general middleware
//! budget. We request only the fields we use, which is both faster and politer than pulling the full
//! product document.

use std::time::Duration;

use serde::Deserialize;
use vegify_core::{BrandedFood, BrandedSource, IngredientNutrientInput};

/// The OFF read API (v2). Not configurable, for the same reason FDC's base isn't.
const BASE: &str = "https://world.openfoodfacts.org/api/v2";

/// Descriptive User-Agent, as OFF's API guidance asks for. Version is deliberately absent (every
/// committed version field in this repo is a vestigial `0.0.0` — the real version is git-tag-derived
/// at build time, so hardcoding one here would just be a lie that ages).
const USER_AGENT: &str =
    "Vegify/1.0 (open-source plant-based nutrition tracker; https://vegify.app)";

/// Same hard ceiling as the FDC client: a hung upstream degrades one lookup, never the site.
const TIMEOUT: Duration = Duration::from_secs(8);

/// The only fields we ask OFF for.
const FIELDS: &str =
    "code,product_name,brands,ingredients_text,serving_quantity,serving_size,nutriments";

/// OFF nutriment key → the catalog's (name, unit) and the factor converting OFF's stored value.
///
/// **OFF normalizes almost everything to GRAMS per 100 g** — including minerals and vitamins, so iron
/// arrives as `0.0015` where the catalog wants `1.5 mg`. Energy is the exception (`energy-kcal_100g`
/// really is kcal). Getting this table wrong is a 1000× error in someone's iron intake, so the factors
/// are stated explicitly per row rather than inferred from the unit string.
const NUTRIMENTS: &[(&str, &str, &str, f64)] = &[
    ("proteins_100g", "Protein", "g", 1.0),
    ("carbohydrates_100g", "Carbohydrates", "g", 1.0),
    ("fat_100g", "Total Fat", "g", 1.0),
    ("saturated-fat_100g", "Saturated Fat", "g", 1.0),
    ("fiber_100g", "Total Fiber", "g", 1.0),
    ("cholesterol_100g", "Cholesterol", "mg", 1000.0),
    ("sodium_100g", "Sodium", "mg", 1000.0),
    ("calcium_100g", "Calcium", "mg", 1000.0),
    ("iron_100g", "Iron", "mg", 1000.0),
    ("magnesium_100g", "Magnesium", "mg", 1000.0),
    ("potassium_100g", "Potassium", "mg", 1000.0),
    ("zinc_100g", "Zinc", "mg", 1000.0),
    ("copper_100g", "Copper", "mg", 1000.0),
    ("phosphorus_100g", "Phosphorus", "mg", 1000.0),
    ("manganese_100g", "Manganese", "mg", 1000.0),
    ("selenium_100g", "Selenium", "ug", 1_000_000.0),
    ("vitamin-c_100g", "Vitamin C", "mg", 1000.0),
    ("vitamin-b6_100g", "Vitamin B6", "mg", 1000.0),
    ("vitamin-b12_100g", "Vitamin B12", "ug", 1_000_000.0),
    ("vitamin-e_100g", "Vitamin E", "mg", 1000.0),
    ("vitamin-k_100g", "Vitamin K", "ug", 1_000_000.0),
    ("vitamin-b1_100g", "Thiamin", "mg", 1000.0),
    ("vitamin-b2_100g", "Riboflavin", "mg", 1000.0),
    ("vitamin-pp_100g", "Niacin", "mg", 1000.0),
    ("vitamin-b9_100g", "Folate", "ug", 1_000_000.0),
    ("pantothenic-acid_100g", "Pantothenic Acid", "mg", 1000.0),
];

#[derive(Deserialize)]
struct ProductResponse {
    #[serde(default)]
    status: i64,
    #[serde(default)]
    product: Option<Product>,
}

#[derive(Deserialize)]
struct Product {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    product_name: Option<String>,
    #[serde(default)]
    brands: Option<String>,
    #[serde(default)]
    ingredients_text: Option<String>,
    /// OFF states the serving as a number of grams (`serving_quantity`) plus a free-text
    /// `serving_size` ("240 ml (1 cup)"). Only the numeric grams are canonical for us.
    #[serde(default)]
    serving_quantity: Option<serde_json::Value>,
    #[serde(default)]
    nutriments: std::collections::HashMap<String, serde_json::Value>,
}

/// Non-empty, trimmed.
fn clean(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// OFF is community-entered and loosely typed: the same field arrives as `240`, `240.0`, or `"240"`
/// depending on who typed it. Accept all three, reject anything else.
fn as_f64(v: Option<&serde_json::Value>) -> Option<f64> {
    match v? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Map an OFF product document into the branded store's shape.
fn to_branded(barcode: &str, product: Product) -> Option<BrandedFood> {
    let name = clean(product.product_name)?; // an unnamed product is not a usable result
    let mut nutrients = Vec::new();
    for (key, catalog_name, unit, factor) in NUTRIMENTS {
        let Some(value) = as_f64(product.nutriments.get(*key)) else {
            continue;
        };
        if !value.is_finite() || value < 0.0 {
            continue; // community data: refuse nonsense rather than storing it
        }
        nutrients.push(IngredientNutrientInput {
            name: (*catalog_name).to_string(),
            amount_per_100g: value * factor,
            unit: (*unit).to_string(),
        });
    }
    let calories_per_100g = as_f64(product.nutriments.get("energy-kcal_100g"))
        .or_else(|| {
            // Some rows carry only kilojoules — convert rather than drop the calorie figure.
            as_f64(product.nutriments.get("energy-kj_100g")).map(|kj| kj / 4.184)
        })
        .filter(|v| v.is_finite() && *v >= 0.0);

    let ingredients_text = clean(product.ingredients_text);
    let gtin = clean(product.code).unwrap_or_else(|| barcode.to_string());
    Some(BrandedFood {
        source: BrandedSource::Off,
        // The barcode IS OFF's product id, so it doubles as the store's external id.
        external_id: gtin.clone(),
        gtin: Some(gtin),
        brand: clean(product.brands).and_then(|b| {
            // OFF's `brands` is a comma-separated list; the first entry is the primary brand.
            clean(b.split(',').next().map(str::to_string))
        }),
        serving_grams: as_f64(product.serving_quantity.as_ref()).filter(|v| *v > 0.0),
        serving_unit: None,
        // The ONE scan definition, shared with the store's read path (see fdc.rs's note): product name
        // AND statement, word-aware, so cold and warm reads of the same product always agree.
        diet_flags: vegify_core::branded_diet_flags(&name, ingredients_text.as_deref()),
        name,
        ingredients_text,
        calories_per_100g,
        nutrients,
        ingredient_id: None,
        attribution: BrandedSource::Off.attribution().to_string(),
    })
}

/// The product carrying `barcode`, per Open Food Facts. Blocking — call inside `spawn_blocking`.
/// `None` on a miss OR any failure (both logged): a barcode we can't resolve is an empty result the
/// user can still enter by hand, never a 500.
pub fn by_barcode(barcode: &str) -> Option<BrandedFood> {
    let barcode = barcode.trim();
    if barcode.is_empty() || !barcode.chars().all(|c| c.is_ascii_digit()) {
        return None; // the path segment is interpolated — digits only, no exceptions
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let url = format!("{BASE}/product/{barcode}.json");
    let mut resp = agent
        .get(&url)
        .header("user-agent", USER_AGENT)
        .query("fields", FIELDS)
        .call()
        .inspect_err(|e| tracing::warn!(error = %e, "Open Food Facts lookup failed"))
        .ok()?;
    if !resp.status().is_success() {
        // 404 is OFF's ordinary "unknown barcode" answer — expected, not an incident.
        tracing::debug!(
            status = resp.status().as_u16(),
            "Open Food Facts had no product"
        );
        return None;
    }
    let body: ProductResponse = resp
        .body_mut()
        .read_json()
        .inspect_err(|e| tracing::warn!(error = %e, "Open Food Facts body did not parse"))
        .ok()?;
    if body.status != 1 {
        return None;
    }
    to_branded(barcode, body.product?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod tests {
    use super::*;

    fn product(json: serde_json::Value) -> Product {
        serde_json::from_value(json).unwrap()
    }

    /// The conversion that matters most: OFF stores minerals in GRAMS per 100 g. Getting this wrong
    /// is a 1000× error in somebody's iron intake, so it gets its own assertion per magnitude class.
    #[test]
    fn off_gram_normalized_micronutrients_convert_to_catalog_units() {
        let food = to_branded(
            "3017620422003",
            product(serde_json::json!({
                "code": "3017620422003",
                "product_name": "Hummus",
                "brands": "Store Brand, Store",
                "ingredients_text": "Chickpeas, tahini, olive oil, salt",
                "serving_quantity": 30,
                "nutriments": {
                    "energy-kcal_100g": 180.0,
                    "proteins_100g": 7.0,
                    "iron_100g": 0.0015,
                    "calcium_100g": 0.038,
                    "selenium_100g": 0.0000032,
                    "sodium_100g": 0.4
                }
            })),
        )
        .unwrap();
        assert_eq!(food.source, BrandedSource::Off);
        assert_eq!(food.external_id, "3017620422003");
        assert_eq!(
            food.brand.as_deref(),
            Some("Store Brand"),
            "first brand only"
        );
        assert_eq!(food.calories_per_100g, Some(180.0));
        assert_eq!(food.serving_grams, Some(30.0));

        let by_name = |n: &str| {
            food.nutrients
                .iter()
                .find(|r| r.name == n)
                .map(|r| (r.amount_per_100g, r.unit.clone()))
        };
        assert_eq!(by_name("Protein"), Some((7.0, "g".into())));
        assert_eq!(by_name("Iron"), Some((1.5, "mg".into())));
        assert_eq!(by_name("Calcium"), Some((38.0, "mg".into())));
        assert_eq!(by_name("Sodium"), Some((400.0, "mg".into())));
        let (selenium, unit) = by_name("Selenium").unwrap();
        assert!((selenium - 3.2).abs() < 1e-9, "selenium in ug: {selenium}");
        assert_eq!(unit, "ug");
        assert!(food.attribution.contains("ODbL"));
    }

    /// OFF is community-entered: the same field arrives as a number or a string.
    #[test]
    fn loosely_typed_values_are_accepted() {
        let food = to_branded(
            "1",
            product(serde_json::json!({
                "product_name": "Thing",
                "serving_quantity": "45",
                "nutriments": { "proteins_100g": "3.5", "energy-kcal_100g": "120" }
            })),
        )
        .unwrap();
        assert_eq!(food.serving_grams, Some(45.0));
        assert_eq!(food.calories_per_100g, Some(120.0));
        assert_eq!(food.nutrients[0].amount_per_100g, 3.5);
    }

    #[test]
    fn kilojoule_only_rows_still_get_calories() {
        let food = to_branded(
            "1",
            product(serde_json::json!({
                "product_name": "Thing",
                "nutriments": { "energy-kj_100g": 418.4 }
            })),
        )
        .unwrap();
        let kcal = food.calories_per_100g.unwrap();
        assert!(
            (kcal - 100.0).abs() < 1e-6,
            "418.4 kJ ≈ 100 kcal, got {kcal}"
        );
    }

    #[test]
    fn nonsense_community_values_are_refused() {
        let food = to_branded(
            "1",
            product(serde_json::json!({
                "product_name": "Thing",
                "nutriments": { "proteins_100g": -4.0, "iron_100g": "not a number" }
            })),
        )
        .unwrap();
        assert!(food.nutrients.is_empty());
    }

    #[test]
    fn an_unnamed_product_is_not_a_result() {
        assert!(to_branded("1", product(serde_json::json!({ "nutriments": {} }))).is_none());
        assert!(to_branded(
            "1",
            product(serde_json::json!({ "product_name": "  ", "nutriments": {} }))
        )
        .is_none());
    }

    /// The word-aware flags run on OFF text too — an OFF "Coconut Milk" must not be flagged dairy.
    #[test]
    fn diet_flags_are_word_aware_on_off_text() {
        let vegan = to_branded(
            "1",
            product(serde_json::json!({
                "product_name": "Coconut Milk",
                "ingredients_text": "Coconut extract, water, guar gum",
                "nutriments": {}
            })),
        )
        .unwrap();
        assert!(vegan.diet_flags.is_empty(), "{:?}", vegan.diet_flags);

        let dairy = to_branded(
            "2",
            product(serde_json::json!({
                "product_name": "Cheddar",
                "ingredients_text": "Pasteurized milk, salt, rennet",
                "nutriments": {}
            })),
        )
        .unwrap();
        let terms: Vec<_> = dairy.diet_flags.iter().map(|f| f.term.as_str()).collect();
        assert_eq!(terms, ["milk", "rennet"]);
    }

    /// The barcode is interpolated into the request path, so anything non-numeric is refused before a
    /// request is ever built.
    #[test]
    fn non_numeric_barcodes_never_reach_the_network() {
        assert!(by_barcode("").is_none());
        assert!(by_barcode("../../etc/passwd").is_none());
        assert!(by_barcode("123abc").is_none());
    }
}
