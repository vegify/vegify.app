// Leptos spike — "React-like DX in Rust." The page is built from real components
// (#[component] + the view! macro: signals/JSX-ish), rendered to HTML on the server.
// Same data layer as web-fast: nutrition via one recursive CTE over the shared SQLite db.
// This is an SSR-only spike to evaluate the authoring DX; full hydration/islands would add
// the wasm client build (cargo-leptos) — a follow-up, not needed to judge the component model.
use axum::{extract::Path, http::StatusCode, response::Html, response::IntoResponse, response::Response, routing::get, Router};
use leptos::prelude::*;
use rusqlite::{Connection, OptionalExtension};

// ---------- data (identical recursive CTE to web-fast) ----------

thread_local! { static CONN: Connection = open_db(); }
fn open_db() -> Connection {
    let path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "../../.data/vegify.db".into());
    let c = Connection::open(&path).expect("open db");
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").ok();
    c
}

const CTE: &str = "
WITH RECURSIVE
recipe_total AS (
  SELECT r.id AS recipe_id, r.as_ingredient_id, SUM(a.grams) AS total_grams
  FROM recipes r JOIN ingredient_in_recipe iir ON iir.recipe_id=r.id
  JOIN amounts a ON a.id=iir.amount_id GROUP BY r.id),
expand(ingredient_id, eff_grams) AS (
  SELECT rt.as_ingredient_id, rt.total_grams FROM recipe_total rt WHERE rt.recipe_id=?1
  UNION ALL
  SELECT iir.ingredient_id, e.eff_grams*a.grams/rt.total_grams
  FROM expand e JOIN recipes r ON r.as_ingredient_id=e.ingredient_id
  JOIN recipe_total rt ON rt.recipe_id=r.id
  JOIN ingredient_in_recipe iir ON iir.recipe_id=r.id
  JOIN amounts a ON a.id=iir.amount_id)";

#[derive(Clone)]
struct NutRow { name: String, per_serving: f64, unit: String, pct: Option<i64> }

#[derive(Clone)]
struct Page {
    name: String,
    subtitle: Option<String>,
    directions: String,
    ingredients: Vec<String>,
    servings: f64,
    serving_grams: f64,
    calories: f64,
    rows: Vec<NutRow>,
}

