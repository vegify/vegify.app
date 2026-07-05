//! vegify-admin — operator tooling that works through the LIVE content API as a signed-in user
//! (never the DB directly: the API is the only write path, so every owner gate, cascade rule, and
//! WS fanout applies exactly as it would for any client).
//!
//! v1: `purge-content` — delete ALL recipes and leaf ingredients OWNED BY the signed-in user (the
//! seed/test-content cleanup that makes room for the USDA catalog + the CompleteFoods import).
//! Recipes go first (freeing their ingredient references and their as-ingredient cards), then a
//! re-pull, then the remaining owned leaf ingredients; anything still refused (e.g. used by another
//! user's recipe) is reported and skipped — the DAL's friendly refusal, not a crash.
//!
//! DRY-RUN BY DEFAULT: prints exactly what would be deleted; nothing happens without `--yes`.
//!
//!   VEGIFY_EMAIL=you@x VEGIFY_PASSWORD=... cargo run -p vegify-admin -- purge-content [--yes]
//!   (VEGIFY_API_URL defaults to http://localhost:8787 — point it at prod deliberately.)
use serde::Deserialize;

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
    user: WhoAmI,
}

#[derive(Deserialize)]
struct WhoAmI {
    id: String,
    username: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRow {
    id: String,
    name: String,
    user_id: Option<String>,
}

#[derive(Deserialize)]
struct PullPayload {
    recipes: Vec<PullRow>,
    ingredients: Vec<PullRow>,
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

fn login() -> Result<LoginResponse, String> {
    let email = std::env::var("VEGIFY_EMAIL").map_err(|_| "set VEGIFY_EMAIL".to_string())?;
    let password = std::env::var("VEGIFY_PASSWORD").map_err(|_| "set VEGIFY_PASSWORD".to_string())?;
    ureq::post(&format!("{}/api/auth/login", base_url()))
        .send_json(serde_json::json!({ "email": email, "password": password }))
        .map_err(err_msg)?
        .into_json()
        .map_err(|e| e.to_string())
}

fn pull(token: &str) -> Result<PullPayload, String> {
    ureq::get(&format!("{}/api/content/pull", base_url()))
        .set("authorization", &format!("Bearer {token}"))
        .call()
        .map_err(err_msg)?
        .into_json()
        .map_err(|e| e.to_string())
}

fn delete(token: &str, collection: &str, id: &str) -> Result<(), String> {
    ureq::delete(&format!("{}/api/content/{collection}?id={id}", base_url()))
        .set("authorization", &format!("Bearer {token}"))
        .call()
        .map_err(err_msg)?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("purge-content") {
        eprintln!("usage: vegify-admin purge-content [--yes]   (env: VEGIFY_API_URL, VEGIFY_EMAIL, VEGIFY_PASSWORD)");
        std::process::exit(2);
    }
    let execute = args.iter().any(|a| a == "--yes");

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
        println!("\nDRY RUN — nothing deleted. Re-run with --yes to purge the {} items above.", recipes.len() + ingredients.len());
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
