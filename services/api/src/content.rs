//! The /api/content/pull payload assembly — the Rust port of content.ts's `pullContent`. Returns the
//! viewer's listed world (public + own) in @vegify/db MUTATION shape (ids + each row's owner + items /
//! nutrients), which the desktop applies via vegify-core's do_save_*. Leaf ingredients only — recipe
//! as-ingredients are excluded (the recipe apply recreates them). The single-row reads + the lists go
//! through vegify-core directly; only this bulk mutation-shape assembly is server-specific.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::AppError;

#[derive(Serialize)]
pub struct PullPayload {
    pub recipes: Vec<PullRecipe>,
    pub ingredients: Vec<PullIngredient>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRecipe {
    pub id: String,
    pub as_ingredient_id: String,
    pub user_id: Option<String>,
    pub visibility: String,
    pub name: String,
    pub subtitle: Option<String>,
    pub directions: Option<String>,
    pub serving_grams: Option<f64>,
    pub batch_grams: Option<f64>,
    pub slug: Option<String>,
    pub items: Vec<PullItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullItem {
    pub ingredient_id: String,
    pub grams: f64,
    pub unit: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullIngredient {
    pub id: String,
    pub user_id: Option<String>,
    pub visibility: String,
    pub name: String,
    pub description: Option<String>,
    pub price: Option<i64>,
    pub calories_per_100g: Option<f64>,
    pub serving_grams: Option<f64>,
    pub package_grams: Option<f64>,
    pub slug: Option<String>,
    /// Soft-delete tombstone (ms). Tombstoned rows STAY in the pull — recipes that use them need
    /// the data — and clients mirror the flag so their local list/search filtering matches.
    pub deleted_at: Option<i64>,
    pub nutrients: Vec<PullReading>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullReading {
    pub name: String,
    pub amount_per_100g: f64,
    pub unit: String,
}

/// Assemble the viewer's listed world in mutation shape. isListed = public + own (the same gate the
/// vegify-core lists use); each row carries its REAL owner so the desktop's apply stamps it correctly.
pub fn pull(conn: &Connection, viewer: Option<&str>) -> Result<PullPayload, AppError> {
    // Recipes (isListed), in mutation shape.
    let mut recipes: Vec<PullRecipe> = {
        let mut stmt = conn.prepare(
            "SELECT r.id, r.as_ingredient_id, i.user_id, i.visibility, i.name, r.subtitle, r.directions,
                    sa.grams, ba.grams, i.slug
             FROM recipes r
             JOIN ingredients i ON i.id = r.as_ingredient_id
             LEFT JOIN amounts sa ON sa.id = i.serving_size_id
             LEFT JOIN amounts ba ON ba.id = i.batch_size_id
             WHERE i.visibility = 'public' OR i.user_id = ?1
             ORDER BY i.name",
        )?;
        let rows = stmt.query_map(params![viewer], |row| {
            Ok(PullRecipe {
                id: row.get(0)?,
                as_ingredient_id: row.get(1)?,
                user_id: row.get(2)?,
                visibility: row.get(3)?,
                name: row.get(4)?,
                subtitle: row.get(5)?,
                directions: row.get(6)?,
                serving_grams: row.get(7)?,
                batch_grams: row.get(8)?,
                slug: row.get(9)?,
                items: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    // Each recipe's items, in declaration order (FK references, not joined rows).
    {
        let mut istmt = conn.prepare(
            "SELECT iir.ingredient_id, a.grams, a.unit
             FROM ingredient_in_recipe iir
             JOIN amounts a ON a.id = iir.amount_id
             WHERE iir.recipe_id = ?1 ORDER BY iir.\"order\"",
        )?;
        for r in &mut recipes {
            r.items = istmt
                .query_map([&r.id], |row| {
                    Ok(PullItem {
                        ingredient_id: row.get(0)?,
                        grams: row.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                        unit: row.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
    }
    // Leaf ingredients (not a recipe's as-ingredient), isListed, in mutation shape.
    let mut ingredients: Vec<PullIngredient> = {
        let mut stmt = conn.prepare(
            "SELECT i.id, i.user_id, i.visibility, i.name, i.description, i.price, i.calories_per_100g,
                    sa.grams, ba.grams, i.slug, i.deleted_at
             FROM ingredients i
             LEFT JOIN amounts sa ON sa.id = i.serving_size_id
             LEFT JOIN amounts ba ON ba.id = i.batch_size_id
             WHERE i.id NOT IN (SELECT as_ingredient_id FROM recipes)
               AND (i.visibility = 'public' OR i.user_id = ?1)
             ORDER BY i.name",
        )?;
        let rows = stmt.query_map(params![viewer], |row| {
            Ok(PullIngredient {
                id: row.get(0)?,
                user_id: row.get(1)?,
                visibility: row.get(2)?,
                name: row.get(3)?,
                description: row.get(4)?,
                price: row.get(5)?,
                calories_per_100g: row.get(6)?,
                serving_grams: row.get(7)?,
                package_grams: row.get(8)?,
                slug: row.get(9)?,
                deleted_at: row.get(10)?,
                nutrients: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    // Each leaf ingredient's stored per-100g nutrients.
    {
        let mut nstmt = conn.prepare(
            "SELECT n.name, inu.amount_per_100g, inu.unit
             FROM ingredient_nutrient inu
             JOIN nutrients n ON n.id = inu.nutrient_id
             WHERE inu.ingredient_id = ?1 ORDER BY n.name",
        )?;
        for ing in &mut ingredients {
            ing.nutrients = nstmt
                .query_map([&ing.id], |row| {
                    Ok(PullReading {
                        name: row.get(0)?,
                        amount_per_100g: row.get(1)?,
                        unit: row.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
    }
    Ok(PullPayload { recipes, ingredients })
}