fn query_page(id: i64) -> rusqlite::Result<Option<Page>> {
    CONN.with(|c| {
        let meta = c
            .query_row(
                "SELECT i.name, r.subtitle, r.directions, COALESCE(sa.grams,0)
                 FROM recipes r JOIN ingredients i ON i.id=r.as_ingredient_id
                 LEFT JOIN amounts sa ON sa.id=i.serving_size_id WHERE r.id=?1",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, f64>(3)?)),
            )
            .optional()?;
        let Some((name, subtitle, directions, serving_grams)) = meta else { return Ok(None) };
        let batch_grams: f64 = c.query_row(
            "SELECT COALESCE(SUM(a.grams),0) FROM ingredient_in_recipe iir JOIN amounts a ON a.id=iir.amount_id WHERE iir.recipe_id=?1",
            [id], |r| r.get(0))?;
        let mut istmt = c.prepare("SELECT a.amount,a.unit,i.name FROM ingredient_in_recipe iir JOIN ingredients i ON i.id=iir.ingredient_id JOIN amounts a ON a.id=iir.amount_id WHERE iir.recipe_id=?1 ORDER BY iir.\"order\"")?;
        let ingredients = istmt.query_map([id], |r| {
            Ok(format!("{} {} {}", fmt(r.get::<_, Option<f64>>(0)?.unwrap_or(0.0)), r.get::<_, Option<String>>(1)?.unwrap_or_default(), r.get::<_, String>(2)?))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        let cal_total: f64 = c.query_row(&format!("{CTE} SELECT COALESCE(SUM(i.calories_per_100g*e.eff_grams/100.0),0) FROM expand e JOIN ingredients i ON i.id=e.ingredient_id WHERE i.calories_per_100g IS NOT NULL"), [id], |r| r.get(0))?;
        let mut nstmt = c.prepare(&format!("{CTE} SELECT n.name, SUM(inu.amount_per_100g*e.eff_grams/100.0), inu.unit FROM expand e JOIN ingredient_nutrient inu ON inu.ingredient_id=e.ingredient_id JOIN nutrients n ON n.id=inu.nutrient_id GROUP BY n.id ORDER BY n.name"))?;
        let scale = if batch_grams > 0.0 { serving_grams / batch_grams } else { 0.0 };
        let rows = nstmt.query_map([id], |r| {
            let name: String = r.get(0)?; let total: f64 = r.get(1)?; let unit: String = r.get(2)?;
            let ps = total * scale;
            Ok(NutRow { pct: pct_dv(&name, ps, &unit), name, per_serving: ps, unit })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(Page {
            name, subtitle, directions: directions.unwrap_or_else(|| "No directions yet.".into()),
            ingredients,
            servings: if serving_grams > 0.0 { batch_grams / serving_grams } else { 0.0 },
            serving_grams, calories: cal_total * scale, rows,
        }))
    })
}

fn daily_value(name: &str) -> Option<(f64, &'static str)> {
    match name.trim().to_lowercase().as_str() {
        "total protein" | "protein" => Some((50.0, "g")),
        "total fat" | "fat" => Some((78.0, "g")),
        "total carbohydrates" | "carbohydrate" | "carbohydrates" => Some((275.0, "g")),
        "iron" => Some((18.0, "mg")), "calcium" => Some((1300.0, "mg")),
        "vitamin b12" => Some((2.4, "µg")), "vitamin c" => Some((90.0, "mg")),
        "vitamin d" => Some((20.0, "µg")), "potassium" => Some((4700.0, "mg")),
        "sodium" => Some((2300.0, "mg")), "magnesium" => Some((420.0, "mg")),
        _ => None,
    }
}
fn to_ug(a: f64, u: &str) -> f64 { match u.to_lowercase().as_str() { "g" => a*1e6, "mg" => a*1e3, "µg"|"mcg"|"ug" => a, _ => f64::NAN } }
fn pct_dv(name: &str, ps: f64, unit: &str) -> Option<i64> {
    let (dv, du) = daily_value(name)?; let b = to_ug(ps, unit); let d = to_ug(dv, du);
    if b.is_nan() || d == 0.0 { None } else { Some((b / d * 100.0).round() as i64) }
}
fn fmt(n: f64) -> String { let r = (n*10.0).round()/10.0; if r.fract()==0.0 { format!("{}", r as i64) } else { format!("{r}") } }

// ---------- components (the DX showcase: composable #[component] + view!) ----------

#[component]
fn NutritionFacts(
    servings: f64,
    serving_grams: f64,
    calories: f64,
    rows: Vec<NutRow>,
) -> impl IntoView {
    view! {
        <aside class="panel">
            <h2 class="nf">"Nutrition Facts"</h2>
            <p class="muted">
                "This Recipe" <br/>
                {format!("{} servings per batch", fmt(servings))} <br/>
                {format!("Serving size {} g", fmt(serving_grams))}
            </p>
            <div class="cal"><span>"Calories"</span><span>{fmt(calories)}</span></div>
            <table>
                {rows.into_iter().map(|r| view! {
                    <tr>
                        <td><b>{r.name}</b></td>
                        <td class="r">{format!("{}{}", fmt(r.per_serving), r.unit)}</td>
                        <td class="r">{r.pct.map(|p| format!("{p}%")).unwrap_or_else(|| "—".into())}</td>
                    </tr>
                }).collect_view()}
            </table>
        </aside>
    }
}

#[component]
fn RecipeView(page: Page) -> impl IntoView {
    view! {
        <header class="hdr">"🌱 vegify · leptos (React-like components in Rust, SSR)"</header>
        <main class="grid">
            <section>
                <h1 class="title">{page.name}</h1>
                {page.subtitle.map(|s| view! { <p class="sub">{s}</p> })}
                <h2 class="ctr">"Ingredients"</h2>
                <ul class="ings">
                    {page.ingredients.into_iter().map(|i| view! { <li>{i}</li> }).collect_view()}
                </ul>
                <h2 class="ctr">"Directions"</h2>
                <p>{page.directions}</p>
            </section>
            <NutritionFacts
                servings=page.servings
                serving_grams=page.serving_grams
                calories=page.calories
                rows=page.rows
            />
        </main>
    }
}

const STYLE: &str = "
:root{font-family:'Avenir Next',system-ui,sans-serif;color:#2B2B2B}
body{margin:0}.hdr{background:#328432;color:#F1F1F1;padding:14px 24px;font-weight:700}
.grid{max-width:900px;margin:0 auto;padding:24px;display:grid;grid-template-columns:1fr 320px;gap:32px}
.title{color:#328432;text-align:center}.ctr{text-align:center}.sub{text-align:center;color:#8A8B8A}
.ings{columns:2;list-style:disc;padding-left:20px}.muted{color:#8A8B8A}
.panel{border:1px solid #eee;border-radius:12px;padding:16px}.nf{border-bottom:4px solid #2B2B2B}
table{width:100%;border-collapse:collapse;font-size:14px}td{border-bottom:1px solid #f1f1f1;padding:2px 0}
.r{text-align:right}.cal{font-size:28px;font-weight:800;display:flex;justify-content:space-between;border-bottom:4px solid #2B2B2B}
@media(max-width:700px){.grid{grid-template-columns:1fr}}";

async fn recipe(Path(id): Path<i64>) -> Response {
    match tokio::task::spawn_blocking(move || query_page(id)).await {
        Ok(Ok(Some(page))) => {
            // Leptos 0.8: components instantiate in an Owner scope; render via RenderHtml::to_html().
            let owner = Owner::new();
            let body = owner.with(|| view! { <RecipeView page=page/> }.to_html());
            Html(format!(
                "<!doctype html><html lang=en><head><meta charset=utf-8>\
                 <meta name=viewport content=\"width=device-width,initial-scale=1\">\
                 <style>{STYLE}</style></head><body>{body}</body></html>"
            )).into_response()
        }
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "recipe not found").into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "error").into_response(),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/recipes/{id}", get(recipe))
        .route("/healthz", get(|| async { "ok" }));
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(39060);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.unwrap();
    println!("web-leptos listening on :{port}");
    axum::serve(listener, app).await.unwrap();
}
