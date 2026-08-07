//! The USDA FoodData Central **API** client — the live half of gate D1's decided shape (on-demand
//! fetch + cache + promote-on-first-use).
//!
//! This is a different thing from `usda.rs`, and the distinction matters: `usda.rs` ingests a
//! PRE-BUILT artifact of the Foundation/SR **plants** catalog from S3 once at boot (~2k curated rows,
//! the communal reference catalog). This module talks to the live FDC **Branded** dataset — ~2M rows
//! nobody wants on a t4g.nano — one lookup at a time, only when a user actually searches or scans.
//! Every miss is a fresh fetch, which is also why the branded data is never stale: there is no refresh
//! job to fall behind, because there is no bulk copy.
//!
//! **The API key stays server-side.** Clients call our endpoints, never FDC — that is why the desktop
//! can remain a thin cache of whatever the server already promoted, and why a leaked-key story never
//! starts with "we shipped it in the app bundle".
//!
//! Transport is blocking `ureq` (already the workspace's HTTP client, via vegify-client-rs), called
//! from `spawn_blocking` — the same discipline every rusqlite touch in this server already uses. The
//! risky part of this module is the PARSING, not the socket, so that is what the tests cover: real
//! FDC response fixtures in, `BrandedFood` out. Live behaviour is verified by curling a running
//! server, exactly like the rest of the API.

use std::time::Duration;

use serde::Deserialize;
use vegify_core::{BrandedFood, BrandedSource, IngredientNutrientInput};

/// FDC's public API base. Not configurable: this IS the intended host, and a "configurable" third-party
/// base URL is a way to accidentally point a deployment at somebody else's mirror.
const BASE: &str = "https://api.nal.usda.gov/fdc/v1";

/// A hard ceiling on any single third-party call. The nano serves everything on one box; a hung
/// upstream must degrade a branded lookup, never the site.
const TIMEOUT: Duration = Duration::from_secs(8);

/// NDB nutrient number → the catalog's (name, unit). The SAME bridge `crates/usda-importer` uses for
/// the plants artifact, deliberately duplicated in the vocabulary it maps rather than shared: the
/// importer is a dev-time tool crate, and applications are leaves here (no shipped build may depend on
/// it). Keeping the two lists honest is what `nutrient_vocabulary_matches_the_importer` guards.
///
/// Branded foods report a much thinner nutrient set than Foundation foods (a label carries what the
/// label carries), so most of these simply won't appear on a given product — that's expected, and a
/// missing reading is recorded as missing rather than zero.
const NUTRIENTS: &[(&str, &str, &str)] = &[
    ("208", "Calories", "kcal"),
    ("205", "Carbohydrates", "g"),
    ("203", "Protein", "g"),
    ("204", "Total Fat", "g"),
    ("606", "Saturated Fat", "g"),
    ("645", "Monounsaturated Fat", "g"),
    ("646", "Polyunsaturated Fat", "g"),
    ("851", "Omega-3 Fatty Acids", "g"),
    ("675", "Omega-6 Fatty Acids", "g"),
    ("291", "Total Fiber", "g"),
    ("601", "Cholesterol", "mg"),
    ("301", "Calcium", "mg"),
    ("312", "Copper", "mg"),
    ("303", "Iron", "mg"),
    ("304", "Magnesium", "mg"),
    ("315", "Manganese", "mg"),
    ("305", "Phosphorus", "mg"),
    ("306", "Potassium", "mg"),
    ("317", "Selenium", "ug"),
    ("307", "Sodium", "mg"),
    ("309", "Zinc", "mg"),
    ("318", "Vitamin A", "IU"),
    ("415", "Vitamin B6", "mg"),
    ("418", "Vitamin B12", "ug"),
    ("401", "Vitamin C", "mg"),
    ("324", "Vitamin D", "IU"),
    ("323", "Vitamin E", "mg"),
    ("430", "Vitamin K", "ug"),
    ("404", "Thiamin", "mg"),
    ("405", "Riboflavin", "mg"),
    ("406", "Niacin", "mg"),
    ("435", "Folate", "ug"),
    ("417", "Folate", "ug"),
    ("410", "Pantothenic Acid", "mg"),
    ("421", "Choline", "mg"),
];

