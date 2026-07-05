//! The USDA plant catalog — boot-time ingest of the embedded, processed FoodData Central dataset
//! (`data/usda-plants.json.gz`, built by `.scripts/usda-process.mjs`; Foundation + SR Legacy,
//! plants only, public domain). Entries become COMMUNAL REFERENCE ingredients: unowned (user_id
//! NULL — no user can edit them, per the DAL's owner gate), public, slugged like any other
//! ingredient, and stamped with provenance (`source` = "USDA FoodData Central") per
//! docs/usernames.md — never a fabricated user.
//!
//! Idempotent like the blog seed: any row carrying the source marker means the catalog is present
//! and the whole pass is skipped, so a boot after the first costs one COUNT. Inserts go through
//! vegify_core::do_save_ingredient — the same DAL user content uses — so slugs, nutrient rows, and
//! visibility can't drift from the app's own semantics.
use std::io::Read;

use flate2::read::GzDecoder;
use rusqlite::Connection;
use serde::Deserialize;

const SEED_GZ: &[u8] = include_bytes!("../data/usda-plants.json.gz");

/// The provenance marker — also the CompleteFoods importer's match target for catalog mapping.
pub const SOURCE: &str = "USDA FoodData Central";

#[derive(Deserialize)]
struct SeedFood {
    name: String,
    nutrients: Vec<SeedNutrient>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedNutrient {
    name: String,
    unit: String,
    amount_per_100g: f64,
}

/// Ingest the catalog into an instance that doesn't have it (marker: any `source` = [`SOURCE`] row).
pub fn seed_if_empty(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let present: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ingredients WHERE source = ?1",
        [SOURCE],
        |r| r.get(0),
    )?;
    if present > 0 {
        return Ok(());
    }
    let mut json = String::new();
    GzDecoder::new(SEED_GZ).read_to_string(&mut json)?;
    let foods: Vec<SeedFood> = serde_json::from_str(&json)?;
    let total = foods.len();

    // One transaction: ~2k ingredients × ~30 readings; a one-time boot cost of seconds, and a crash
    // mid-way leaves no marker rows behind (all-or-nothing), so the next boot retries cleanly.
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for food in foods {
            // Calories live on the card column; the remaining readings become nutrient rows (the
            // nutrition-facts panel's source) — mirrors how the ingredient form splits them.
            let calories = food
                .nutrients
                .iter()
                .find(|n| n.name == "Calories")
                .map(|n| n.amount_per_100g);
            let input = vegify_core::SaveIngredientInput {
                id: None,
                visibility: Some(vegify_core::Visibility::Public),
                name: food.name,
                description: None,
                price: None,
                calories_per_100g: calories,
                serving_grams: None,
                package_grams: None,
                nutrients: food
                    .nutrients
                    .into_iter()
                    .filter(|n| n.name != "Calories")
                    .map(|n| vegify_core::IngredientNutrientInput {
                        name: n.name,
                        amount_per_100g: n.amount_per_100g,
                        unit: n.unit,
                    })
                    .collect(),
                slug: None, // the DAL generates a unique catalog slug, like any user create
            };
            let id = vegify_core::do_save_ingredient(conn, &input, None)?;
            conn.execute(
                "UPDATE ingredients SET source = ?1 WHERE id = ?2",
                rusqlite::params![SOURCE, id],
            )?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            tracing::info!(foods = total, "USDA plant catalog seeded");
            Ok(())
        }
        Err(e) => {
            conn.execute_batch("ROLLBACK").ok();
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The REAL client schema (the desktop ships it; its drift test pins it to the drizzle source),
    /// so this fixture can't silently diverge from production tables.
    const CLIENT_SCHEMA: &str = include_str!("../../../apps/desktop/src-tauri/schema.sql");

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CLIENT_SCHEMA).unwrap();
        conn
    }

    #[test]
    fn seeds_once_idempotently_with_unowned_public_sourced_rows() {
        let conn = test_conn();
        seed_if_empty(&conn).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM ingredients WHERE source = ?1", [SOURCE], |r| r.get(0))
            .unwrap();
        assert!(n > 1500, "the plant catalog should be substantial (got {n})");
        let (owned, nonpublic, unslugged): (i64, i64, i64) = conn
            .query_row(
                "SELECT
                   SUM(CASE WHEN user_id IS NOT NULL THEN 1 ELSE 0 END),
                   SUM(CASE WHEN visibility != 'public' THEN 1 ELSE 0 END),
                   SUM(CASE WHEN slug IS NULL OR slug = '' THEN 1 ELSE 0 END)
                 FROM ingredients WHERE source = ?1",
                [SOURCE],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((owned, nonpublic, unslugged), (0, 0, 0), "unowned, public, slugged");
        // Readings landed for a staple.
        let broccoli_iron: f64 = conn
            .query_row(
                "SELECT inut.amount_per_100g FROM ingredients i
                   JOIN ingredient_nutrient inut ON inut.ingredient_id = i.id
                   JOIN nutrients nu ON nu.id = inut.nutrient_id
                  WHERE i.name = 'Broccoli, raw' AND nu.name = 'Iron'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(broccoli_iron > 0.0 && broccoli_iron < 5.0, "sane per-100g iron (got {broccoli_iron})");
        // Idempotent: a second boot adds nothing.
        seed_if_empty(&conn).unwrap();
        let n2: i64 = conn
            .query_row("SELECT COUNT(*) FROM ingredients WHERE source = ?1", [SOURCE], |r| r.get(0))
            .unwrap();
        assert_eq!(n, n2);
    }
}
