// The fastest vegify. Rust + Axum + rusqlite (direct SQLite, no client/HTTP hop).
// Recipe nutrition — the compute-heavy part — is one recursive CTE that walks the
// recipe→ingredient graph (recipes nest: a recipe IS an ingredient) and aggregates every
// nutrient in the DB in a single round-trip. That's the TechEmpower lesson applied: collapse
// the N+1 "Multiple Queries" pattern into a "Single Query"/"Fortunes"-class read, served by
// the stack that tops those tests. Reads the same .data/vegify.db as the other two shells.
use std::fmt::Write as _;

use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::get,
    Router,
};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

thread_local! {
    static CONN: Connection = open_db();
}

fn open_db() -> Connection {
    let path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "../../.data/vegify.db".into());
    let c = Connection::open(&path).expect("open db");
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")
        .ok();
    c
}

// One recursive CTE: effective grams of every leaf ingredient within the recipe's batch.
// eff_grams(top) = batch grams; eff_grams(child) = eff_grams(parent) * child_grams / parent_total.
// A leaf's nutrient contribution = per_100g * eff_grams / 100. Bind param ?1 = recipe id.
const CTE: &str = "
WITH RECURSIVE
recipe_total AS (
  SELECT r.id AS recipe_id, r.as_ingredient_id, SUM(a.grams) AS total_grams
  FROM recipes r
  JOIN ingredient_in_recipe iir ON iir.recipe_id = r.id
  JOIN amounts a ON a.id = iir.amount_id
  GROUP BY r.id
),
expand(ingredient_id, eff_grams) AS (
  SELECT rt.as_ingredient_id, rt.total_grams FROM recipe_total rt WHERE rt.recipe_id = ?1
  UNION ALL
  SELECT iir.ingredient_id, e.eff_grams * a.grams / rt.total_grams
  FROM expand e
  JOIN recipes r ON r.as_ingredient_id = e.ingredient_id
  JOIN recipe_total rt ON rt.recipe_id = r.id
  JOIN ingredient_in_recipe iir ON iir.recipe_id = r.id
  JOIN amounts a ON a.id = iir.amount_id
)";

struct Nutrient {
    name: String,
    total: f64, // whole-batch total
    unit: String,
}

struct RecipeData {
    name: String,
    subtitle: Option<String>,
    directions: Option<String>,
    serving_grams: f64,
    batch_grams: f64,
    calories_total: f64,
    nutrients: Vec<Nutrient>,
    ingredients: Vec<(f64, String, String)>, // amount, unit, name
}

