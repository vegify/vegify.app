//! Build the USDA catalog artifact from the raw FoodData
//! Central downloads — PLANTS ONLY (the product decision). The artifact is DATA and lives in S3
//! (the server stack's Data bucket), NOT the repo: this tool writes .data/build/ (gitignored);
//! `just usda-upload` ships it to the bucket (name resolved from SSM); the server ingests at boot.
//! A dedicated dev-tool crate — no shipped build depends on it. Run from the repo root:
//!
//!   cargo run -p usda-importer        # or: just usda-data, then just usda-upload
//!
//! Inputs (gitignored, .data/import/usda/, public domain): the LATEST Foundation release present
//! (semi-annual, April/October — download from fdc.nal.usda.gov/download-datasets; the newest file
//! by date wins) + SR Legacy 2018-04 (USDA's final Standard Reference — it never updates).
//! Selection: the six unambiguous plant categories (oils/beverages/sweets are mixed bags — curate
//! later if wanted). Nutrients: the 42-field dictionary from the completefoods capture
//! (nutrient-meta.json's usdaId tagnames) mapped to classic NDB nutrient numbers — the SAME bridge
//! the CompleteFoods import uses, so the two datasets speak one nutrient vocabulary. Units are
//! FDC-native (Vitamin E in mg, not CF's IU — importers convert, the catalog stays honest).
//! Amounts in both datasets are per 100 g — exactly vegify's ingredient_nutrient model.
//! Dedupe: Foundation (newer, analytically measured) wins over SR Legacy on a normalized-name tie.
use std::collections::HashMap;
use std::io::Write;

use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

const SRC: &str = ".data/import/usda";
const OUT: &str = ".data/build/usda-plants.json.gz";

const PLANT_CATEGORIES: [&str; 6] = [
    "Vegetables and Vegetable Products",
    "Fruits and Fruit Juices",
    "Legumes and Legume Products",
    "Nut and Seed Products",
    "Cereal Grains and Pasta",
    "Spices and Herbs",
];

/// NDB nutrient number → catalog (name, unit). Names are the completefoods dictionary's display
/// titles (its import maps 1:1); numbers are the classic NDB ids FDC still carries on every
/// nutrient row. Folate prefers DFE (435) with food folate (417) as fallback — see below.
const NUTRIENTS: [(&str, &str, &str); 35] = [
    ("208", "Calories", "kcal"),
    ("205", "Carbohydrates", "g"),
    ("203", "Protein", "g"),
    ("204", "Total Fat", "g"),
    ("606", "Saturated Fat", "g"),
    ("645", "Monounsaturated Fat", "g"),
    ("646", "Polyunsaturated Fat", "g"),
    ("851", "Omega-3 Fatty Acids", "g"), // 18:3 n-3 (ALA) — the plant omega-3
    ("675", "Omega-6 Fatty Acids", "g"), // 18:2 n-6
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
    ("323", "Vitamin E", "mg"), // FDC-native mg alpha-tocopherol (CF displays IU; its import converts)
    ("430", "Vitamin K", "ug"),
    ("404", "Thiamin", "mg"),
    ("405", "Riboflavin", "mg"),
    ("406", "Niacin", "mg"),
    ("435", "Folate", "ug"), // DFE, preferred
    ("417", "Folate", "ug"), // food folate, fallback when 435 absent
    ("410", "Pantothenic Acid", "mg"),
    ("421", "Choline", "mg"),
];
const FOLATE_DFE: &str = "435";
const FOLATE_FOOD: &str = "417";

// The arrays are Vec<Option<Food>>: the 2026-04 Foundation release ships literal `null` elements
// (discovered by parse failure) — tolerate and skip them rather than trusting USDA's JSON hygiene.
#[derive(Deserialize)]
struct FoundationFile {
    #[serde(rename = "FoundationFoods")]
    foods: Vec<Option<Food>>,
}

#[derive(Deserialize)]
struct LegacyFile {
    #[serde(rename = "SRLegacyFoods")]
    foods: Vec<Option<Food>>,
}

#[derive(Deserialize)]
struct Food {
    description: String,
    #[serde(rename = "fdcId")]
    fdc_id: Option<i64>,
    #[serde(rename = "foodCategory")]
    category: Option<Category>,
    #[serde(rename = "foodNutrients", default)]
    nutrients: Vec<FoodNutrient>,
}

#[derive(Deserialize)]
struct Category {
    description: Option<String>,
}

#[derive(Deserialize)]
struct FoodNutrient {
    nutrient: Option<NutrientRef>,
    amount: Option<f64>,
}

#[derive(Deserialize)]
struct NutrientRef {
    number: Option<String>,
}

// The artifact shape — field names match the previous generator byte-for-byte so the embedded
// consumer (src/usda.rs) and any diff of the committed artifact stay stable.
#[derive(Serialize)]
struct Entry {
    #[serde(rename = "fdcId")]
    fdc_id: Option<i64>,
    name: String,
    category: String,
    nutrients: Vec<OutNutrient>,
    dataset: &'static str,
}

