//! `import-completefoods` — move a CompleteFoods account capture (.data/import/completefoods/,
//! gitignored personal data) into vegify through the live API as the signed-in user.
//!
//! The capture's semantics (verified against its README + known USDA values):
//! - recipes/*.json: `canEdit: true` = the user's own (89 of 90); each ingredient row carries
//!   `amount` (grams IN the recipe) and `serving` (grams its nutrient values are PER) — so
//!   per-100g = value × 100 / serving. Zero-amount rows are pantry residue and are skipped.
//! - Nutrient keys are CF's own (typos included: `maganese`, `selinium`, `panthothenic`); units
//!   come from its dictionary and are CONVERTED to the catalog's units (the mg-class minerals CF
//!   stores in grams get ×1000; Vitamin E IU → mg α-tocopherol at 1.49 IU/mg) so one nutrient name
//!   means one unit everywhere in vegify.
//! - CF's food db is USDA-derived and uses VERBATIM SR names ("Wheat flour, white, all-purpose,
//!   enriched, unbleached") — so resolution against the ingested USDA catalog is an exact
//!   case-insensitive name match, sanity-checked by calories proximity (±15%); everything else
//!   (Amazon/store products, hand-entered foods) becomes the user's own custom ingredient.
//!
//! DRY-RUN BY DEFAULT: prints the full resolution report. `--yes` creates the custom ingredients
//! (deduped by name across recipes) then the recipes. Re-runnable: a recipe whose title the user
//! already owns is skipped, so a crashed run resumes instead of duplicating.
use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{login, pull, PullRow};

const SRC: &str = ".data/import/completefoods";

/// CF row key → (vegify nutrient name, unit, CF→vegify factor). Names + units match the USDA
/// catalog bridge exactly, so imported customs and catalog entries speak one vocabulary. Factor
/// converts CF's stored unit to ours: ×1000 = CF grams → mg; Vitamin E = IU → mg (1.49 IU/mg).
/// CF-only nutrients the catalog doesn't track (soluble/insoluble fiber, chloride, chromium,
/// iodine, molybdenum, sulfur, biotin) still import — they're real readings of the user's foods.
const NUTRIENTS: [(&str, &str, &str, f64); 42] = [
    ("calories", "Calories", "kcal", 1.0),
    ("carbs", "Carbohydrates", "g", 1.0),
    ("protein", "Protein", "g", 1.0),
    ("fat", "Total Fat", "g", 1.0),
    ("saturated-fat", "Saturated Fat", "g", 1.0),
    ("monounsaturated-fat", "Monounsaturated Fat", "g", 1.0),
    ("polyunsaturated-fat", "Polyunsaturated Fat", "g", 1.0),
    ("omega_3", "Omega-3 Fatty Acids", "g", 1.0),
    ("omega_6", "Omega-6 Fatty Acids", "g", 1.0),
    ("fiber", "Total Fiber", "g", 1.0),
    ("soluble-fiber", "Soluble Fiber", "g", 1.0),
    ("insoluble-fiber", "Insoluble Fiber", "g", 1.0),
    ("cholesterol", "Cholesterol", "mg", 1.0),
    ("calcium", "Calcium", "mg", 1000.0),
    ("chloride", "Chloride", "mg", 1000.0),
    ("chromium", "Chromium", "ug", 1.0),
    ("copper", "Copper", "mg", 1.0),
    ("iodine", "Iodine", "ug", 1.0),
    ("iron", "Iron", "mg", 1.0),
    ("magnesium", "Magnesium", "mg", 1.0),
    ("maganese", "Manganese", "mg", 1.0),
    ("molybdenum", "Molybdenum", "ug", 1.0),
    ("phosphorus", "Phosphorus", "mg", 1000.0),
    ("potassium", "Potassium", "mg", 1000.0),
    ("selinium", "Selenium", "ug", 1.0),
    ("sodium", "Sodium", "mg", 1000.0),
    ("sulfur", "Sulfur", "mg", 1000.0),
    ("zinc", "Zinc", "mg", 1.0),
    ("vitamin_a", "Vitamin A", "IU", 1.0),
    ("vitamin_b6", "Vitamin B6", "mg", 1.0),
    ("vitamin_b12", "Vitamin B12", "ug", 1.0),
    ("vitamin_c", "Vitamin C", "mg", 1.0),
    ("vitamin_d", "Vitamin D", "IU", 1.0),
    ("vitamin_e", "Vitamin E", "mg", 1.0 / 1.49),
    ("vitamin_k", "Vitamin K", "ug", 1.0),
    ("thiamin", "Thiamin", "mg", 1.0),
    ("riboflavin", "Riboflavin", "mg", 1.0),
    ("niacin", "Niacin", "mg", 1.0),
    ("folate", "Folate", "ug", 1.0),
    ("panthothenic", "Pantothenic Acid", "mg", 1.0),
    ("biotin", "Biotin", "ug", 1.0),
    ("choline", "Choline", "mg", 1.0),
];

