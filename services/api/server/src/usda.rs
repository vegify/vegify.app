//! The USDA plant catalog — boot-time ingest of the processed FoodData Central artifact. The
//! artifact is DATA, so it lives in S3 (the server stack's Data bucket), not the repo: built by the
//! `usda-importer` crate (`just usda-data`), uploaded with `just usda-upload`, fetched here at boot
//! (`catalog/usda-plants.json.gz`, ~190 KB). Foundation + SR Legacy, plants only, public domain.
//!
//! Entries become COMMUNAL REFERENCE ingredients: unowned (user_id NULL — no user can edit them,
//! per the DAL's owner gate), public, slugged like any other ingredient, and stamped with
//! provenance (`source` = "USDA FoodData Central") per docs/usernames.md — never a fabricated user.
//!
//! Fetch-and-ingest is marker-gated (any row carrying the source marker ⇒ skip — a later boot costs
//! one COUNT and no S3 read). A missing bucket/object logs a warning and the server serves without
//! the catalog: reference data is an enhancement, never a boot blocker — upload it and the next
//! server deploy's boot picks it up.
use std::io::Read;

use flate2::read::GzDecoder;
use rusqlite::Connection;
use serde::Deserialize;

/// The artifact's object key in the data bucket.
pub const OBJECT_KEY: &str = "catalog/usda-plants.json.gz";

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

/// Whether the catalog still needs ingesting (no row carries the source marker).
pub fn catalog_missing(conn: &Connection) -> rusqlite::Result<bool> {
    let present: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ingredients WHERE source = ?1",
        [SOURCE],
        |r| r.get(0),
    )?;
    Ok(present == 0)
}

/// Fetch the artifact from the configured data bucket. `None` = not configured / not uploaded /
/// unreadable (each logged) — the caller serves without the catalog.
pub async fn fetch_artifact() -> Option<Vec<u8>> {
    let Some(bucket) = vegify_config::server::data_bucket() else {
        tracing::warn!("VEGIFY_DATA_BUCKET not set — serving without the USDA catalog");
        return None;
    };
    let cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .load()
        .await;
    let s3 = aws_sdk_s3::Client::new(&cfg);
    match s3.get_object().bucket(&bucket).key(OBJECT_KEY).send().await {
        Ok(out) => match out.body.collect().await {
            Ok(bytes) => Some(bytes.into_bytes().to_vec()),
            Err(e) => {
                tracing::warn!(error = %e, "catalog artifact read failed — serving without it");
                None
            }
        },
        Err(e) => {
            tracing::warn!(
                bucket, key = OBJECT_KEY, error = %e,
                "catalog artifact fetch failed (not uploaded yet? run `just usda-data && just usda-upload`) — serving without it"
            );
            None
        }
    }
}

/// Ingest the gzipped artifact (idempotence is the caller's marker check; a re-run on a seeded DB
/// would duplicate). One transaction: ~2k ingredients × ~30 readings, seconds once, and a crash
/// mid-way rolls back cleanly so the next boot retries.
pub fn ingest(conn: &Connection, gz: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
    let mut json = String::new();
    GzDecoder::new(gz).read_to_string(&mut json)?;
    let foods: Vec<SeedFood> = serde_json::from_str(&json)?;
    let total = foods.len();

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
                serving_unit: None,
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
            Ok(total)
        }
        Err(e) => {
            conn.execute_batch("ROLLBACK").ok();
            Err(e)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod tests {
    use super::*;
    use std::io::Write;

    /// The REAL client schema (the desktop ships it; its drift test pins it to the drizzle source),
    /// so this fixture can't silently diverge from production tables.
    const CLIENT_SCHEMA: &str = include_str!("../../../../apps/desktop/src-tauri/schema.sql");

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CLIENT_SCHEMA).unwrap();
        conn
    }

    /// A tiny artifact-shaped fixture, gzipped like the real thing (this is TEST DATA — the real
    /// artifact lives in S3, not the repo).
    fn fixture_gz() -> Vec<u8> {
        let json = serde_json::json!([
            { "name": "Test Broccoli, raw", "category": "Vegetables and Vegetable Products",
              "nutrients": [
                { "name": "Calories", "unit": "kcal", "amountPer100g": 31.0 },
                { "name": "Iron", "unit": "mg", "amountPer100g": 0.69 },
                { "name": "Vitamin C", "unit": "mg", "amountPer100g": 91.3 }
              ] },
            { "name": "Test Lentils, cooked", "category": "Legumes and Legume Products",
              "nutrients": [
                { "name": "Calories", "unit": "kcal", "amountPer100g": 116.0 },
                { "name": "Protein", "unit": "g", "amountPer100g": 9.02 }
              ] }
        ])
        .to_string();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(json.as_bytes()).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn ingest_creates_unowned_public_sourced_slugged_rows_and_marker_gates() {
        let conn = test_conn();
        assert!(catalog_missing(&conn).unwrap());
        let n = ingest(&conn, &fixture_gz()).unwrap();
        assert_eq!(n, 2);
        assert!(
            !catalog_missing(&conn).unwrap(),
            "marker present after ingest"
        );
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
        assert_eq!(
            (owned, nonpublic, unslugged),
            (0, 0, 0),
            "unowned, public, slugged"
        );
        // Calories split onto the card column; the rest are nutrient readings.
        let (cal, readings): (f64, i64) = conn
            .query_row(
                "SELECT i.calories_per_100g,
                        (SELECT COUNT(*) FROM ingredient_nutrient WHERE ingredient_id = i.id)
                   FROM ingredients i WHERE i.name = 'Test Broccoli, raw'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(cal, 31.0);
        assert_eq!(
            readings, 2,
            "Iron + Vitamin C (Calories lives on the column)"
        );
    }

    #[test]
    fn corrupt_artifact_rolls_back_leaving_no_marker() {
        let conn = test_conn();
        assert!(ingest(&conn, b"not gzip at all").is_err());
        assert!(
            catalog_missing(&conn).unwrap(),
            "failed ingest leaves a clean slate for retry"
        );
    }
}
