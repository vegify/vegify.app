//! On-device data-access layer (DAL) for the Tauri desktop shell — now an **embedded replica**.
//!
//! The `#[procedures]` trait below IS the typed contract (same surface as @vegify/db on the web).
//! Backed by the `libsql` crate as a **remote embedded replica**: a local SQLite file that syncs
//! from a self-hosted sqld primary. Reads (incl. the recursive nutrition CTE, ported verbatim from
//! packages/db/src/nutrition.ts) hit the local file — fast and OFFLINE. Writes are write-through to
//! the primary (Stage 1); offline writes are a deferred fork. `sync()` reconciles with the primary.

use libsql::{Builder, Connection, Database};
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

impl From<libsql::Error> for DataError {
    fn from(e: libsql::Error) -> Self {
        DataError::Db(e.to_string())
    }
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
    db: Database,
    conn: Connection,
    synced: bool,
}

impl Db {
    /// Open the on-device DB. Empty `sync_url` → a LOCAL-ONLY SQLite (no primary, $0, fully
    /// offline — the free default). A non-empty `sync_url` → a remote embedded replica syncing
    /// from that primary (the opt-in cloud-sync upgrade).
    pub async fn open(path: &str, sync_url: String, auth_token: String) -> Result<Self, DataError> {
        let synced = !sync_url.trim().is_empty();
        let db = if synced {
            Builder::new_remote_replica(path, sync_url, auth_token)
                .build()
                .await?
        } else {
            Builder::new_local(path).build().await?
        };
        let conn = db.connect()?;
        Ok(Self { db, conn, synced })
    }

    /// Reconcile the local replica with the primary — a no-op in local-only mode.
    pub async fn sync(&self) -> Result<(), DataError> {
        if self.synced {
            self.db.sync().await?;
        }
        Ok(())
    }

    async fn aggregate_per100g(&self, ingredient_id: i64) -> Result<AggregatedNutrition, DataError> {
        let mut rows = self.conn.query(CTE, libsql::params![ingredient_id]).await?;
        let mut calories_per_100g = None;
        let mut readings = Vec::new();
        while let Some(row) = rows.next().await? {
            let kind: String = row.get(0)?;
            let name: Option<String> = row.get(1)?;
            let unit: Option<String> = row.get(2)?;
            let per100g: Option<f64> = row.get(3)?;
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
}

#[procedures]
pub trait VegifyData {
    async fn list_recipes(&self) -> Result<Vec<RecipeCard>, DataError>;
    async fn recipe(&self, id: i32) -> Result<Option<RecipeView>, DataError>;
    async fn sync(&self) -> Result<(), DataError>;
}

impl VegifyData for Db {
    async fn list_recipes(&self) -> Result<Vec<RecipeCard>, DataError> {
        let mut rows = self
            .conn
            .query(
                "SELECT r.id, i.name, r.subtitle
                 FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id
                 ORDER BY i.name",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(RecipeCard {
                id: row.get::<i64>(0)? as i32,
                name: row.get(1)?,
                subtitle: row.get(2)?,
            });
        }
        Ok(out)
    }

    async fn recipe(&self, id: i32) -> Result<Option<RecipeView>, DataError> {
        let mut meta_rows = self
            .conn
            .query(
                "SELECT i.id, i.name, r.subtitle, r.directions, u.name,
                        sa.amount, sa.unit, sa.grams, ba.grams
                 FROM recipes r
                 JOIN ingredients i ON i.id = r.as_ingredient_id
                 LEFT JOIN users u ON u.id = i.user_id
                 LEFT JOIN amounts sa ON sa.id = i.serving_size_id
                 LEFT JOIN amounts ba ON ba.id = i.batch_size_id
                 WHERE r.id = ?1",
                libsql::params![id as i64],
            )
            .await?;
        let Some(meta) = meta_rows.next().await? else {
            return Ok(None);
        };
        let as_ing_id: i64 = meta.get(0)?;
        let name: String = meta.get(1)?;
        let subtitle: Option<String> = meta.get(2)?;
        let directions: Option<String> = meta.get(3)?;
        let creator: Option<String> = meta.get(4)?;
        let s_amount: Option<f64> = meta.get(5)?;
        let s_unit: Option<String> = meta.get(6)?;
        let s_grams: Option<f64> = meta.get(7)?;
        let batch_grams: Option<f64> = meta.get(8)?;

        let mut item_rows = self
            .conn
            .query(
                "SELECT i.id, i.name, a.amount, a.unit, a.grams
                 FROM ingredient_in_recipe iir
                 JOIN ingredients i ON i.id = iir.ingredient_id
                 JOIN amounts a ON a.id = iir.amount_id
                 WHERE iir.recipe_id = ?1 ORDER BY iir.\"order\"",
                libsql::params![id as i64],
            )
            .await?;
        let mut items = Vec::new();
        while let Some(row) = item_rows.next().await? {
            items.push(RecipeItem {
                id: row.get::<i64>(0)? as i32,
                name: row.get(1)?,
                amount: Amount {
                    amount: row.get(2)?,
                    unit: row.get(3)?,
                    grams: row.get::<Option<f64>>(4)?.unwrap_or(0.0),
                },
            });
        }

        let nutrition = self.aggregate_per100g(as_ing_id).await?;
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

    async fn sync(&self) -> Result<(), DataError> {
        Db::sync(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // End-to-end Stage-1 proof: open an embedded replica against the running sqld primary
    // (docker vegify-sqld on :8080), sync, then run the on-device CTE — recipe 17 must be the
    // 307.5-cal shake that was seeded into the primary. Requires the primary up + seeded.
    #[test]
    fn replica_syncs_and_computes_recipe_17() {
        tauri::async_runtime::block_on(async {
            let (path, _url, token) = crate::db_config();
            let db = Db::open(&path, "http://127.0.0.1:8080".into(), token)
                .await
                .expect("open replica");
            db.sync().await.expect("initial sync from primary");
            let r = VegifyData::recipe(&db, 17)
                .await
                .expect("query ok")
                .expect("recipe 17 synced from primary");
            let cal100 = r.nutrition.calories_per_100g.expect("has calories");
            let grams = r.serving.as_ref().expect("has serving").grams;
            let per_serving = cal100 * grams / 100.0;
            eprintln!(
                "replica recipe 17 = {:?}: {:.1} cal/serving, {} nutrients",
                r.name,
                per_serving,
                r.nutrition.readings.len()
            );
            assert!(
                (per_serving - 307.5).abs() < 0.5,
                "expected ~307.5 cal/serving from the synced replica, got {per_serving:.2}"
            );
        });
    }

    // Free default: open LOCAL-ONLY (no primary) — must succeed offline with no sync server.
    #[test]
    fn local_only_opens_without_primary() {
        tauri::async_runtime::block_on(async {
            let path = std::env::temp_dir().join("vegify-local-only-test.db");
            let _ = std::fs::remove_file(&path);
            let db = Db::open(path.to_str().unwrap(), String::new(), String::new())
                .await
                .expect("open local-only (no primary)");
            db.sync().await.expect("sync is a no-op in local-only mode");
            // Proves the no-primary open + no-op-sync path (the free/offline default). A fresh
            // local DB has no app schema yet — a real first-run bundles migrations + a seed; that
            // is out of scope here, so we don't query the app tables.
            eprintln!("local-only opened with no primary; sync no-op OK");
        });
    }
}
