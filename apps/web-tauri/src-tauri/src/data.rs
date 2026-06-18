//! On-device DAL + scale-to-zero changeset sync for the Tauri desktop shell.
//!
//! The `#[procedures]` trait is the typed contract (same surface as @vegify/db on the web).
//! Backed by rusqlite (bundled SQLite, sync); nutrition is ONE recursive CTE ported from
//! packages/db/src/nutrition.ts (per-100g). WRITES are captured as SQLite **session changesets**
//! and persisted to a blob store as immutable objects — the scale-to-zero sync unit. `sync()`
//! pulls unseen blobs and applies them (LWW). A local dir stands in for S3 here; swapping it for
//! an S3 bucket (+ optional scale-to-zero Lambda) is a transport change, not a logic change. No
//! always-on server.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::session::{ConflictAction, Session};
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
        // Local bookkeeping: which changeset blobs we've already applied (incl. our own writes).
        conn.execute_batch("CREATE TABLE IF NOT EXISTS _applied_changesets(id TEXT PRIMARY KEY);")?;
        let blob_dir = PathBuf::from(blob_dir);
        fs::create_dir_all(&blob_dir).map_err(io_err)?;
        Ok(Self { conn: Mutex::new(conn), blob_dir })
    }

    /// Run a mutation while recording a session changeset, then persist that changeset as an
    /// immutable blob (the S3-transportable unit) and mark it already-applied locally.
    fn write_capture(
        &self,
        write: impl FnOnce(&Connection) -> Result<(), DataError>,
    ) -> Result<(), DataError> {
        let conn = self.conn.lock().unwrap();
        let mut bytes = Vec::new();
        {
            let mut session = Session::new(&conn)?;
            session.attach(None)?; // track all tables
            write(&conn)?;
            session.changeset_strm(&mut bytes)?;
        }
        if !bytes.is_empty() {
            let id = ulid::Ulid::new().to_string(); // sortable, offline-generatable
            fs::write(self.blob_dir.join(format!("{id}.cs")), &bytes).map_err(io_err)?;
            conn.execute("INSERT OR IGNORE INTO _applied_changesets(id) VALUES (?1)", [&id])?;
        }
        Ok(())
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
    fn rename_recipe(&self, id: i32, name: String) -> Result<(), DataError>;
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

    /// A WRITE — captured as a changeset blob (the sync unit) via write_capture.
    fn rename_recipe(&self, id: i32, name: String) -> Result<(), DataError> {
        self.write_capture(|conn| {
            conn.execute(
                "UPDATE ingredients SET name = ?2
                 WHERE id = (SELECT as_ingredient_id FROM recipes WHERE id = ?1)",
                rusqlite::params![id, name],
            )?;
            Ok(())
        })
    }

    /// Pull-and-apply: scan the blob store, apply any changeset not yet seen (LWW), record it.
    /// No server — the blob store is S3 (a local dir here). This is the whole sync, scale-to-zero.
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

    // Scale-to-zero sync, end-to-end in the DAL: device A's offline write flows to device B via a
    // changeset blob in the shared store — no server. (Local dir here = S3 in production.)
    #[test]
    fn changeset_syncs_write_between_devices() {
        let tmp = std::env::temp_dir();
        let a_db = tmp.join("dev-password-A.db");
        let b_db = tmp.join("dev-password-B.db");
        let blobs = tmp.join("vegify-sync-blobs");
        let _ = fs::remove_file(&a_db);
        let _ = fs::remove_file(&b_db);
        let _ = fs::remove_dir_all(&blobs);
        // Both devices start from the same seed (same row ids → changesets apply cleanly).
        fs::copy(crate::db_path(), &a_db).expect("seed A");
        fs::copy(crate::db_path(), &b_db).expect("seed B");

        let a = Db::open(a_db.to_str().unwrap(), blobs.to_str().unwrap()).expect("open A");
        let b = Db::open(b_db.to_str().unwrap(), blobs.to_str().unwrap()).expect("open B");

        // Device A makes an offline write (captured to the blob store).
        VegifyData::rename_recipe(&a, 17, "Synced Shake".into()).expect("A writes");
        // Device B pulls + applies from the shared blob store — no server in between.
        VegifyData::sync(&b).expect("B syncs");

        let r = VegifyData::recipe(&b, 17).expect("query").expect("recipe 17 on B");
        eprintln!("device B recipe 17 name after sync = {:?}", r.name);
        assert_eq!(r.name, "Synced Shake", "B should see A's write after applying the changeset");
    }
}