#[derive(Deserialize)]
struct CfRecipe {
    #[serde(default)]
    title: Option<String>,
    // Option: the capture's draft recipe carries a literal `canEdit: null`.
    #[serde(default, rename = "canEdit")]
    can_edit: Option<bool>,
    #[serde(default)]
    ingredients: Option<Vec<serde_json::Value>>,
}

/// A CF ingredient row distilled: per-100g readings in vegify units.
struct CfIngredient {
    name: String,
    serving_grams: Option<f64>,
    calories_per_100g: Option<f64>,
    readings: Vec<(String, String, f64)>, // (name, unit, amount_per_100g)
}

/// One recipe item: which CF ingredient (by name) at how many grams.
struct CfItem {
    name: String,
    grams: f64,
}

/// The five entities CF's export escapes (verified against the capture: &amp; and &quot; occur).
fn unescape_html(t: &str) -> String {
    t.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .trim()
        .to_string()
}

fn distill(row: &serde_json::Value) -> Option<(CfIngredient, CfItem)> {
    let name = row.get("name")?.as_str()?.trim().to_string();
    let grams = row.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if name.is_empty() || grams <= 0.0 {
        return None; // pantry residue (295 zero-amount rows in the capture)
    }
    let serving = row
        .get("serving")
        .and_then(|v| v.as_f64())
        .filter(|s| *s > 0.0);
    let mut calories = None;
    let mut readings = Vec::new();
    if let Some(serving) = serving {
        let scale = 100.0 / serving;
        for (cf_key, out_name, unit, factor) in NUTRIENTS {
            let Some(v) = row.get(cf_key).and_then(|v| v.as_f64()) else {
                continue;
            };
            if v <= 0.0 {
                continue;
            }
            let per_100g = v * factor * scale;
            if out_name == "Calories" {
                calories = Some(per_100g);
            } else {
                readings.push((out_name.to_string(), unit.to_string(), per_100g));
            }
        }
    }
    Some((
        CfIngredient {
            name: name.clone(),
            serving_grams: serving,
            calories_per_100g: calories,
            readings,
        },
        CfItem { name, grams },
    ))
}

/// Catalog resolution: exact case-insensitive name hit + calories within ±15% (or both absent).
fn catalog_match<'a>(
    catalog: &'a BTreeMap<String, &'a PullRow>,
    cf: &CfIngredient,
) -> Result<Option<&'a PullRow>, String> {
    let Some(hit) = catalog.get(&cf.name.to_lowercase()) else {
        return Ok(None);
    };
    match (cf.calories_per_100g, hit.calories_per_100g) {
        (Some(a), Some(b)) if (a - b).abs() > (0.15 * b).max(15.0) => Err(format!(
            "name-hit but calorie mismatch (CF {a:.0} vs catalog {b:.0}/100g) — creating a custom instead"
        )),
        _ => Ok(Some(hit)),
    }
}