/// FDC's `unitName` spellings, normalized to the catalog's. A reading whose FDC unit does NOT match
/// what the vocabulary expects for that nutrient is DROPPED rather than coerced — a silently rescaled
/// micronutrient is exactly the kind of junk-data drift P2.5's trust layer exists to prevent, and a
/// missing reading is honest where a wrong one is not.
fn normalize_unit(fdc: &str) -> String {
    match fdc.trim().to_uppercase().as_str() {
        "G" | "GRM" => "g".into(),
        "MG" => "mg".into(),
        "UG" | "MCG" | "µG" => "ug".into(),
        "KCAL" => "kcal".into(),
        "IU" => "IU".into(),
        other => other.to_lowercase(),
    }
}

// ---- the FDC search response (the one call that carries everything we need) ----

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    foods: Vec<SearchFood>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchFood {
    fdc_id: i64,
    description: String,
    #[serde(default)]
    brand_owner: Option<String>,
    #[serde(default)]
    brand_name: Option<String>,
    #[serde(default)]
    gtin_upc: Option<String>,
    #[serde(default)]
    ingredients: Option<String>,
    #[serde(default)]
    serving_size: Option<f64>,
    #[serde(default)]
    serving_size_unit: Option<String>,
    #[serde(default)]
    food_nutrients: Vec<SearchNutrient>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchNutrient {
    #[serde(default)]
    nutrient_number: Option<String>,
    #[serde(default)]
    unit_name: Option<String>,
    #[serde(default)]
    value: Option<f64>,
}

/// Non-empty, trimmed — FDC returns `""` and `" "` for absent brand fields rather than omitting them.
fn clean(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Map one FDC search hit into the branded store's shape. Amounts on Branded rows are per 100 g,
/// which is already the catalog's canonical basis, so no rescaling happens here at all.
fn to_branded(food: SearchFood) -> BrandedFood {
    let mut calories_per_100g = None;
    let mut nutrients: Vec<IngredientNutrientInput> = Vec::new();
    for n in food.food_nutrients {
        let (Some(number), Some(value)) = (n.nutrient_number.as_deref(), n.value) else {
            continue;
        };
        let Some((_, name, unit)) = NUTRIENTS.iter().find(|(num, ..)| *num == number) else {
            continue;
        };
        // Unit sanity: trust our vocabulary's unit only when FDC agrees with it (see normalize_unit).
        if let Some(fdc_unit) = n.unit_name.as_deref() {
            if normalize_unit(fdc_unit) != *unit {
                continue;
            }
        }
        if *name == "Calories" {
            calories_per_100g.get_or_insert(value);
            continue; // calories live on the ingredient's own column, not as a reading
        }
        // Folate maps from two numbers (435 DFE preferred, 417 food folate fallback) — first wins.
        if nutrients.iter().any(|r| r.name == *name) {
            continue;
        }
        nutrients.push(IngredientNutrientInput {
            name: (*name).to_string(),
            amount_per_100g: value,
            unit: (*unit).to_string(),
        });
    }

    let name = food.description.trim().to_string();
    let ingredients_text = clean(food.ingredients);
    // FDC states the serving unit separately; only grams are canonical for us. A serving in "ml" or
    // "cup" is left unset rather than assumed to be 1 g per unit.
    let serving_grams = food.serving_size.filter(|_| {
        matches!(
            food.serving_size_unit
                .as_deref()
                .map(str::to_lowercase)
                .as_deref(),
            Some("g") | Some("grm") | Some("gram") | Some("grams")
        )
    });
    BrandedFood {
        source: BrandedSource::UsdaBranded,
        external_id: food.fdc_id.to_string(),
        gtin: clean(food.gtin_upc),
        brand: clean(food.brand_name).or_else(|| clean(food.brand_owner)),
        serving_grams,
        serving_unit: None,
        // Recomputed on every store read; set here so a freshly fetched food is already complete for
        // the response that returns it before the cache write lands. Product name AND statement: a
        // label that says "Milk Chocolate" in the name but omits an ingredient list still deserves
        // the flag.
        diet_flags: vegify_core::animal_derived_flags(&format!(
            "{name} {}",
            ingredients_text.as_deref().unwrap_or_default()
        )),
        name,
        ingredients_text,
        calories_per_100g,
        nutrients,
        ingredient_id: None,
        attribution: BrandedSource::UsdaBranded.attribution().to_string(),
    }
}

/// A shared agent: `http_status_as_error(false)` so a 429/403 from FDC (over quota, bad key) surfaces
/// as a response we can log with its status rather than an opaque transport error.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Run one FDC `/foods/search`, restricted to the Branded dataset. Blocking — call it inside
/// `spawn_blocking`. `None` on any failure (network, quota, malformed body), always logged: a branded
/// lookup that can't reach USDA degrades to "no results", never to a 500.
fn search_branded(query: &str, page_size: u32) -> Option<Vec<BrandedFood>> {
    let key = vegify_config::server::fdc_api_key();
    let url = format!("{BASE}/foods/search");
    let mut resp = agent()
        .get(&url)
        .query("api_key", &key)
        .query("query", query)
        .query("dataType", "Branded")
        .query("pageSize", page_size.to_string())
        .call()
        .inspect_err(
            |e| tracing::warn!(error = %e, "FDC search failed — serving cached results only"),
        )
        .ok()?;
    let status = resp.status();
    if !status.is_success() {
        tracing::warn!(
            status = status.as_u16(),
            "FDC search returned a non-success status (over quota, or the API key is missing/invalid \
             — VEGIFY_FDC_API_KEY; DEMO_KEY allows only ~30 requests/hour)"
        );
        return None;
    }
    let body: SearchResponse = resp
        .body_mut()
        .read_json()
        .inspect_err(|e| tracing::warn!(error = %e, "FDC search body did not parse"))
        .ok()?;
    Some(body.foods.into_iter().map(to_branded).collect())
}

/// Branded foods matching a free-text query (brand or product name). Blocking.
pub fn search(query: &str, limit: u32) -> Vec<BrandedFood> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    search_branded(query, limit).unwrap_or_default()
}

/// The branded food carrying `gtin`, if USDA has it. Blocking.
///
/// FDC has no barcode endpoint: the search index covers `gtinUpc`, so the barcode goes in as the
/// query and the results are then filtered to an EXACT GTIN match — the same word-vs-substring
/// discipline as everywhere else in this feature, applied to identifiers. A fuzzy "close enough"
/// barcode match would attach the wrong product's nutrition to somebody's diary.
pub fn by_gtin(gtin: &str) -> Option<BrandedFood> {
    let gtin = gtin.trim();
    if gtin.is_empty() {
        return None;
    }
    search_branded(gtin, 10)?
        .into_iter()
        .find(|f| f.gtin.as_deref().map(normalize_gtin) == Some(normalize_gtin(gtin)))
}

/// Compare barcodes by their digits with leading zeros stripped: the same product is printed as a
/// 12-digit UPC-A and stored as a 13- or 14-digit GTIN with left padding, and a string compare would
/// call those different products.
fn normalize_gtin(gtin: &str) -> String {
    let digits: String = gtin.chars().filter(char::is_ascii_digit).collect();
    digits.trim_start_matches('0').to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod tests {
    use super::*;

    /// A real-shaped FDC `/foods/search` hit (field names and unit spellings verbatim from the API).
    fn fixture(extra_nutrients: serde_json::Value) -> SearchFood {
        let mut base = serde_json::json!({
            "fdcId": 2011111,
            "description": "OAT DRINK, ORIGINAL",
            "brandOwner": "Oatly AB",
            "brandName": "OATLY",
            "gtinUpc": "0081345600012",
            "ingredients": "OAT BASE (WATER, OATS), RAPESEED OIL, CALCIUM CARBONATE",
            "servingSize": 240.0,
            "servingSizeUnit": "g",
            "foodNutrients": []
        });
        base["foodNutrients"] = extra_nutrients;
        serde_json::from_value(base).unwrap()
    }

    #[test]
    fn maps_a_branded_hit_into_the_store_shape() {
        let food = to_branded(fixture(serde_json::json!([
            { "nutrientNumber": "208", "unitName": "KCAL", "value": 50.0 },
            { "nutrientNumber": "301", "unitName": "MG",   "value": 120.0 },
            { "nutrientNumber": "203", "unitName": "G",    "value": 1.0 }
        ])));
        assert_eq!(food.source, BrandedSource::UsdaBranded);
        assert_eq!(food.external_id, "2011111");
        assert_eq!(food.name, "OAT DRINK, ORIGINAL");
        assert_eq!(food.brand.as_deref(), Some("OATLY"), "brandName wins");
        assert_eq!(food.gtin.as_deref(), Some("0081345600012"));
        assert_eq!(food.serving_grams, Some(240.0));
        assert_eq!(
            food.calories_per_100g,
            Some(50.0),
            "calories go on the column, not the readings"
        );
        let readings: Vec<_> = food
            .nutrients
            .iter()
            .map(|r| (r.name.as_str(), r.amount_per_100g, r.unit.as_str()))
            .collect();
        assert_eq!(readings, [("Calcium", 120.0, "mg"), ("Protein", 1.0, "g")]);
        assert!(food.attribution.contains("CC0"));
        assert!(food.ingredient_id.is_none(), "fetched ≠ promoted");
    }

    /// An oat drink must NOT flag dairy even though FDC's own text is full of the word "oat" — and a
    /// milk chocolate bar must flag from the NAME alone when the statement is absent.
    #[test]
    fn diet_flags_read_name_and_statement_word_aware() {
        let oat = to_branded(fixture(serde_json::json!([])));
        assert!(oat.diet_flags.is_empty());

        let mut raw = fixture(serde_json::json!([]));
        raw.description = "MILK CHOCOLATE BAR".into();
        raw.ingredients = None;
        let bar = to_branded(raw);
        assert_eq!(
            bar.diet_flags
                .iter()
                .map(|f| f.term.as_str())
                .collect::<Vec<_>>(),
            ["milk"]
        );
    }

    /// A reading whose FDC unit disagrees with the catalog's vocabulary is DROPPED, never coerced —
    /// a silently rescaled micronutrient is worse than a missing one.
    #[test]
    fn mismatched_units_are_dropped_not_coerced() {
        let food = to_branded(fixture(serde_json::json!([
            { "nutrientNumber": "301", "unitName": "G",  "value": 0.12 },
            { "nutrientNumber": "303", "unitName": "MG", "value": 1.5 }
        ])));
        let names: Vec<_> = food.nutrients.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Iron"], "calcium-in-grams is refused");
    }

    /// Folate arrives as either 435 (DFE, preferred) or 417 (food folate) — never both.
    #[test]
    fn folate_maps_once() {
        let food = to_branded(fixture(serde_json::json!([
            { "nutrientNumber": "435", "unitName": "UG", "value": 40.0 },
            { "nutrientNumber": "417", "unitName": "UG", "value": 12.0 }
        ])));
        assert_eq!(food.nutrients.len(), 1);
        assert_eq!(food.nutrients[0].amount_per_100g, 40.0, "DFE wins");
    }

    /// A serving stated in a non-gram unit is left unset rather than assumed to be 1 g per unit.
    #[test]
    fn non_gram_servings_are_not_assumed() {
        let mut raw = fixture(serde_json::json!([]));
        raw.serving_size_unit = Some("ml".into());
        assert_eq!(to_branded(raw).serving_grams, None);
    }

    #[test]
    fn blank_brand_fields_do_not_become_empty_strings() {
        let mut raw = fixture(serde_json::json!([]));
        raw.brand_name = Some("   ".into());
        raw.brand_owner = Some("Oatly AB".into());
        assert_eq!(to_branded(raw).brand.as_deref(), Some("Oatly AB"));
    }

    /// UPC-A on the package vs. zero-padded GTIN-13 in the database is the SAME product.
    #[test]
    fn gtin_comparison_ignores_leading_zeros() {
        assert_eq!(
            normalize_gtin("0081345600012"),
            normalize_gtin("81345600012")
        );
        assert_ne!(
            normalize_gtin("081345600012"),
            normalize_gtin("081345600013")
        );
    }

    #[test]
    fn unit_normalization_covers_fdcs_spellings() {
        assert_eq!(normalize_unit("MCG"), "ug");
        assert_eq!(normalize_unit("UG"), "ug");
        assert_eq!(normalize_unit("G"), "g");
        assert_eq!(normalize_unit("KCAL"), "kcal");
        assert_eq!(normalize_unit("IU"), "IU");
    }

    /// The importer's vocabulary and this module's must stay one vocabulary: a branded row and a
    /// plants row that disagree on a nutrient's name or unit would silently split every day total.
    /// (usda-importer is a dev-tool crate no shipped build may depend on, so the list is duplicated
    /// rather than imported — this test is what keeps the duplicate honest.)
    #[test]
    fn nutrient_vocabulary_matches_the_importer() {
        let importer = include_str!("../../../../crates/usda-importer/src/main.rs");
        for (number, name, unit) in NUTRIENTS {
            let entry = format!("(\"{number}\", \"{name}\", \"{unit}\")");
            assert!(
                importer.contains(&entry),
                "{entry} is missing from usda-importer's NUTRIENTS — the two vocabularies drifted"
            );
        }
    }
}
