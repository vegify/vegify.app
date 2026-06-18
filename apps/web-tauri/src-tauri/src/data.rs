//! On-device data-access layer (DAL) for the Tauri desktop shell.
//!
//! The `#[procedures]` trait below IS the typed contract; the same surface is implemented
//! server-side in TypeScript (`@vegify/db`) for the web shell — two bindings of one interface.
//! Backed by rusqlite (bundled SQLite, sync). Nutrition is ONE recursive CTE, ported verbatim
//! from `packages/db/src/nutrition.ts` (normalized to per-100g), so the shared React
//! `NutritionFacts` component consumes the result unchanged.

use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use specta::Type;
use ttipc::procedures;

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
    pub id: i32,
    pub name: String,
    pub amount: Amount,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeView {
    pub id: i32,
    pub name: String,
    pub subtitle: Option<String>,
    pub directions: Option<String>,
    pub creator: Option<String>,
    pub serving: Option<Amount>,
    pub batch_grams: Option<f64>,
    pub items: Vec<RecipeItem>,
    pub nutrition: AggregatedNutrition,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeCard {
    pub id: i32,
    pub name: String,
    pub subtitle: Option<String>,
}

#[derive(Debug, ttipc::Error)]
pub enum DataError {
    Db(String),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataError::Db(m) => write!(f, "{m}"),
        }
    }
}

impl From<rusqlite::Error> for DataError {
    fn from(e: rusqlite::Error) -> Self {
        DataError::Db(e.to_string())
    }
}

/// Same recursive CTE as packages/db/src/nutrition.ts, normalized to per-100g: walk the
/// recipe→ingredient graph from the target ingredient, carrying each leaf's effective grams
/// within the batch (eff_grams) and the batch total (denom); per-100g = SUM(per100g·eff/denom).
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

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")
            .ok();
        Ok(Self { conn: Mutex::new(conn) })
    }
}

fn aggregate_per100g(conn: &Connection, ingredient_id: i64) -> Result<AggregatedNutrition, DataError> {
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

#[procedures]
pub trait VegifyData {
    fn list_recipes(&self) -> Result<Vec<RecipeCard>, DataError>;
    fn recipe(&self, id: i32) -> Result<Option<RecipeView>, DataError>;
}

impl VegifyData for Db {
    fn list_recipes(&self) -> Result<Vec<RecipeCard>, DataError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT r.id, i.name, r.subtitle
             FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id
             ORDER BY i.name",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(RecipeCard { id: row.get(0)?, name: row.get(1)?, subtitle: row.get(2)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn recipe(&self, id: i32) -> Result<Option<RecipeView>, DataError> {
        let conn = self.conn.lock().unwrap();
        let meta = conn
            .query_row(
                "SELECT i.id, i.name, r.subtitle, r.directions, u.name,
                        sa.amount, sa.unit, sa.grams, ba.grams
                 FROM recipes r
                 JOIN ingredients i ON i.id = r.as_ingredient_id
                 LEFT JOIN users u ON u.id = i.user_id
                 LEFT JOIN amounts sa ON sa.id = i.serving_size_id
                 LEFT JOIN amounts ba ON ba.id = i.batch_size_id
                 WHERE r.id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<f64>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<f64>>(7)?,
                        row.get::<_, Option<f64>>(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((as_ing_id, name, subtitle, directions, creator, s_amount, s_unit, s_grams, batch_grams)) = meta
        else {
            return Ok(None);
        };

        let mut istmt = conn.prepare(
            "SELECT i.id, i.name, a.amount, a.unit, a.grams
             FROM ingredient_in_recipe iir
             JOIN ingredients i ON i.id = iir.ingredient_id
             JOIN amounts a ON a.id = iir.amount_id
             WHERE iir.recipe_id = ?1 ORDER BY iir.\"order\"",
        )?;
        let items = istmt
            .query_map([id], |row| {
                Ok(RecipeItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    amount: Amount {
                        amount: row.get(2)?,
                        unit: row.get(3)?,
                        grams: row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let nutrition = aggregate_per100g(&conn, as_ing_id)?;
        let serving = s_grams.map(|grams| Amount { amount: s_amount, unit: s_unit, grams });

        Ok(Some(RecipeView {
            id,
            name,
            subtitle,
            directions,
            creator,
            serving,
            batch_grams,
            items,
            nutrition,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Proves the on-device DAL: open the seeded SQLite, run the recursive CTE, and confirm the
    // 20-ingredient Complete Shake (recipe 17) aggregates to ~307.5 cal/serving — no server.
    #[test]
    fn recipe_17_nutrition_on_device() {
        let db = Db::open(&crate::db_path()).expect("open db");
        let r = db.recipe(17).expect("query ok").expect("recipe 17 exists");
        let cal100 = r.nutrition.calories_per_100g.expect("has calories");
        let grams = r.serving.as_ref().expect("has serving").grams;
        let per_serving = cal100 * grams / 100.0;
        eprintln!(
            "recipe 17 = {:?}: {:.1} cal/serving, {} nutrients",
            r.name,
            per_serving,
            r.nutrition.readings.len()
        );
        assert!(
            (per_serving - 307.5).abs() < 0.5,
            "expected ~307.5 cal/serving, got {per_serving:.2}"
        );
    }
}