fn query_recipe(id: i64) -> rusqlite::Result<Option<RecipeData>> {
    CONN.with(|c| {
        let meta = c
            .query_row(
                "SELECT i.name, r.subtitle, r.directions, COALESCE(sa.grams, 0)
                 FROM recipes r
                 JOIN ingredients i ON i.id = r.as_ingredient_id
                 LEFT JOIN amounts sa ON sa.id = i.serving_size_id
                 WHERE r.id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, f64>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((name, subtitle, directions, serving_grams)) = meta else {
            return Ok(None);
        };

        let batch_grams: f64 = c.query_row(
            "SELECT COALESCE(SUM(a.grams), 0) FROM ingredient_in_recipe iir
             JOIN amounts a ON a.id = iir.amount_id WHERE iir.recipe_id = ?1",
            [id],
            |r| r.get(0),
        )?;

        let mut istmt = c.prepare(
            "SELECT a.amount, a.unit, i.name FROM ingredient_in_recipe iir
             JOIN ingredients i ON i.id = iir.ingredient_id
             JOIN amounts a ON a.id = iir.amount_id
             WHERE iir.recipe_id = ?1 ORDER BY iir.\"order\"",
        )?;
        let ingredients = istmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, Option<f64>>(0)?.unwrap_or(0.0),
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let calories_total: f64 = c.query_row(
            &format!(
                "{CTE} SELECT COALESCE(SUM(i.calories_per_100g * e.eff_grams / 100.0), 0)
                 FROM expand e JOIN ingredients i ON i.id = e.ingredient_id
                 WHERE i.calories_per_100g IS NOT NULL"
            ),
            [id],
            |r| r.get(0),
        )?;

        let mut nstmt = c.prepare(&format!(
            "{CTE} SELECT n.name, SUM(inu.amount_per_100g * e.eff_grams / 100.0), inu.unit
             FROM expand e
             JOIN ingredient_nutrient inu ON inu.ingredient_id = e.ingredient_id
             JOIN nutrients n ON n.id = inu.nutrient_id
             GROUP BY n.id ORDER BY n.name"
        ))?;
        let nutrients = nstmt
            .query_map([id], |r| {
                Ok(Nutrient {
                    name: r.get(0)?,
                    total: r.get(1)?,
                    unit: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Some(RecipeData {
            name,
            subtitle,
            directions,
            serving_grams,
            batch_grams,
            calories_total,
            nutrients,
            ingredients,
        }))
    })
}

// FDA Daily Values (subset). None unit handled via to_ug.
fn daily_value(name: &str) -> Option<(f64, &'static str)> {
    match name.trim().to_lowercase().as_str() {
        "total fat" | "fat" => Some((78.0, "g")),
        "total carbohydrates" | "carbohydrate" | "carbohydrates" => Some((275.0, "g")),
        "total protein" | "protein" => Some((50.0, "g")),
        "calcium" => Some((1300.0, "mg")),
        "iron" => Some((18.0, "mg")),
        "potassium" => Some((4700.0, "mg")),
        "sodium" => Some((2300.0, "mg")),
        "magnesium" => Some((420.0, "mg")),
        "zinc" => Some((11.0, "mg")),
        "vitamin a" => Some((900.0, "µg")),
        "vitamin b6" => Some((1.7, "mg")),
        "vitamin b12" => Some((2.4, "µg")),
        "vitamin c" => Some((90.0, "mg")),
        "vitamin d" => Some((20.0, "µg")),
        "vitamin e" => Some((15.0, "mg")),
        "vitamin k" => Some((120.0, "µg")),
        _ => None,
    }
}

fn to_ug(amt: f64, unit: &str) -> f64 {
    match unit.to_lowercase().as_str() {
        "g" => amt * 1e6,
        "mg" => amt * 1e3,
        "µg" | "mcg" | "ug" => amt,
        _ => f64::NAN,
    }
}

fn pct_dv(name: &str, per_serving: f64, unit: &str) -> Option<i64> {
    let (dv, dv_unit) = daily_value(name)?;
    let base = to_ug(per_serving, unit);
    let dv_base = to_ug(dv, dv_unit);
    if base.is_nan() || dv_base == 0.0 {
        None
    } else {
        Some((base / dv_base * 100.0).round() as i64)
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn fmt_num(n: f64) -> String {
    let r = (n * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r}") }
}

fn render(r: &RecipeData) -> String {
    let scale = if r.batch_grams > 0.0 { r.serving_grams / r.batch_grams } else { 0.0 };
    let servings = if r.serving_grams > 0.0 { r.batch_grams / r.serving_grams } else { 0.0 };
    let mut rows = String::new();
    for n in &r.nutrients {
        let per_serving = n.total * scale;
        let pct = pct_dv(&n.name, per_serving, &n.unit)
            .map(|p| format!("{p}%"))
            .unwrap_or_else(|| "—".into());
        let _ = write!(
            rows,
            "<tr><td>{}</td><td class=r>{}{}</td><td class=r>{}</td></tr>",
            esc(&n.name), fmt_num(per_serving), esc(&n.unit), pct
        );
    }
    let mut items = String::new();
    for (amt, unit, name) in &r.ingredients {
        let _ = write!(items, "<li>{} {} {}</li>", fmt_num(*amt), esc(unit), esc(name));
    }
    format!(
        r#"<!doctype html><html lang=en><head><meta charset=utf-8>
<meta name=viewport content="width=device-width,initial-scale=1">
<title>{name} · vegify (fastest)</title>
<style>
:root{{font-family:'Avenir Next',system-ui,sans-serif;color:#2B2B2B}}
body{{margin:0}} header{{background:#328432;color:#F1F1F1;padding:14px 24px;font-weight:700}}
main{{max-width:900px;margin:0 auto;padding:24px;display:grid;grid-template-columns:1fr 320px;gap:32px}}
h1{{color:#328432;text-align:center}} h2{{text-align:center}}
ul{{columns:2;list-style:disc;padding-left:20px}}
.panel{{border:1px solid #eee;border-radius:12px;padding:16px}}
table{{width:100%;border-collapse:collapse;font-size:14px}} td{{border-bottom:1px solid #f1f1f1;padding:2px 0}}
.r{{text-align:right}} .cal{{font-size:28px;font-weight:800;display:flex;justify-content:space-between;border-bottom:4px solid #2B2B2B}}
@media(max-width:700px){{main{{grid-template-columns:1fr}}}}
</style></head><body>
<header>🌱 vegify · fastest (Rust + Axum, nutrition via 1 recursive CTE)</header>
<main>
<section>
<h1>{name}</h1>{subtitle}
<h2>Ingredients</h2><ul>{items}</ul>
<h2>Directions</h2><p>{directions}</p>
</section>
<aside class=panel>
<h2 style="text-align:left;border-bottom:4px solid #2B2B2B">Nutrition Facts</h2>
<p>This Recipe<br>{servings} servings per batch<br>Serving size {serving} g</p>
<div class=cal><span>Calories</span><span>{calories}</span></div>
<table>{rows}</table>
<p style="font-size:10px;color:#8A8B8A">* Percent Daily Values based on a 2,000 calorie diet.</p>
</aside>
</main></body></html>"#,
        name = esc(&r.name),
        subtitle = r.subtitle.as_deref().map(|s| format!("<p style=text-align:center;color:#8A8B8A>{}</p>", esc(s))).unwrap_or_default(),
        directions = esc(r.directions.as_deref().unwrap_or("No directions yet.")),
        items = items,
        rows = rows,
        servings = fmt_num(servings),
        serving = fmt_num(r.serving_grams),
        calories = fmt_num(r.calories_total * scale),
    )
}

#[derive(Serialize)]
struct NutrientOut {
    name: String,
    per_serving: f64,
    unit: String,
    percent_dv: Option<i64>,
}
#[derive(Serialize)]
struct NutritionOut {
    recipe: String,
    servings_per_batch: f64,
    serving_grams: f64,
    calories_per_serving: f64,
    nutrients: Vec<NutrientOut>,
}

fn to_out(r: &RecipeData) -> NutritionOut {
    let scale = if r.batch_grams > 0.0 { r.serving_grams / r.batch_grams } else { 0.0 };
    NutritionOut {
        recipe: r.name.clone(),
        servings_per_batch: if r.serving_grams > 0.0 { r.batch_grams / r.serving_grams } else { 0.0 },
        serving_grams: r.serving_grams,
        calories_per_serving: (r.calories_total * scale * 10.0).round() / 10.0,
        nutrients: r
            .nutrients
            .iter()
            .map(|n| {
                let ps = n.total * scale;
                NutrientOut {
                    name: n.name.clone(),
                    per_serving: (ps * 1000.0).round() / 1000.0,
                    unit: n.unit.clone(),
                    percent_dv: pct_dv(&n.name, ps, &n.unit),
                }
            })
            .collect(),
    }
}

async fn recipe_html(Path(id): Path<i64>) -> Response {
    match tokio::task::spawn_blocking(move || query_recipe(id)).await {
        Ok(Ok(Some(data))) => Html(render(&data)).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "recipe not found").into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "error").into_response(),
    }
}

async fn recipe_json(Path(id): Path<i64>) -> Response {
    match tokio::task::spawn_blocking(move || query_recipe(id)).await {
        Ok(Ok(Some(data))) => Json(to_out(&data)).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "recipe not found").into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "error").into_response(),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/recipes/{id}", get(recipe_html))
        .route("/recipes/{id}/nutrition.json", get(recipe_json))
        .route("/healthz", get(|| async { "ok" }));

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(39050);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.unwrap();
    println!("web-fast listening on :{port}");
    axum::serve(listener, app).await.unwrap();
}