pub fn run(execute: bool) {
    let session = match login() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("login failed: {e}");
            std::process::exit(1);
        }
    };
    println!("signed in as @{}", session.user.username);

    // Load the capture: the user's own recipes, distilled.
    let mut recipes: Vec<(String, Vec<CfItem>)> = Vec::new();
    let mut empty_recipes: Vec<String> = Vec::new();
    let mut ingredients: BTreeMap<String, CfIngredient> = BTreeMap::new(); // by name, first wins
    let mut skipped_rows = 0usize;
    let dir = std::path::Path::new(SRC).join("recipes");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            eprintln!("cannot read {}: {e}", dir.display());
            std::process::exit(1);
        })
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    for path in files {
        let rec: CfRecipe = match serde_json::from_str(&std::fs::read_to_string(&path).unwrap()) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("SKIPPED unparseable {}: {e}", path.display());
                continue;
            }
        };
        if !rec.can_edit.unwrap_or(false) {
            continue; // not the user's recipe (or the capture's null draft)
        }
        // CF exports titles HTML-escaped ("Macs &amp; Cheese", "&quot;fake&quot; sweet").
        let title = unescape_html(&rec.title.unwrap_or_default());
        if title.is_empty() {
            continue;
        }
        let mut items = Vec::new();
        for row in rec.ingredients.as_deref().unwrap_or(&[]) {
            match distill(row) {
                Some((ing, item)) => {
                    ingredients.entry(ing.name.clone()).or_insert(ing);
                    items.push(item);
                }
                None => skipped_rows += 1,
            }
        }
        if items.is_empty() {
            // Every row was pantry residue (amount 0) — an empty CF template, nothing to import.
            empty_recipes.push(title);
        } else {
            recipes.push((title, items));
        }
    }

    // The current world: catalog index (unowned rows) + the user's existing recipe titles (re-runs
    // skip them) — one pull serves both.
    let world = pull(&session.token).expect("content pull failed");
    let catalog: BTreeMap<String, &PullRow> = world
        .ingredients
        .iter()
        .filter(|r| r.user_id.is_none())
        .map(|r| (r.name.to_lowercase(), r))
        .collect();
    let existing_titles: std::collections::HashSet<String> = world
        .recipes
        .iter()
        .filter(|r| r.user_id.as_deref() == Some(session.user.id.as_str()))
        .map(|r| r.name.to_lowercase())
        .collect();

    // Resolve every unique ingredient.
    let mut matched: Vec<(String, String)> = Vec::new(); // cf name → catalog id
    let mut to_create: Vec<&CfIngredient> = Vec::new();
    for ing in ingredients.values() {
        match catalog_match(&catalog, ing) {
            Ok(Some(hit)) => matched.push((ing.name.clone(), hit.id.clone())),
            Ok(None) => to_create.push(ing),
            Err(why) => {
                println!("  ! {}: {why}", ing.name);
                to_create.push(ing);
            }
        }
    }

    if !empty_recipes.is_empty() {
        println!(
            "\nEMPTY (all-pantry CF templates, not imported): {}",
            empty_recipes.join(" · ")
        );
    }
    println!(
        "\nRESOLUTION: {} recipes ({} already exist, will skip), {} unique ingredients → {} catalog matches, {} customs to create; {} zero-amount rows skipped",
        recipes.len(),
        recipes.iter().filter(|(t, _)| existing_titles.contains(&t.to_lowercase())).count(),
        ingredients.len(),
        matched.len(),
        to_create.len(),
        skipped_rows
    );
    for (cf, _) in &matched {
        println!("  catalog: {cf}");
    }
    for ing in &to_create {
        println!("  custom:  {} ({} readings)", ing.name, ing.readings.len());
    }
    if !execute {
        println!("\nDRY RUN — nothing imported. Re-run with --yes to import.");
        return;
    }

    // Create customs, then recipes.
    let mut ids: BTreeMap<String, String> = matched.into_iter().collect();
    for ing in to_create {
        let body = serde_json::json!({
            "name": ing.name,
            "visibility": "public",
            "caloriesPer100g": ing.calories_per_100g,
            "servingGrams": ing.serving_grams,
            "nutrients": ing.readings.iter().map(|(name, unit, amount)| serde_json::json!({
                "name": name, "unit": unit, "amountPer100g": amount
            })).collect::<Vec<_>>(),
        });
        match session.post_content("ingredients", &body) {
            Ok(id) => {
                println!("created ingredient: {}", ing.name);
                ids.insert(ing.name.clone(), id);
            }
            Err(e) => eprintln!("FAILED ingredient {}: {e}", ing.name),
        }
    }
    let mut done = 0usize;
    for (title, items) in &recipes {
        if existing_titles.contains(&title.to_lowercase()) {
            println!("exists, skipped: {title}");
            continue;
        }
        let wire_items: Vec<_> = items
            .iter()
            .filter_map(|it| {
                let id = ids.get(&it.name)?;
                Some(serde_json::json!({ "ingredientId": id, "grams": it.grams }))
            })
            .collect();
        if wire_items.len() != items.len() {
            eprintln!(
                "SKIPPED recipe {title}: {} of {} items unresolved",
                items.len() - wire_items.len(),
                items.len()
            );
            continue;
        }
        let body =
            serde_json::json!({ "name": title, "visibility": "public", "items": wire_items });
        match session.post_content("recipes", &body) {
            Ok(_) => {
                done += 1;
                println!("created recipe: {title}");
            }
            Err(e) => eprintln!("FAILED recipe {title}: {e}"),
        }
    }
    println!("\ndone. {done} recipes imported.");
}
