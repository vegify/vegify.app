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

/// A ureq agent that hands back non-2xx responses (with their JSON error body) rather than dropping
/// the body into a bare status error — so [`read`]/[`expect_ok`] can surface the server's message.
fn client() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into()
}

/// A ureq transport failure → a message. With `http_status_as_error(false)`, a non-2xx is NOT an
/// `Err` here (it's `Ok(response)`, handled by [`read`]/[`expect_ok`]), so this is always network.
fn err_msg(e: ureq::Error) -> String {
    format!("network error: {e}")
}

/// Read a 2xx JSON body, else the server's `{error}` message (or `HTTP <code>`) as a String error.
fn read<T: serde::de::DeserializeOwned>(
    mut resp: ureq::http::Response<ureq::Body>,
) -> Result<T, String> {
    if resp.status().is_success() {
        resp.body_mut().read_json::<T>().map_err(|e| e.to_string())
    } else {
        let code = resp.status().as_u16();
        Err(resp
            .body_mut()
            .read_json::<ApiError>()
            .map(|b| b.error)
            .unwrap_or_else(|_| format!("HTTP {code}")))
    }
}

/// Assert a 2xx (body discarded), else the server's `{error}` message as a String error.
fn expect_ok(mut resp: ureq::http::Response<ureq::Body>) -> Result<(), String> {
    if resp.status().is_success() {
        Ok(())
    } else {
        let code = resp.status().as_u16();
        Err(resp
            .body_mut()
            .read_json::<ApiError>()
            .map(|b| b.error)
            .unwrap_or_else(|_| format!("HTTP {code}")))
    }
}

pub fn login() -> Result<ApiSession, String> {
    let email = std::env::var("VEGIFY_EMAIL").map_err(|_| "set VEGIFY_EMAIL".to_string())?;
    let password =
        std::env::var("VEGIFY_PASSWORD").map_err(|_| "set VEGIFY_PASSWORD".to_string())?;
    let resp = client()
        .post(format!("{}/api/auth/login", base_url()))
        .send_json(serde_json::json!({ "email": email, "password": password }))
        .map_err(err_msg)?;
    read(resp)
}

pub fn pull(token: &str) -> Result<PullPayload, String> {
    let resp = client()
        .get(format!("{}/api/content/pull", base_url()))
        .header("authorization", &format!("Bearer {token}"))
        .call()
        .map_err(err_msg)?;
    read(resp)
}

impl ApiSession {
    /// POST a content mutation (`recipes` / `ingredients`), returning the created/updated id.
    pub fn post_content(
        &self,
        collection: &str,
        body: &serde_json::Value,
    ) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Created {
            id: String,
        }
        let resp = client()
            .post(format!("{}/api/content/{collection}", base_url()))
            .header("authorization", &format!("Bearer {}", self.token))
            .send_json(body)
            .map_err(err_msg)?;
        read::<Created>(resp).map(|c| c.id)
    }
}

fn delete(token: &str, collection: &str, id: &str) -> Result<(), String> {
    let resp = client()
        .delete(format!("{}/api/content/{collection}?id={id}", base_url()))
        .header("authorization", &format!("Bearer {token}"))
        .call()
        .map_err(err_msg)?;
    expect_ok(resp)
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

    println!(
        "\nOWNED CONTENT ({} recipes, {} ingredients):",
        recipes.len(),
        ingredients.len()
    );
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

/// `invite <email> [name]` — create an account while public signups stay closed. Authenticates as an
/// admin (VEGIFY_EMAIL/VEGIFY_PASSWORD in the allowlist), generates a strong password, and prints the
/// credentials to hand over (e.g. an App Review demo account).
fn invite(args: &[String]) {
    let email = match args.get(2) {
        Some(e) if e.contains('@') => e.clone(),
        _ => {
            eprintln!("usage: vegify-admin invite <email> [display name]");
            std::process::exit(2);
        }
    };
    let name = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| email.split('@').next().unwrap_or("Guest").to_string());
    let session = match login() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("login failed (admin creds in VEGIFY_EMAIL/VEGIFY_PASSWORD?): {e}");
            std::process::exit(1);
        }
    };
    // A strong, human-typable generated password (the invitee can change it).
    let password = generate_password();
    #[derive(serde::Deserialize)]
    struct Invited {
        user: InvitedUser,
    }
    #[derive(serde::Deserialize)]
    struct InvitedUser {
        username: String,
    }
    match client()
        .post(format!("{}/api/auth/invite", base_url()))
        .header("authorization", &format!("Bearer {}", session.token))
        .send_json(serde_json::json!({ "name": name, "email": email, "password": password }))
    {
        Ok(mut resp) if resp.status().is_success() => {
            let username = resp
                .body_mut()
                .read_json::<Invited>()
                .map(|i| i.user.username)
                .unwrap_or_default();
            println!("invited @{username}\n");
            println!("  email:    {email}");
            println!("  password: {password}");
            println!("\nHand these to the invitee (or App Review). Public signups stay disabled.");
        }
        Ok(mut resp) => {
            let code = resp.status().as_u16();
            let msg = resp
                .body_mut()
                .read_json::<ApiError>()
                .map(|b| b.error)
                .unwrap_or_else(|_| format!("HTTP {code}"));
            eprintln!("invite failed: {msg}");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("invite failed: {}", err_msg(e));
            std::process::exit(1);
        }
    }
}

/// A strong, unambiguous, typable password: 4 words-ish blocks of base32-safe chars + digits.
fn generate_password() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    const CH: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // no I/O/0/1
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    seed ^= std::process::id() as u128;
    let mut out = String::new();
    for i in 0..20 {
        if i > 0 && i % 5 == 0 {
            out.push('-');
        }
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        out.push(CH[(seed >> 64) as usize % CH.len()] as char);
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let execute = args.iter().any(|a| a == "--yes");
    match args.get(1).map(String::as_str) {
        Some("purge-content") => purge(execute),
        Some("import-completefoods") => completefoods::run(execute),
        Some("invite") => invite(&args),
        _ => {
            eprintln!(
                "usage: vegify-admin <purge-content|import-completefoods|invite <email> [name]> [--yes]   (env: VEGIFY_API_URL, VEGIFY_EMAIL, VEGIFY_PASSWORD)"
            );
            std::process::exit(2);
        }
    }
}