#[derive(Serialize)]
struct OutNutrient {
    name: String,
    unit: String,
    #[serde(rename = "amountPer100g")]
    amount_per_100g: f64,
}

fn extract(food: Food, dataset: &'static str) -> Option<Entry> {
    let category = food.category.and_then(|c| c.description)?;
    if !PLANT_CATEGORIES.contains(&category.as_str()) {
        return None;
    }
    let map: HashMap<&str, (&str, &str)> = NUTRIENTS
        .iter()
        .map(|(num, name, unit)| (*num, (*name, *unit)))
        .collect();
    let mut by_number: HashMap<String, f64> = HashMap::new();
    for fnut in food.nutrients {
        let Some(number) = fnut.nutrient.and_then(|n| n.number) else {
            continue;
        };
        if !map.contains_key(number.as_str()) {
            continue;
        }
        let Some(amount) = fnut.amount else { continue };
        if amount.is_nan() {
            continue;
        }
        by_number.insert(number, amount);
    }
    // Folate: one row, DFE preferred.
    if by_number.contains_key(FOLATE_DFE) {
        by_number.remove(FOLATE_FOOD);
    }
    if by_number.is_empty() {
        return None;
    }
    // Deterministic artifact: nutrient rows ordered by NDB number.
    let mut rows: Vec<(String, f64)> = by_number.into_iter().collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let nutrients = rows
        .into_iter()
        .map(|(number, amount)| {
            let (name, unit) = map[number.as_str()];
            OutNutrient {
                name: name.to_string(),
                unit: unit.to_string(),
                amount_per_100g: amount,
            }
        })
        .collect();
    Some(Entry {
        fdc_id: food.fdc_id,
        name: food.description.trim().to_string(),
        category,
        nutrients,
        dataset,
    })
}

/// The lexically-latest Foundation JSON in the source dir — the filenames carry ISO dates, so byte
/// order IS release order. Foundation releases semi-annually (April/October); SR Legacy is USDA's
/// FINAL Standard Reference release (2018-04, "will not be updated") and stays pinned.
fn latest_foundation_path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let mut candidates: Vec<_> = std::fs::read_dir(SRC)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with("FoodData_Central_foundation_food_json_") && n.ends_with(".json")
            })
        })
        .collect();
    candidates.sort();
    candidates.pop().ok_or_else(|| {
        format!("no Foundation JSON in {SRC} — download it from fdc.nal.usda.gov/download-datasets")
            .into()
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let foundation_path = latest_foundation_path()?;
    println!("foundation source: {}", foundation_path.display());
    let foundation: FoundationFile =
        serde_json::from_str(&std::fs::read_to_string(&foundation_path)?)?;
    let legacy: LegacyFile = serde_json::from_str(&std::fs::read_to_string(format!(
        "{SRC}/FoodData_Central_sr_legacy_food_json_2018-04.json"
    ))?)?;

    let mut out: HashMap<String, Entry> = HashMap::new(); // normalized name → entry; Foundation first, wins ties
    let mut from_foundation = 0usize;
    for food in foundation.foods.into_iter().flatten() {
        if let Some(e) = extract(food, "foundation") {
            out.insert(e.name.to_lowercase(), e);
            from_foundation += 1;
        }
    }
    let (mut from_legacy, mut shadowed) = (0usize, 0usize);
    for food in legacy.foods.into_iter().flatten() {
        let Some(e) = extract(food, "sr-legacy") else {
            continue;
        };
        let key = e.name.to_lowercase();
        if out.contains_key(&key) {
            shadowed += 1;
            continue;
        }
        out.insert(key, e);
        from_legacy += 1;
    }

    let mut entries: Vec<Entry> = out.into_values().collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    let json = serde_json::to_string(&entries)?;

    std::fs::create_dir_all(".data/build")?;
    let mut gz = GzEncoder::new(std::fs::File::create(OUT)?, Compression::best());
    gz.write_all(json.as_bytes())?;
    let bytes = gz.finish()?.metadata()?.len();

    let counts: Vec<usize> = entries.iter().map(|e| e.nutrients.len()).collect();
    let avg = counts.iter().sum::<usize>() as f64 / entries.len() as f64;
    println!("wrote {OUT}");
    println!(
        "foods: {} (foundation {from_foundation}, sr-legacy {from_legacy}, shadowed {shadowed})",
        entries.len()
    );
    println!(
        "nutrients/food avg {avg:.1}, min {}, max {}",
        counts.iter().min().unwrap(),
        counts.iter().max().unwrap()
    );
    println!(
        "raw {:.1} MB → gz {} KB",
        json.len() as f64 / 1e6,
        bytes / 1000
    );
    let mut per_cat: HashMap<&str, usize> = HashMap::new();
    for e in &entries {
        *per_cat.entry(e.category.as_str()).or_default() += 1;
    }
    let mut cats: Vec<_> = per_cat.into_iter().collect();
    cats.sort();
    for (c, n) in cats {
        println!("  {c}: {n}");
    }
    Ok(())
}
