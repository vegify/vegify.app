//! vegify-admin — operator tooling that works through the LIVE content API as a signed-in user
//! (never the DB directly: the API is the only write path, so every owner gate, cascade rule, and
//! WS fanout applies exactly as it would for any client).
//!
//! Subcommands (both DRY-RUN by default; `--yes` executes):
//! - `purge-content` — delete ALL recipes + leaf ingredients owned by the signed-in user (the
//!   seed/test-content cleanup). Recipes first (freeing references + their as-ingredient cards),
//!   re-pull, then the remaining owned leaves; refusals are reported and skipped.
//! - `import-completefoods` — move the CompleteFoods capture into vegify (see completefoods.rs).
//!
//!   VEGIFY_EMAIL=you@x VEGIFY_PASSWORD=... cargo run -p vegify-admin -- <subcommand> [--yes]
//!   (VEGIFY_API_URL defaults to http://localhost:8787 — point it at prod deliberately.)
use serde::Deserialize;

mod completefoods;

#[derive(Deserialize)]
pub struct ApiSession {
    pub token: String,
    pub user: WhoAmI,
}

#[derive(Deserialize)]
pub struct WhoAmI {
    pub id: String,
    pub username: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRow {
    pub id: String,
    pub name: String,
    pub user_id: Option<String>,
    #[serde(default)]
    pub calories_per_100g: Option<f64>,
}

#[derive(Deserialize)]
pub struct PullPayload {
    pub recipes: Vec<PullRow>,
    pub ingredients: Vec<PullRow>,
}

#[derive(Deserialize)]
struct ApiError {
    error: String,
}

fn base_url() -> String {
    std::env::var("VEGIFY_API_URL").unwrap_or_else(|_| "http://localhost:8787".to_string())
}

fn err_msg(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => resp
            .into_json::<ApiError>()
            .map(|b| b.error)
            .unwrap_or_else(|_| format!("HTTP {code}")),
        e => format!("network error: {e}"),
    }
}

pub fn login() -> Result<ApiSession, String> {
    let email = std::env::var("VEGIFY_EMAIL").map_err(|_| "set VEGIFY_EMAIL".to_string())?;
    let password = std::env::var("VEGIFY_PASSWORD").map_err(|_| "set VEGIFY_PASSWORD".to_string())?;
    ureq::post(&format!("{}/api/auth/login", base_url()))
        .send_json(serde_json::json!({ "email": email, "password": password }))
        .map_err(err_msg)?
        .into_json()
        .map_err(|e| e.to_string())
}

pub fn pull(token: &str) -> Result<PullPayload, String> {
    ureq::get(&format!("{}/api/content/pull", base_url()))
        .set("authorization", &format!("Bearer {token}"))
        .call()
        .map_err(err_msg)?
        .into_json()
        .map_err(|e| e.to_string())
}

impl ApiSession {
    /// POST a content mutation (`recipes` / `ingredients`), returning the created/updated id.
    pub fn post_content(&self, collection: &str, body: &serde_json::Value) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Created {
            id: String,
        }
        ureq::post(&format!("{}/api/content/{collection}", base_url()))
            .set("authorization", &format!("Bearer {}", self.token))
            .send_json(body)
            .map_err(err_msg)?
            .into_json::<Created>()
            .map(|c| c.id)
            .map_err(|e| e.to_string())
    }
}

fn delete(token: &str, collection: &str, id: &str) -> Result<(), String> {
    ureq::delete(&format!("{}/api/content/{collection}?id={id}", base_url()))
        .set("authorization", &format!("Bearer {token}"))
        .call()
        .map_err(err_msg)?;
    Ok(())
}

fn purge(execute: bool) {
    let session = match login() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("login failed: {e}");
            std::process::exit(1);
        }
    };
    let me = &session.user;
    println!("signed in as @{} against {}", me.username, base_url());

    let world = pull(&session.token).expect("content pull failed");
    let mine = |rows: &[PullRow]| -> Vec<(String, String)> {
        rows.iter()
            .filter(|r| r.user_id.as_deref() == Some(me.id.as_str()))
            .map(|r| (r.id.clone(), r.name.clone()))
            .collect()
    };
    let recipes = mine(&world.recipes);
    let ingredients = mine(&world.ingredients);

    println!("\nOWNED CONTENT ({} recipes, {} ingredients):", recipes.len(), ingredients.len());
    for (_, name) in &recipes {
        println!("  recipe:     {name}");
    }
    for (_, name) in &ingredients {
        println!("  ingredient: {name}");
    }
    if recipes.is_empty() && ingredients.is_empty() {
        println!("nothing to purge.");
        return;
    }
    if !execute {
        println!(
            "\nDRY RUN — nothing deleted. Re-run with --yes to purge the {} items above.",
            recipes.len() + ingredients.len()
        );
        return;
    }

    // Recipes first: each delete also removes its as-ingredient card and frees item references.
    let mut failed = 0usize;
    for (id, name) in &recipes {
        match delete(&session.token, "recipes", id) {
            Ok(()) => println!("deleted recipe: {name}"),
            Err(e) => {
                failed += 1;
                eprintln!("SKIPPED recipe {name}: {e}");
            }
        }
    }
    // Re-pull: recipe deletes removed cards + freed leaves; the remaining owned ingredients go now.
    let world = pull(&session.token).expect("re-pull failed");
    for (id, name) in mine(&world.ingredients) {
        match delete(&session.token, "ingredients", &id) {
            Ok(()) => println!("deleted ingredient: {name}"),
            Err(e) => {
                failed += 1;
                eprintln!("SKIPPED ingredient {name}: {e}");
            }
        }
    }
    let world = pull(&session.token).expect("final pull failed");
    println!(
        "\ndone. remaining owned: {} recipes, {} ingredients ({} skipped — see above).",
        mine(&world.recipes).len(),
        mine(&world.ingredients).len(),
        failed
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let execute = args.iter().any(|a| a == "--yes");
    match args.get(1).map(String::as_str) {
        Some("purge-content") => purge(execute),
        Some("import-completefoods") => completefoods::run(execute),
        _ => {
            eprintln!(
                "usage: vegify-admin <purge-content|import-completefoods> [--yes]   (env: VEGIFY_API_URL, VEGIFY_EMAIL, VEGIFY_PASSWORD)"
            );
            std::process::exit(2);
        }
    }
}
