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

// Same shadcn/brand Tailwind classes as packages/ui/src/nutrition-facts.tsx — the design
// system (tokens → CSS) transfers verbatim; only the authoring (view!) is Rust.
#[component]
fn NutritionFacts(
    servings: f64,
    serving_grams: f64,
    calories: f64,
    rows: Vec<NutRow>,
) -> impl IntoView {
    view! {
        <div class="text-sm text-foreground">
            <div class="flex items-center justify-between border-b-4 border-foreground pb-1">
                <h2 class="text-2xl font-extrabold tracking-tight">"Nutrition Facts"</h2>
            </div>
            <p class="mt-2 text-lg font-semibold">"This Recipe"</p>
            <p>{format!("{} servings per batch", fmt(servings))}</p>
            <p class="font-semibold">{format!("Serving size {} g", fmt(serving_grams))}</p>
            <div class="mt-1 border-t-8 border-foreground"></div>
            <p class="pt-1 text-xs font-semibold">"Amount per serving"</p>
            <div class="flex items-end justify-between border-b-4 border-foreground pb-1">
                <span class="text-3xl font-extrabold">"Calories"</span>
                <span class="text-3xl font-extrabold">{fmt(calories)}</span>
            </div>
            <p class="mt-1 border-b border-foreground pb-0.5 text-right text-xs font-bold">
                "% Daily Value*"
            </p>
            <div>
                {rows.into_iter().map(|r| view! {
                    <div class="flex items-baseline justify-between border-b border-foreground/15 py-0.5">
                        <span><b>{r.name}</b>" "{format!("{}{}", fmt(r.per_serving), r.unit)}</span>
                        <span class="font-bold">
                            {r.pct.map(|p| format!("{p}%")).unwrap_or_else(|| "—".into())}
                        </span>
                    </div>
                }).collect_view()}
            </div>
            <p class="mt-2 border-t border-foreground pt-1 text-[10px] leading-tight text-muted-foreground">
                "* Percent Daily Values are based on a 2,000 calorie diet."
            </p>
        </div>
    }
}

#[component]
fn RecipeView(page: Page) -> impl IntoView {
    view! {
        <header class="flex items-center gap-2 bg-green-dark px-6 py-3.5 text-white">
            <span class="text-2xl font-semibold lowercase tracking-tight">"vegify"</span>
            <span class="text-sm opacity-80">"· leptos (Rust components, SSR, real shadcn theme)"</span>
        </header>
        <main class="mx-auto grid max-w-4xl gap-8 p-6 lg:grid-cols-[1fr_320px] lg:p-8">
            <section>
                <h1 class="mt-2 text-center text-4xl font-bold text-primary-dark">{page.name}</h1>
                {page.subtitle.map(|s| view! {
                    <p class="mt-1 text-center text-muted-foreground">{s}</p>
                })}
                <h2 class="mt-8 text-center text-xl font-bold">"Ingredients"</h2>
                <ul class="mx-auto mt-4 grid max-w-2xl list-disc grid-cols-1 gap-x-8 gap-y-1.5 pl-5 marker:text-primary sm:grid-cols-2 lg:grid-cols-3">
                    {page.ingredients.into_iter().map(|i| view! { <li>{i}</li> }).collect_view()}
                </ul>
                <h2 class="mt-8 text-center text-xl font-bold">"Directions"</h2>
                <p class="mt-3 text-muted-foreground">{page.directions}</p>
            </section>
            <aside class="lg:border-l lg:border-border lg:pl-6">
                <NutritionFacts
                    servings=page.servings
                    serving_grams=page.serving_grams
                    calories=page.calories
                    rows=page.rows
                />
            </aside>
        </main>
    }
}

// The real design system: Tailwind v4 + @vegify/tokens, compiled from style/app.css by scanning
// these .rs files for class names. Embedded at compile time (run `build:css` before `cargo build`).
const CSS: &str = include_str!("../style/out.css");

async fn recipe(Path(id): Path<i64>) -> Response {
    match tokio::task::spawn_blocking(move || query_page(id)).await {
        Ok(Ok(Some(page))) => {
            // Leptos 0.8: components instantiate in an Owner scope; render via RenderHtml::to_html().
            let owner = Owner::new();
            let body = owner.with(|| view! { <RecipeView page=page/> }.to_html());
            Html(format!(
                "<!doctype html><html lang=en><head><meta charset=utf-8>\
                 <meta name=viewport content=\"width=device-width,initial-scale=1\">\
                 <style>{CSS}</style></head><body class=\"bg-background text-foreground antialiased\">{body}</body></html>"
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
