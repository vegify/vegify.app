//! On-device DAL + scale-to-zero changeset sync for the Tauri desktop shell.
//!
//! The `#[procedures]` trait is the typed contract (same surface as @vegify/db on the web). Backed
//! by rusqlite (bundled SQLite, sync); nutrition is ONE recursive CTE ported from
//! packages/db/src/nutrition.ts. The mutations are ported from packages/db/src/mutations.ts
//! (including the amounts cascade-cleanup fixes). Every write runs inside `write_capture`, which
//! records a SQLite `session` changeset and persists it as an immutable blob — the scale-to-zero
//! sync unit. `sync()` pulls unseen blobs and applies them (LWW). Local dir = S3 stand-in. No server.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::session::{ConflictAction, Session};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use ttipc::procedures;

// ---- read/output wire types ----

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

// ---- write/input wire types (mirror @vegify/db mutation inputs) ----

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IngredientNutrientInput {
    pub name: String,
    pub amount_per_100g: f64,
    pub unit: String,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveIngredientInput {
    pub id: Option<i32>,
    pub name: String,
    pub description: Option<String>,
    pub price: Option<i32>, // cents
    pub calories_per_100g: Option<f64>,
    pub serving_grams: Option<f64>,
    pub package_grams: Option<f64>,
    pub nutrients: Vec<IngredientNutrientInput>,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeItemInput {
    pub ingredient_id: i32,
    pub grams: f64,
    pub unit: Option<String>,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveRecipeInput {
    pub id: Option<i32>,
    pub name: String,
    pub subtitle: Option<String>,
    pub directions: Option<String>,
    pub serving_grams: Option<f64>,
    pub batch_grams: Option<f64>,
    pub items: Vec<RecipeItemInput>,
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

fn io_err(e: std::io::Error) -> DataError {
    DataError::Db(e.to_string())
}

/// Same recursive CTE as packages/db/src/nutrition.ts, normalized to per-100g.
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

// ---- mutation helpers (ported from mutations.ts) ----

fn upsert_amount(
    conn: &Connection,
    id: Option<i64>,
    grams: Option<f64>,
    unit: &str,
) -> Result<Option<i64>, DataError> {
    let Some(grams) = grams else { return Ok(id) };
    if let Some(id) = id {
        conn.execute("UPDATE amounts SET grams = ?2, unit = ?3 WHERE id = ?1", params![id, grams, unit])?;
        Ok(Some(id))
    } else {
        let new_id: i64 = conn.query_row(
            "INSERT INTO amounts(grams, unit, amount, preferred) VALUES (?1, ?2, 1, 'grams') RETURNING id",
            params![grams, unit],
            |r| r.get(0),
        )?;
        Ok(Some(new_id))
    }
}

// Owner→amount is the wrong cascade direction, so orphaned amounts must be deleted by id.
fn delete_amounts(conn: &Connection, ids: &[Option<i64>]) -> Result<(), DataError> {
    for id in ids.iter().flatten() {
        conn.execute("DELETE FROM amounts WHERE id = ?1", [id])?;
    }
    Ok(())
}

fn find_or_create_nutrient(conn: &Connection, name: &str) -> Result<i64, DataError> {
    if let Some(id) =
        conn.query_row("SELECT id FROM nutrients WHERE name = ?1", [name], |r| r.get::<_, i64>(0)).optional()?
    {
        return Ok(id);
    }
    Ok(conn.query_row("INSERT INTO nutrients(name) VALUES (?1) RETURNING id", [name], |r| r.get(0))?)
}

fn do_save_ingredient(conn: &Connection, input: &SaveIngredientInput) -> Result<i64, DataError> {
    let ingredient_id: i64 = if let Some(id) = input.id {
        let id = id as i64;
        let (serving, batch): (Option<i64>, Option<i64>) = conn
            .query_row("SELECT serving_size_id, batch_size_id FROM ingredients WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?
            .unwrap_or((None, None));
        let serving_size_id = upsert_amount(conn, serving, input.serving_grams, "serving")?;
        let batch_size_id = upsert_amount(conn, batch, input.package_grams, "package")?;
        conn.execute(
            "UPDATE ingredients SET name=?2, description=?3, price=?4, calories_per_100g=?5,
             serving_size_id=?6, batch_size_id=?7 WHERE id=?1",
            params![
                id,
                input.name,
                input.description.as_deref(),
                input.price,
                input.calories_per_100g,
                serving_size_id,
                batch_size_id
            ],
        )?;
        conn.execute("DELETE FROM ingredient_nutrient WHERE ingredient_id = ?1", [id])?;
        id
    } else {
        let serving_size_id = upsert_amount(conn, None, input.serving_grams, "serving")?;
        let batch_size_id = upsert_amount(conn, None, input.package_grams, "package")?;
        conn.query_row(
            "INSERT INTO ingredients(user_id, name, description, is_vegan, price, calories_per_100g,
             serving_size_id, batch_size_id) VALUES (NULL, ?1, ?2, 1, ?3, ?4, ?5, ?6) RETURNING id",
            params![
                input.name,
                input.description.as_deref(),
                input.price,
                input.calories_per_100g,
                serving_size_id,
                batch_size_id
            ],
            |r| r.get(0),
        )?
    };

    let mut seen = HashSet::new();
    for n in &input.nutrients {
        let name = n.name.trim();
        if name.is_empty() {
            continue;
        }
        let nutrient_id = find_or_create_nutrient(conn, name)?;
        if !seen.insert(nutrient_id) {
            continue;
        }
        let unit = if n.unit.is_empty() { "g" } else { n.unit.as_str() };
        conn.execute(
            "INSERT INTO ingredient_nutrient(ingredient_id, nutrient_id, amount_per_100g, unit)
             VALUES (?1, ?2, ?3, ?4)",
            params![ingredient_id, nutrient_id, n.amount_per_100g, unit],
        )?;
    }
    Ok(ingredient_id)
}

fn do_delete_ingredient(conn: &Connection, id: i64) -> Result<(), DataError> {
    let existing: Option<(Option<i64>, Option<i64>)> = conn
        .query_row("SELECT serving_size_id, batch_size_id FROM ingredients WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let Some((serving, batch)) = existing else { return Ok(()) };
    conn.execute("DELETE FROM ingredients WHERE id = ?1", [id])?;
    delete_amounts(conn, &[serving, batch])
}

fn do_save_recipe(conn: &Connection, input: &SaveRecipeInput) -> Result<i64, DataError> {
    let recipe_id: i64 = if let Some(id) = input.id {
        let id = id as i64;
        let (as_ing_id, serving, batch): (i64, Option<i64>, Option<i64>) = conn.query_row(
            "SELECT r.as_ingredient_id, i.serving_size_id, i.batch_size_id
             FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id WHERE r.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let serving_size_id = upsert_amount(conn, serving, input.serving_grams, "serving")?;
        let batch_size_id = upsert_amount(conn, batch, input.batch_grams, "batch")?;
        conn.execute(
            "UPDATE ingredients SET name=?2, serving_size_id=?3, batch_size_id=?4 WHERE id=?1",
            params![as_ing_id, input.name, serving_size_id, batch_size_id],
        )?;
        conn.execute(
            "UPDATE recipes SET subtitle=?2, directions=?3 WHERE id=?1",
            params![id, input.subtitle.as_deref(), input.directions.as_deref()],
        )?;
        // Re-attach items from scratch; delete the join rows AND their (non-cascaded) amounts.
        let prev: Vec<Option<i64>> = {
            let mut stmt = conn.prepare("SELECT amount_id FROM ingredient_in_recipe WHERE recipe_id = ?1")?;
            let v = stmt
                .query_map([id], |r| r.get::<_, Option<i64>>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        conn.execute("DELETE FROM ingredient_in_recipe WHERE recipe_id = ?1", [id])?;
        delete_amounts(conn, &prev)?;
        id
    } else {
        let serving_size_id = upsert_amount(conn, None, input.serving_grams, "serving")?;
        let batch_size_id = upsert_amount(conn, None, input.batch_grams, "batch")?;
        let as_ing_id: i64 = conn.query_row(
            "INSERT INTO ingredients(user_id, name, is_vegan, serving_size_id, batch_size_id)
             VALUES (NULL, ?1, 1, ?2, ?3) RETURNING id",
            params![input.name, serving_size_id, batch_size_id],
            |r| r.get(0),
        )?;
        conn.query_row(
            "INSERT INTO recipes(as_ingredient_id, subtitle, directions) VALUES (?1, ?2, ?3) RETURNING id",
            params![as_ing_id, input.subtitle.as_deref(), input.directions.as_deref()],
            |r| r.get(0),
        )?
    };

    let mut order = 0i64;
    for item in &input.items {
        if item.ingredient_id == 0 || item.grams == 0.0 {
            continue;
        }
        let unit = item.unit.as_deref().unwrap_or("g");
        let amount_id: i64 = conn.query_row(
            "INSERT INTO amounts(grams, amount, unit, preferred) VALUES (?1, ?1, ?2, 'grams') RETURNING id",
            params![item.grams, unit],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO ingredient_in_recipe(\"order\", recipe_id, ingredient_id, amount_id)
             VALUES (?1, ?2, ?3, ?4)",
            params![order, recipe_id, item.ingredient_id as i64, amount_id],
        )?;
        order += 1;
    }
    Ok(recipe_id)
}

fn do_delete_recipe(conn: &Connection, id: i64) -> Result<(), DataError> {
    let as_ing: Option<(i64, Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT r.as_ingredient_id, i.serving_size_id, i.batch_size_id
             FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id WHERE r.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((as_ing_id, serving, batch)) = as_ing else { return Ok(()) };

    let item_amounts: Vec<Option<i64>> = {
        let mut stmt = conn.prepare("SELECT amount_id FROM ingredient_in_recipe WHERE recipe_id = ?1")?;
        let v = stmt
            .query_map([id], |r| r.get::<_, Option<i64>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    conn.execute("DELETE FROM recipes WHERE id = ?1", [id])?; // cascades ingredient_in_recipe
    delete_amounts(conn, &item_amounts)?;

    // Delete the as-ingredient only if no other recipe still consumes it as an item.
    let still_used: Option<i64> = conn
        .query_row("SELECT id FROM ingredient_in_recipe WHERE ingredient_id = ?1 LIMIT 1", [as_ing_id], |r| {
            r.get(0)
        })
        .optional()?;
    if still_used.is_none() {
        conn.execute("DELETE FROM ingredients WHERE id = ?1", [as_ing_id])?; // cascades ingredient_nutrient
        delete_amounts(conn, &[serving, batch])?;
    }
    Ok(())
}

pub struct Db {
    conn: Mutex<Connection>,
    blob_dir: PathBuf,
}

impl Db {
    /// Open the local DB and the changeset blob store (a local dir standing in for S3).
    pub fn open(db_path: &str, blob_dir: &str) -> Result<Self, DataError> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")
            .ok();
        conn.execute_batch("CREATE TABLE IF NOT EXISTS _applied_changesets(id TEXT PRIMARY KEY);")?;
        let blob_dir = PathBuf::from(blob_dir);
        fs::create_dir_all(&blob_dir).map_err(io_err)?;
        Ok(Self { conn: Mutex::new(conn), blob_dir })
    }

    /// Run a mutation while recording a session changeset, persist it as an immutable blob (the
    /// S3-transportable sync unit), mark it applied-locally, and return the mutation's value.
    fn write_capture<T>(
        &self,
        write: impl FnOnce(&Connection) -> Result<T, DataError>,
    ) -> Result<T, DataError> {
        let conn = self.conn.lock().unwrap();
        let mut bytes = Vec::new();
        let value = {
            let mut session = Session::new(&conn)?;
            session.attach(None)?;
            let v = write(&conn)?;
            session.changeset_strm(&mut bytes)?;
            v
        };
        if !bytes.is_empty() {
            let id = ulid::Ulid::new().to_string();
            fs::write(self.blob_dir.join(format!("{id}.cs")), &bytes).map_err(io_err)?;
            conn.execute("INSERT OR IGNORE INTO _applied_changesets(id) VALUES (?1)", [&id])?;
        }
        Ok(value)
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
    fn save_ingredient(&self, input: SaveIngredientInput) -> Result<i32, DataError>;
    fn delete_ingredient(&self, id: i32) -> Result<(), DataError>;
    fn save_recipe(&self, input: SaveRecipeInput) -> Result<i32, DataError>;
    fn delete_recipe(&self, id: i32) -> Result<(), DataError>;
    fn sync(&self) -> Result<(), DataError>;
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

    fn save_ingredient(&self, input: SaveIngredientInput) -> Result<i32, DataError> {
        self.write_capture(|conn| do_save_ingredient(conn, &input).map(|id| id as i32))
    }

    fn delete_ingredient(&self, id: i32) -> Result<(), DataError> {
        self.write_capture(|conn| do_delete_ingredient(conn, id as i64))
    }

    fn save_recipe(&self, input: SaveRecipeInput) -> Result<i32, DataError> {
        self.write_capture(|conn| do_save_recipe(conn, &input).map(|id| id as i32))
    }

    fn delete_recipe(&self, id: i32) -> Result<(), DataError> {
        self.write_capture(|conn| do_delete_recipe(conn, id as i64))
    }

    /// Pull-and-apply: scan the blob store, apply any changeset not yet seen (LWW), record it.
    fn sync(&self) -> Result<(), DataError> {
        let conn = self.conn.lock().unwrap();
        for entry in fs::read_dir(&self.blob_dir).map_err(io_err)? {
            let path = entry.map_err(io_err)?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("cs") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let seen: Option<String> = conn
                .query_row("SELECT id FROM _applied_changesets WHERE id = ?1", [id], |r| r.get(0))
                .optional()?;
            if seen.is_some() {
                continue;
            }
            let bytes = fs::read(&path).map_err(io_err)?;
            conn.apply_strm(
                &mut &bytes[..],
                None::<fn(&str) -> bool>,
                |_conflict, _item| ConflictAction::SQLITE_CHANGESET_REPLACE,
            )?;
            conn.execute("INSERT OR IGNORE INTO _applied_changesets(id) VALUES (?1)", [id])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_17_nutrition_on_device() {
        let blobs = std::env::temp_dir().join("vegify-cte-blobs");
        let db = Db::open(&crate::db_path(), blobs.to_str().unwrap()).expect("open");
        let r = VegifyData::recipe(&db, 17).expect("query ok").expect("recipe 17 exists");
        let cal100 = r.nutrition.calories_per_100g.expect("has calories");
        let grams = r.serving.as_ref().expect("has serving").grams;
        let per_serving = cal100 * grams / 100.0;
        eprintln!("recipe 17 = {:?}: {:.1} cal/serving", r.name, per_serving);
        assert!((per_serving - 307.5).abs() < 0.5, "got {per_serving:.2}");
    }

    // Full write surface flows through sync: device A SAVES a new recipe → changeset blob →
    // device B sync() → B sees the new recipe. No server.
    #[test]
    fn save_recipe_syncs_between_devices() {
        let tmp = std::env::temp_dir();
        let a_db = tmp.join("vegify-write-A.db");
        let b_db = tmp.join("vegify-write-B.db");
        let blobs = tmp.join("vegify-write-blobs");
        let _ = fs::remove_file(&a_db);
        let _ = fs::remove_file(&b_db);
        let _ = fs::remove_dir_all(&blobs);
        fs::copy(crate::db_path(), &a_db).expect("seed A");
        fs::copy(crate::db_path(), &b_db).expect("seed B");

        let a = Db::open(a_db.to_str().unwrap(), blobs.to_str().unwrap()).expect("open A");
        let b = Db::open(b_db.to_str().unwrap(), blobs.to_str().unwrap()).expect("open B");

        let input = SaveRecipeInput {
            id: None,
            name: "Synced New Recipe".into(),
            subtitle: Some("made offline on A".into()),
            directions: None,
            serving_grams: Some(100.0),
            batch_grams: Some(300.0),
            items: vec![],
        };
        let new_id = VegifyData::save_recipe(&a, input).expect("A saves recipe");
        assert!(new_id > 0);

        VegifyData::sync(&b).expect("B syncs");
        let names: Vec<String> = VegifyData::list_recipes(&b)
            .expect("B lists")
            .into_iter()
            .map(|c| c.name)
            .collect();
        eprintln!("device B recipes after sync: {names:?}");
        assert!(names.contains(&"Synced New Recipe".to_string()), "B should see A's new recipe");
    }
}
