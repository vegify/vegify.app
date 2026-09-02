//! Tests for the data module. Kept in their own file so code scanning can skip test
//! fixtures (see `.github/codeql/codeql-config.yml`).

use super::*;
use std::fs;

fn recipe_id_by_name(db: &Db, needle: &str) -> String {
    VegifyData::list_recipes(db, Page::default())
        .expect("list")
        .into_iter()
        .find(|c| c.name.contains(needle))
        .unwrap_or_else(|| panic!("no recipe matching {needle:?}"))
        .id
}

/// Stamp the in-memory session as `id` (writes get owned by it; reads scope to it).
fn set_auth(db: &Db, id: &str, name: &str) {
    *db.auth.lock().unwrap() = Some(Session {
        token: "t".into(),
        user: AuthUser {
            id: id.into(),
            name: name.into(),
            username: name.to_lowercase(),
            email: format!("{name}@x"),
            email_verified: false,
        },
    });
}

/// Sign in as the seed user (owns all seed content), returning its id.
fn sign_in_seed(db: &Db) -> String {
    let uid: String = db
        .conn
        .lock()
        .unwrap()
        .query_row("SELECT id FROM users LIMIT 1", [], |r| r.get(0))
        .expect("a seed user exists");
    set_auth(db, &uid, "Seed");
    uid
}

#[test]
fn recipe_nutrition_on_device() {
    let db = Db::open(&crate::db_path()).expect("open");
    let id = recipe_id_by_name(&db, "Complete Shake");
    let r = VegifyData::recipe(&db, id)
        .expect("query ok")
        .expect("recipe exists");
    let cal100 = r.nutrition.calories_per_100g.expect("has calories");
    let grams = r.serving.as_ref().expect("has serving").grams;
    let per_serving = cal100 * grams / 100.0;
    eprintln!(
        "recipe {:?}: {:.1} cal/serving (ULID {})",
        r.name, per_serving, r.id
    );
    assert!((per_serving - 307.5).abs() < 0.5, "got {per_serving:.2}");
}

// The chrome/global search DAL method (P2.4), against the real seeded local cache — proves the
// desktop's FTS5 index (built by `ensure_content_schema` on `Db::open`) actually powers it, not
// just the vegify-core unit tests' synthetic fixtures.
#[test]
fn search_content_hits_the_local_fts5_index() {
    let db = Db::open(&crate::db_path()).expect("open");
    let res = VegifyData::search_content(&db, "Flour".into()).expect("search");
    assert!(
        res.ingredients.iter().any(|i| i.name.contains("Flour")),
        "the seeded Flour ingredient should surface: {:?}",
        res.ingredients.iter().map(|i| &i.name).collect::<Vec<_>>()
    );
}

// The exact path the write UI drives: search an ingredient → save a recipe USING it as an item
// → read it back with items + aggregated nutrition. (Covers do_save_recipe's item INSERTs,
// which the empty-items sync test does not.)
#[test]
fn save_recipe_with_items_via_search_flow() {
    let db_path = std::env::temp_dir().join("vegify-uiflow.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    sign_in_seed(&db); // own the saved recipe so the owner-gated edit-load returns it

    let hits = VegifyData::search_ingredients(&db, "Flour".into()).expect("search");
    let flour = hits
        .into_iter()
        .find(|h| h.name.contains("Flour"))
        .expect("a flour exists");

    let id = VegifyData::save_recipe(
        &db,
        SaveRecipeInput {
            id: None,
            as_ingredient_id: None,
            visibility: None,
            name: "UI Flow Bread".into(),
            subtitle: None,
            directions: Some("mix".into()),
            serving_grams: Some(100.0),
            batch_grams: Some(500.0),
            items: vec![RecipeItemInput {
                ingredient_id: flour.id.clone(),
                grams: 500.0,
                unit: None,
                amount: None,
            }],
            slug: None,
        },
    )
    .expect("save recipe with an item");

    let r = VegifyData::recipe(&db, id.clone())
        .expect("read")
        .expect("exists");
    assert_eq!(r.name, "UI Flow Bread");
    assert_eq!(
        r.items.len(),
        1,
        "the searched ingredient is attached as an item"
    );
    assert_eq!(r.items[0].name, flour.name);
    assert!(
        r.nutrition.calories_per_100g.is_some(),
        "flour has calories → recipe aggregates them"
    );
    eprintln!(
        "UI-flow recipe: {} item(s), {:?} cal/100g",
        r.items.len(),
        r.nutrition.calories_per_100g
    );

    // edit-with-defaults: per-item nutrition + servings (batch/serving = 500/100 = 5).
    let edit = VegifyData::recipe_for_edit(&db, id)
        .expect("edit")
        .expect("exists");
    assert_eq!(edit.servings, Some(5.0));
    assert_eq!(edit.items.len(), 1);
    assert_eq!(edit.items[0].calories_per_100g, Some(364.0));
}

// WHY `branded_promote` pulls before it hands back an id (P2.1/P2.2). Promotion creates the
// catalog row on the SERVER; this shell logs against the LOCAL mirror, which both validates the
// ingredient itself and carries a real foreign key on `log_entries.ingredient_id`. So an id the
// mirror hasn't pulled yet is not a soft miss that heals on the next sync — the write is refused
// outright and the user's tap silently produces nothing. This pins that failure mode, so the pull
// inside `branded_promote` can't later be "simplified" away as a redundant round trip.
#[test]
fn logging_an_ingredient_the_mirror_has_not_pulled_is_refused() {
    let db_path = std::env::temp_dir().join("vegify-branded-fk.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    let uid = sign_in_seed(&db);

    let before = VegifyData::log_day(&db, "2026-08-10".into())
        .expect("day reads")
        .entries
        .len();
    VegifyData::save_log_entry(
        &db,
        SaveLogEntryInput {
            id: None,
            // Shaped exactly like a promoted id the server just minted and this device has not
            // pulled — the state the UI would be in if promote returned before pulling.
            ingredient_id: "01JZZZZZZZZZZZZZZZZZZZZZZZ".into(),
            date: "2026-08-10".into(),
            slot: None,
            grams: 240.0,
            unit: None,
            logged_at: None,
        },
    )
    .expect_err("an unpulled ingredient id must not be loggable");

    let after = VegifyData::log_day(&db, "2026-08-10".into())
        .expect("day reads")
        .entries
        .len();
    assert_eq!(after, before, "nothing was written for user {uid}");
}

// Ingredient browser + edit: a saved ingredient is listed (leaf only, not recipe
// as-ingredients) and its edit defaults (per-100g + own nutrients) round-trip.
#[test]
fn ingredient_browser_and_edit_round_trip() {
    let db_path = std::env::temp_dir().join("vegify-ing.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    sign_in_seed(&db); // own the saved ingredient so the owner-gated edit-load returns it

    let id = VegifyData::save_ingredient(
        &db,
        SaveIngredientInput {
            id: None,
            visibility: None,
            name: "Test Tofu".into(),
            description: Some("firm".into()),
            price: Some(250),
            calories_per_100g: Some(144.0),
            serving_grams: Some(85.0),
            serving_unit: None,
            package_grams: Some(340.0),
            nutrients: vec![IngredientNutrientInput {
                name: "Protein".into(),
                amount_per_100g: 17.3,
                unit: "g".into(),
            }],
            slug: None,
        },
    )
    .expect("save ingredient");

    let cards = VegifyData::list_ingredients(&db, Page::default()).expect("list");
    assert!(
        cards.iter().any(|c| c.name == "Test Tofu"),
        "browser shows the new ingredient"
    );
    assert!(
        !cards.iter().any(|c| c.name.contains("Complete Shake")),
        "browser excludes recipe as-ingredients"
    );

    let e = VegifyData::ingredient_for_edit(&db, id)
        .expect("edit")
        .expect("exists");
    assert_eq!(e.name, "Test Tofu");
    assert_eq!(e.serving_grams, Some(85.0));
    assert_eq!(e.calories_per_100g, Some(144.0));
    assert_eq!(e.nutrients.len(), 1);
    assert_eq!(e.nutrients[0].name, "Protein");
    eprintln!(
        "ingredient edit: {} nutrient(s), serving {:?}g",
        e.nutrients.len(),
        e.serving_grams
    );
}

// A signed-in user's writes are stamped with their id. foreign_keys=ON requires that user to
// exist locally — which ensure_user_local guarantees in the real flow; here we use a seed user.
#[test]
fn writes_stamp_signed_in_user() {
    let db_path = std::env::temp_dir().join("vegify-stamp.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");

    let uid = sign_in_seed(&db);

    let rid = VegifyData::save_recipe(
        &db,
        SaveRecipeInput {
            id: None,
            as_ingredient_id: None,
            visibility: None,
            name: "Owned Recipe".into(),
            subtitle: None,
            directions: None,
            serving_grams: Some(100.0),
            batch_grams: Some(200.0),
            items: vec![],
            slug: None,
        },
    )
    .expect("save");

    let owner: Option<String> = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT i.user_id FROM recipes r JOIN ingredients i ON i.id = r.as_ingredient_id WHERE r.id = ?1",
            [&rid],
            |r| r.get(0),
        )
        .expect("query owner");
    assert_eq!(
        owner.as_deref(),
        Some(uid.as_str()),
        "recipe is stamped with the signed-in user"
    );
}

// The branded IPC proxies against a REAL server (P2.1/P2.2). #[ignore]'d like the sign-in test —
// it needs the network, an FDC key, and a throwaway-DB server — but it is the only thing that
// exercises the actual wire the desktop uses: ureq's query encoding for a multi-word search, the
// `{source, externalId}` promote body, and (the part that matters) that the id `branded_promote`
// hands back is one the LOCAL mirror can already resolve, because it pulled first.
// The account comes from the environment rather than a literal — the throwaway server is stood up
// per run, so its credentials are part of that setup, not of this file.
//   DATABASE_PATH=/tmp/throwaway.db PORT=47411 VEGIFY_FDC_API_KEY=… ./target/debug/vegify-server &
//   VEGIFY_AUTH_URL=http://127.0.0.1:47411 VEGIFY_TEST_EMAIL=… VEGIFY_TEST_PASSWORD=… \
//     cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib branded_lookup -- --ignored --nocapture
#[test]
#[ignore]
fn branded_lookup_promote_and_log_against_a_server() {
    let (Ok(email), Ok(password)) = (
        std::env::var("VEGIFY_TEST_EMAIL"),
        std::env::var("VEGIFY_TEST_PASSWORD"),
    ) else {
        eprintln!("set VEGIFY_TEST_EMAIL + VEGIFY_TEST_PASSWORD (see the comment above)");
        return;
    };
    let db_path = std::env::temp_dir().join("vegify-branded-ipc.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    let _ = VegifyData::sign_out(&db);
    let user = VegifyData::sign_in(&db, SignInInput { email, password })
        .expect("sign in against the throwaway server");

    // A MULTI-WORD query: the encoding case a hand-built URL gets wrong.
    let hits = VegifyData::branded_search(&db, "milk chocolate".into()).expect("branded search");
    assert!(!hits.is_empty(), "the search returned nothing");
    let dairy = hits
        .iter()
        .find(|f| f.diet_flags.iter().any(|d| d.term == "milk"))
        .expect("a milk-chocolate result must carry the dairy flag");
    eprintln!(
        "{} hits; picked {:?} flags={:?}",
        hits.len(),
        dairy.name,
        dairy.diet_flags.iter().map(|d| &d.term).collect::<Vec<_>>()
    );

    // Promote, then immediately log against the returned id. This is the whole contract: if
    // `branded_promote` returned before pulling, this write would be refused (see
    // `logging_an_ingredient_the_mirror_has_not_pulled_is_refused`).
    let ingredient_id =
        VegifyData::branded_promote(&db, dairy.source, dairy.external_id.clone()).expect("promote");
    let entry = VegifyData::save_log_entry(
        &db,
        SaveLogEntryInput {
            id: None,
            ingredient_id: ingredient_id.clone(),
            date: "2026-08-10".into(),
            slot: None,
            grams: dairy.serving_grams.unwrap_or(100.0),
            unit: None,
            logged_at: None,
        },
    )
    .expect("log the promoted food — the mirror must already have it");
    let day = VegifyData::log_day(&db, "2026-08-10".into()).expect("day");
    assert!(
        day.entries.iter().any(|e| e.id == entry),
        "the promoted branded food is in {}'s diary",
        user.name
    );

    // Idempotent: promoting the same food twice is the same catalog row.
    let again = VegifyData::branded_promote(&db, dairy.source, dairy.external_id.clone())
        .expect("promote again");
    assert_eq!(again, ingredient_id, "promote-on-FIRST-use");

    // A barcode nothing resolves is a None, not an error.
    if let Some(gtin) = dairy.gtin.clone() {
        let by_code = VegifyData::branded_barcode(&db, gtin).expect("barcode lookup");
        assert!(by_code.is_some(), "the GTIN we just saw must resolve");
    }
    VegifyData::sign_out(&db).expect("sign out");
}

// Full sign-in path against a running web shell (real network + OS keychain). #[ignore]'d so the
// default suite needs neither. Run with the web build served on $VEGIFY_AUTH_URL:
//   VEGIFY_AUTH_URL=http://localhost:39008 \
//     cargo test --lib sign_in_round_trip -- --ignored --nocapture
#[test]
#[ignore]
fn sign_in_round_trip_against_web() {
    let db_path = std::env::temp_dir().join("vegify-signin.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    let _ = VegifyData::sign_out(&db); // start from a clean keychain slot

    let user = VegifyData::sign_in(
        &db,
        SignInInput {
            email: "dev@example.com".into(),
            password: "dev-password".into(),
        },
    )
    .expect("sign in");
    assert_eq!(user.email, "dev@example.com");
    eprintln!("signed in as {} ({})", user.name, user.id);

    // A fresh Db (simulating relaunch) restores the session from the keychain — offline-capable.
    let db2 = Db::open(db_path.to_str().unwrap()).expect("reopen");
    let restored = VegifyData::current_user(&db2)
        .expect("current")
        .expect("restored from keychain");
    assert_eq!(
        restored.id, user.id,
        "session persisted to the keychain across reopen"
    );

    // Wrong password is rejected (the server's message surfaces as DataError::Auth).
    let bad = VegifyData::sign_in(
        &db,
        SignInInput {
            email: "dev@example.com".into(),
            password: "nope".into(),
        },
    );
    assert!(bad.is_err(), "wrong password should be rejected");

    VegifyData::sign_out(&db).expect("sign out");
    assert!(
        VegifyData::current_user(&db).expect("current").is_none(),
        "cleared after sign out"
    );
}

// A4 visibility model (mirrors the web's 2-user test): public content is shared, private is
// owner-only, and edit/delete are owner-gated. Two accounts over one local DB exercise the
// per-viewer policy directly (the desktop is single-user-local until A3, but the DAL gates are
// per-viewer regardless). Covers BOTH entities: a recipe IS an ingredient at the data level, but
// the recipe read procedures have their own SQL gates, so they're asserted separately below.
#[test]
fn visibility_scopes_reads_and_guards_writes() {
    let db_path = std::env::temp_dir().join("vegify-vis.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");

    // Two accounts: john (the seed owner) + Bob (upserted locally, as sign-in would guarantee).
    let john = sign_in_seed(&db);
    let bob = new_id();
    db.ensure_user_local(&AuthUser {
        id: bob.clone(),
        name: "Bob".into(),
        username: "bob".into(),
        email: "bob@x".into(),
        email_verified: false,
    })
    .expect("create bob");

    // John creates a PRIVATE and a PUBLIC ingredient.
    let mk = |name: &str, vis: Visibility| {
        VegifyData::save_ingredient(
            &db,
            SaveIngredientInput {
                id: None,
                visibility: Some(vis),
                name: name.into(),
                description: None,
                price: None,
                calories_per_100g: Some(50.0),
                serving_grams: Some(100.0),
                serving_unit: None,
                package_grams: None,
                nutrients: vec![],
                slug: None,
            },
        )
        .expect("john saves")
    };
    let secret = mk("John Secret Sauce", Visibility::Private);
    let public = mk("John Public Sauce", Visibility::Public);

    // John also creates a PRIVATE recipe — the recipe read procs apply their own SQL gates.
    let secret_recipe = VegifyData::save_recipe(
        &db,
        SaveRecipeInput {
            id: None,
            as_ingredient_id: None,
            visibility: Some(Visibility::Private),
            name: "John Secret Recipe".into(),
            subtitle: None,
            directions: None,
            serving_grams: Some(100.0),
            batch_grams: Some(200.0),
            items: vec![],
            slug: None,
        },
    )
    .expect("john saves recipe");

    // --- As Bob (a different account) ---
    set_auth(&db, &bob, "Bob");

    // Lists + search (isListed): Bob sees John's PUBLIC, not John's private.
    let listed: Vec<String> = VegifyData::list_ingredients(&db, Page::default())
        .expect("bob list")
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert!(
        listed.contains(&"John Public Sauce".to_string()),
        "bob sees john's public"
    );
    assert!(
        !listed.contains(&"John Secret Sauce".to_string()),
        "bob must NOT see john's private"
    );
    let found: Vec<String> = VegifyData::search_ingredients(&db, "John".into())
        .expect("bob search")
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert!(
        found.contains(&"John Public Sauce".to_string()),
        "search returns public"
    );
    assert!(
        !found.contains(&"John Secret Sauce".to_string()),
        "search hides private"
    );

    // Detail (canView): public viewable; private 404s (None). can_edit is false for the non-owner.
    let bob_view = VegifyData::ingredient(&db, public.clone())
        .expect("ok")
        .expect("public viewable");
    assert!(
        !bob_view.can_edit,
        "non-owner sees no edit affordance on public content"
    );
    assert!(
        VegifyData::ingredient(&db, secret.clone())
            .expect("ok")
            .is_none(),
        "private hidden"
    );

    // Edit-load (isOwner): Bob can't load John's public ingredient for editing.
    assert!(
        VegifyData::ingredient_for_edit(&db, public.clone())
            .expect("ok")
            .is_none(),
        "non-owner can't edit-load"
    );

    // Mutations owner-guarded: Bob can neither edit nor delete John's ingredient.
    let hijack = VegifyData::save_ingredient(
        &db,
        SaveIngredientInput {
            id: Some(public.clone()),
            visibility: Some(Visibility::Private),
            name: "Hijacked".into(),
            description: None,
            price: None,
            calories_per_100g: None,
            serving_grams: None,
            serving_unit: None,
            package_grams: None,
            nutrients: vec![],
            slug: None,
        },
    );
    assert!(hijack.is_err(), "bob can't edit john's ingredient");
    assert!(
        VegifyData::delete_ingredient(&db, public.clone()).is_err(),
        "bob can't delete john's"
    );

    // Recipe gates (separate SQL from the ingredient path): John's private recipe is hidden,
    // unviewable, un-editable, and un-deletable by Bob.
    let recipe_names: Vec<String> = VegifyData::list_recipes(&db, Page::default())
        .expect("bob recipes")
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert!(
        !recipe_names.contains(&"John Secret Recipe".to_string()),
        "private recipe not listed"
    );
    assert!(
        VegifyData::recipe(&db, secret_recipe.clone())
            .expect("ok")
            .is_none(),
        "private recipe 404s"
    );
    assert!(
        VegifyData::recipe_for_edit(&db, secret_recipe.clone())
            .expect("ok")
            .is_none(),
        "non-owner can't edit-load the recipe"
    );
    assert!(
        VegifyData::delete_recipe(&db, secret_recipe.clone()).is_err(),
        "bob can't delete the recipe"
    );

    // --- Back as John (the owner) ---
    set_auth(&db, &john, "John");
    let own_recipe = VegifyData::recipe(&db, secret_recipe.clone())
        .expect("ok")
        .expect("owner views recipe");
    assert!(own_recipe.can_edit, "owner sees the recipe edit affordance");
    assert!(
        VegifyData::recipe_for_edit(&db, secret_recipe)
            .expect("ok")
            .is_some(),
        "owner can edit-load their recipe"
    );
    let e = VegifyData::ingredient_for_edit(&db, secret.clone())
        .expect("ok")
        .expect("owner edit-load");
    assert_eq!(
        e.visibility,
        Visibility::Private,
        "edit defaults carry the stored visibility"
    );
    assert!(e.can_edit, "owner edit-load is editable");
    // Owner can edit (change visibility) then delete own content.
    VegifyData::save_ingredient(
        &db,
        SaveIngredientInput {
            id: Some(secret),
            visibility: Some(Visibility::Unlisted),
            name: "John Secret Sauce".into(),
            description: None,
            price: None,
            calories_per_100g: Some(50.0),
            serving_grams: Some(100.0),
            serving_unit: None,
            package_grams: None,
            nutrients: vec![],
            slug: None,
        },
    )
    .expect("owner edits own");
    let own_ing = VegifyData::ingredient(&db, public.clone())
        .expect("ok")
        .expect("owner views own");
    assert!(
        own_ing.can_edit,
        "owner sees the edit affordance on their own ingredient"
    );
    VegifyData::delete_ingredient(&db, public).expect("owner deletes own");
    eprintln!("visibility: public shared, private hidden from non-owner, edit/delete owner-gated");
}

#[test]
fn writes_require_sign_in() {
    let db_path = std::env::temp_dir().join("vegify-anon-write.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    let _ = VegifyData::sign_out(&db); // ensure logged-out regardless of any stored session

    // Reads work anonymously (public content from the local cache)…
    assert!(
        VegifyData::list_recipes(&db, Page::default()).is_ok(),
        "anonymous reads work"
    );
    // …but writes are refused up front, so nothing is stamped NULL-owner or queued to the outbox.
    let r = VegifyData::save_recipe(
        &db,
        SaveRecipeInput {
            id: None,
            as_ingredient_id: None,
            visibility: Some(Visibility::Public),
            name: "Anon Loaf".into(),
            subtitle: None,
            directions: None,
            serving_grams: None,
            batch_grams: None,
            items: vec![],
            slug: None,
        },
    );
    assert!(
        matches!(r, Err(DataError::Auth(_))),
        "anonymous recipe create is refused"
    );
    let i = VegifyData::save_ingredient(
        &db,
        SaveIngredientInput {
            id: None,
            visibility: Some(Visibility::Public),
            name: "Anon Salt".into(),
            description: None,
            price: None,
            calories_per_100g: None,
            serving_grams: None,
            serving_unit: None,
            package_grams: None,
            nutrients: vec![],
            slug: None,
        },
    );
    assert!(
        matches!(i, Err(DataError::Auth(_))),
        "anonymous ingredient create is refused"
    );
    assert!(
        matches!(
            VegifyData::delete_recipe(&db, new_id()),
            Err(DataError::Auth(_))
        ),
        "anonymous delete is refused"
    );
}

#[test]
fn list_recipes_keyset_paginates_newest_first() {
    let db_path = std::env::temp_dir().join("vegify-keyset-page.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    let _ = VegifyData::sign_out(&db); // public catalog, logged out

    // The full list (default page) is newest-first: ids (ULIDs) strictly descending.
    let all = VegifyData::list_recipes(&db, Page::default()).expect("all");
    assert!(
        all.len() >= 3,
        "seed has several public recipes (got {})",
        all.len()
    );
    assert!(
        all.windows(2).all(|w| w[0].id > w[1].id),
        "ordered newest-first by id"
    );

    // Page 1 (no cursor) is the newest `limit` of that list.
    let p1 = VegifyData::list_recipes(
        &db,
        Page {
            limit: Some(2),
            ..Default::default()
        },
    )
    .expect("page 1");
    assert_eq!(p1.len(), 2, "limit caps the page");
    assert_eq!(
        p1.iter().map(|c| &c.id).collect::<Vec<_>>(),
        all[..2].iter().map(|c| &c.id).collect::<Vec<_>>(),
        "page 1 = the newest two"
    );

    // Page 2 (cursor = page 1's last id) continues with NO overlap and NO skip.
    let p2 = VegifyData::list_recipes(
        &db,
        Page {
            cursor: Some(p1[1].id.clone()),
            limit: Some(2),
            ..Default::default()
        },
    )
    .expect("page 2");
    assert_eq!(
        p2.first().map(|c| &c.id),
        all.get(2).map(|c| &c.id),
        "page 2 starts right after page 1"
    );
    assert!(
        p2.iter().all(|c| p1.iter().all(|x| x.id != c.id)),
        "pages do not overlap"
    );
}

#[test]
fn list_recipes_honors_each_sort() {
    let db_path = std::env::temp_dir().join("vegify-sort.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    let _ = VegifyData::sign_out(&db);

    let list = |sort| {
        VegifyData::list_recipes(
            &db,
            Page {
                sort,
                ..Default::default()
            },
        )
        .expect("list")
    };
    let ids = |cards: &[RecipeCard]| cards.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
    let names = |cards: &[RecipeCard]| cards.iter().map(|c| c.name.clone()).collect::<Vec<_>>();

    let (newest, oldest, az, za) = (
        list(Sort::Newest),
        list(Sort::Oldest),
        list(Sort::NameAsc),
        list(Sort::NameDesc),
    );
    assert!(newest.len() >= 3, "seed has several public recipes");

    // Recency: newest is oldest reversed (same id keyset, opposite direction).
    let mut oldest_rev = ids(&oldest);
    oldest_rev.reverse();
    assert_eq!(ids(&newest), oldest_rev, "newest == oldest reversed");

    // Name: A→Z is name-ascending; Z→A is its reverse.
    let mut sorted = names(&az);
    sorted.sort();
    assert_eq!(names(&az), sorted, "A→Z is name-ascending");
    let mut az_rev = names(&az);
    az_rev.reverse();
    assert_eq!(names(&za), az_rev, "Z→A == A→Z reversed");

    // The composite (name, id) keyset for A→Z paginates without overlap or gaps.
    let p1 = VegifyData::list_recipes(
        &db,
        Page {
            sort: Sort::NameAsc,
            limit: Some(2),
            ..Default::default()
        },
    )
    .expect("az page 1");
    assert_eq!(p1.len(), 2);
    let p2 = VegifyData::list_recipes(
        &db,
        Page {
            sort: Sort::NameAsc,
            cursor: Some(p1[1].id.clone()),
            cursor_name: Some(p1[1].name.clone()),
            limit: Some(2),
        },
    )
    .expect("az page 2");
    assert_eq!(
        p2.first().map(|c| &c.id),
        az.get(2).map(|c| &c.id),
        "A→Z page 2 continues after page 1"
    );
    assert!(
        p2.iter().all(|c| p1.iter().all(|x| x.id != c.id)),
        "A→Z pages do not overlap"
    );
}

// Upsert-by-id + as-ingredient-id threading (step 1 of the sync engine). A supplied-but-absent id
// must CREATE the row WITH that id (not silently no-op an UPDATE), and the recipe's as-ingredient
// id must be honorable so a nested recipe consumed by another (a Biga inside a Dough) keeps a
// stable cross-replica id — the exact shape the sync pull applies. Re-applying is idempotent.
#[test]
fn upsert_by_id_honors_supplied_ids_for_pull() {
    let db_path = std::env::temp_dir().join("vegify-upsert.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    sign_in_seed(&db); // own the rows so the owner-gated edit-load returns them

    // --- ingredient: a supplied-but-absent id is created WITH that id, then updated in place ---
    let ing_id = new_id();
    let returned = VegifyData::save_ingredient(
        &db,
        SaveIngredientInput {
            id: Some(ing_id.clone()),
            visibility: None,
            name: "Pulled Ingredient".into(),
            description: None,
            price: None,
            calories_per_100g: Some(42.0),
            serving_grams: Some(100.0),
            serving_unit: None,
            package_grams: None,
            nutrients: vec![],
            slug: None,
        },
    )
    .expect("save with a supplied id");
    assert_eq!(returned, ing_id, "the supplied id is honored, not minted");
    let loaded = VegifyData::ingredient_for_edit(&db, ing_id.clone())
        .expect("ok")
        .expect("row was created WITH the supplied id (not a no-op UPDATE)");
    assert_eq!(loaded.name, "Pulled Ingredient");

    // re-apply the SAME id with a changed field → updates in place (idempotent, no duplicate row)
    VegifyData::save_ingredient(
        &db,
        SaveIngredientInput {
            id: Some(ing_id.clone()),
            visibility: None,
            name: "Pulled Ingredient v2".into(),
            description: None,
            price: None,
            calories_per_100g: Some(43.0),
            serving_grams: Some(100.0),
            serving_unit: None,
            package_grams: None,
            nutrients: vec![],
            slug: None,
        },
    )
    .expect("re-apply updates");
    let pulled: Vec<String> = VegifyData::list_ingredients(&db, Page::default())
        .expect("list")
        .into_iter()
        .filter(|c| c.name.starts_with("Pulled Ingredient"))
        .map(|c| c.name)
        .collect();
    assert_eq!(
        pulled,
        vec!["Pulled Ingredient v2".to_string()],
        "one row, updated in place"
    );

    // --- recipe + nesting: a Biga (its as-ingredient id supplied) consumed by a Dough as an item ---
    let biga_rid = new_id();
    let biga_aiid = new_id();
    let returned_biga = VegifyData::save_recipe(
        &db,
        SaveRecipeInput {
            id: Some(biga_rid.clone()),
            as_ingredient_id: Some(biga_aiid.clone()),
            visibility: None,
            name: "Pulled Biga".into(),
            subtitle: None,
            directions: None,
            serving_grams: Some(100.0),
            batch_grams: Some(300.0),
            items: vec![],
            slug: None,
        },
    )
    .expect("apply biga with supplied ids");
    assert_eq!(returned_biga, biga_rid, "the supplied recipe id is honored");

    let dough_rid = new_id();
    let dough_aiid = new_id();
    // The Dough item references the BIGA's as-ingredient id — the nested-recipe FK that orphans
    // cross-replica unless the as-ingredient id is threaded and stable. Built fresh per apply so
    // the same pull can be replayed (the input type isn't Clone).
    let build_dough = || SaveRecipeInput {
        id: Some(dough_rid.clone()),
        as_ingredient_id: Some(dough_aiid.clone()),
        visibility: None,
        name: "Pulled Dough".into(),
        subtitle: None,
        directions: None,
        serving_grams: Some(250.0),
        batch_grams: Some(500.0),
        items: vec![RecipeItemInput {
            ingredient_id: biga_aiid.clone(),
            grams: 300.0,
            unit: None,
            amount: None,
        }],
        slug: None,
    };
    let returned_dough =
        VegifyData::save_recipe(&db, build_dough()).expect("apply dough consuming the biga");
    assert_eq!(
        returned_dough, dough_rid,
        "the supplied recipe id is honored"
    );

    let view = VegifyData::recipe(&db, dough_rid.clone())
        .expect("read")
        .expect("exists");
    assert_eq!(
        view.items.len(),
        1,
        "the nested biga item resolved (FK intact via threaded as-ing id)"
    );
    assert_eq!(
        view.items[0].name, "Pulled Biga",
        "the item resolves to the biga's as-ingredient"
    );

    // re-apply the SAME dough (an idempotent pull) → still ONE dough with that id, ONE item
    VegifyData::save_recipe(&db, build_dough()).expect("re-apply dough");
    let doughs: Vec<String> = VegifyData::list_recipes(&db, Page::default())
        .expect("list")
        .into_iter()
        .filter(|c| c.name == "Pulled Dough")
        .map(|c| c.id)
        .collect();
    assert_eq!(
        doughs,
        vec![dough_rid.clone()],
        "idempotent: one dough with the supplied id"
    );
    let again = VegifyData::recipe(&db, dough_rid.clone())
        .expect("read")
        .expect("exists");
    assert_eq!(again.items.len(), 1, "re-apply did not duplicate the item");
    eprintln!("upsert-by-id: supplied ids honored; nested biga/dough FK stable across re-apply");
}

// Step 3: every local content write records a semantic mutation in the _outbox push queue, FIFO,
// with the resolved client id (a create's minted id, captured up front). A recipe's payload also
// carries the as-ingredient id matching the LOCAL row, so a later push creates the server row with
// the same id (cross-replica stability). The server stamps userId from the session, so it's absent.
#[test]
fn writes_record_semantic_outbox() {
    let db_path = std::env::temp_dir().join("vegify-outbox.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    sign_in_seed(&db);

    let outbox = |db: &Db| -> Vec<(String, serde_json::Value)> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT op, payload FROM _outbox ORDER BY seq")
            .unwrap();
        let v = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .map(|row| {
                let (op, p) = row.unwrap();
                (op, serde_json::from_str(&p).unwrap())
            })
            .collect::<Vec<_>>();
        v
    };

    let rid = VegifyData::save_recipe(
        &db,
        SaveRecipeInput {
            id: None,
            as_ingredient_id: None,
            visibility: None,
            name: "Outbox Recipe".into(),
            subtitle: None,
            directions: None,
            serving_grams: Some(100.0),
            batch_grams: Some(200.0),
            items: vec![],
            slug: None,
        },
    )
    .expect("save recipe");
    // capture the local as-ingredient id BEFORE the delete removes the row
    let local_ai: String = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT as_ingredient_id FROM recipes WHERE id = ?1",
            [&rid],
            |r| r.get(0),
        )
        .expect("local recipe row");

    let iid = VegifyData::save_ingredient(
        &db,
        SaveIngredientInput {
            id: None,
            visibility: Some(Visibility::Private),
            name: "Outbox Ingredient".into(),
            description: None,
            price: None,
            calories_per_100g: Some(10.0),
            serving_grams: Some(50.0),
            serving_unit: None,
            package_grams: None,
            nutrients: vec![],
            slug: None,
        },
    )
    .expect("save ingredient");
    VegifyData::delete_recipe(&db, rid.clone()).expect("delete recipe");

    let rows = outbox(&db);
    let ops: Vec<&str> = rows.iter().map(|(op, _)| op.as_str()).collect();
    assert_eq!(
        ops,
        ["saveRecipe", "saveIngredient", "deleteRecipe"],
        "one FIFO entry per write"
    );

    // saveRecipe payload = the content-API body: resolved recipe id, camelCase fields, NO userId.
    let recipe_payload = &rows[0].1;
    assert_eq!(
        recipe_payload["id"],
        serde_json::json!(rid),
        "carries the resolved recipe id"
    );
    assert_eq!(recipe_payload["name"], "Outbox Recipe");
    assert!(
        recipe_payload.get("userId").is_none(),
        "userId omitted — the server stamps it"
    );
    assert_eq!(
        recipe_payload["asIngredientId"],
        serde_json::json!(local_ai),
        "outbox as-ingredient id == the local row's (so a push keeps it stable cross-replica)"
    );

    // saveIngredient payload carries its resolved id + the chosen visibility (serialized lowercase).
    let ing_payload = &rows[1].1;
    assert_eq!(ing_payload["id"], serde_json::json!(iid));
    assert_eq!(ing_payload["visibility"], "private");

    // deleteRecipe payload = { id } (drives DELETE /api/content/recipes?id=…).
    assert_eq!(rows[2].1["id"], serde_json::json!(rid));
    eprintln!("outbox FIFO {ops:?}; recipe payload's as-ingredient id matches the local row");
}

#[test]
fn logging_a_food_writes_locally_and_queues_the_push() {
    let db_path = std::env::temp_dir().join("vegify-diary-outbox.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    sign_in_seed(&db);
    let ops = |db: &Db| -> Vec<String> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT op FROM _outbox ORDER BY seq").unwrap();
        let v = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        v
    };

    let beans = VegifyData::search_ingredients(&db, "Black Beans".into())
        .expect("search")
        .into_iter()
        .find(|r| r.name == "Black Beans")
        .expect("seeded Black Beans")
        .id;

    // Log 150 g on a clean date (avoids the seed's own diary rows).
    let id = VegifyData::save_log_entry(
        &db,
        SaveLogEntryInput {
            id: None,
            ingredient_id: beans,
            date: "2020-01-01".into(),
            slot: None,
            grams: 150.0,
            unit: None,
            logged_at: None,
        },
    )
    .expect("log");

    // The write hit the LOCAL cache: the day reads back the entry with its frozen snapshot.
    let day = VegifyData::log_day(&db, "2020-01-01".into()).expect("day");
    assert_eq!(day.entries.len(), 1, "local write is readable offline");
    assert_eq!(day.entries[0].name, "Black Beans");
    assert!(
        day.calories.unwrap_or(0.0) > 0.0,
        "the snapshot froze calories locally"
    );
    // …and it queued the push (drained to POST /api/log/entries by the sync engine).
    assert!(
        ops(&db).contains(&"saveLogEntry".to_string()),
        "the diary write is queued for sync"
    );

    // Delete soft-deletes locally + queues the delete for sync.
    VegifyData::delete_log_entry(&db, id).expect("delete");
    assert!(
        VegifyData::log_day(&db, "2020-01-01".into())
            .unwrap()
            .entries
            .is_empty(),
        "the deleted entry leaves the day"
    );
    assert!(
        ops(&db).contains(&"deleteLogEntry".to_string()),
        "the delete is queued for sync"
    );
}

#[test]
fn saving_a_profile_writes_locally_and_queues_the_push() {
    let db_path = std::env::temp_dir().join("vegify-profile-outbox.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    sign_in_seed(&db);

    // Fresh profile reads as all-null defaults (generic-adult targets).
    let before = VegifyData::get_nutrition_profile(&db).expect("get");
    assert!(
        before.birth_year.is_none() && before.dri_sex.is_none() && !before.pregnancy,
        "an unset profile defaults to all-null"
    );

    VegifyData::save_nutrition_profile(
        &db,
        NutritionProfile {
            birth_year: Some(1990),
            dri_sex: Some(DriSex::Male),
            weight_kg: Some(80.0),
            pregnancy: false,
            lactation: false,
        },
    )
    .expect("save profile");

    // The write hit the LOCAL cache: it reads back offline with the saved values.
    let after = VegifyData::get_nutrition_profile(&db).expect("get");
    assert_eq!(after.birth_year, Some(1990));
    assert_eq!(after.dri_sex, Some(DriSex::Male));
    assert_eq!(after.weight_kg, Some(80.0));

    // …and it queued the push (drained to POST /api/profile by the sync engine). The payload is the
    // camelCase wire body the server upserts — no userId (the server stamps it from the session).
    let queued: String = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT payload FROM _outbox WHERE op = 'saveProfile' ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("a saveProfile op is queued");
    let payload: serde_json::Value = serde_json::from_str(&queued).unwrap();
    assert_eq!(payload["birthYear"], serde_json::json!(1990));
    assert_eq!(payload["driSex"], "male");
    assert!(
        payload.get("userId").is_none(),
        "userId omitted — the server stamps it from the session"
    );
}

// Step 4: the content-API HTTP client against a running web shell (real network + Bearer auth).
// #[ignore]'d so the default suite needs no server. Run with the web build served on $VEGIFY_AUTH_URL:
//   VEGIFY_AUTH_URL=http://localhost:39008 \
//     cargo test --lib content_client_round_trip -- --ignored --nocapture
#[test]
#[ignore]
fn content_client_round_trip_against_web() {
    let db_path = std::env::temp_dir().join("vegify-client.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");
    let _ = VegifyData::sign_out(&db); // clean slot
    VegifyData::sign_in(
        &db,
        SignInInput {
            email: "dev@example.com".into(),
            password: "dev-password".into(),
        },
    )
    .expect("sign in");
    let token = db.current_token().expect("token after sign in");

    // pull: the seed world comes back in mutation shape (recipes carry their as-ingredient id).
    let p = client().content_pull(Some(&token)).expect("pull");
    eprintln!(
        "pull: {} recipes, {} ingredients",
        p.recipes.len(),
        p.ingredients.len()
    );
    assert!(
        !p.recipes.is_empty() && !p.ingredients.is_empty(),
        "seed content pulled"
    );
    assert!(
        p.recipes.iter().all(|r| !r.asIngredientId.is_empty()),
        "recipes carry as-ingredient id"
    );

    // post a recipe via the client → it appears in a fresh pull.
    let rid = new_id();
    let body = serde_json::json!({
        "id": rid, "asIngredientId": new_id(), "name": "Client Posted Loaf", "visibility": "public",
        "servingGrams": 100.0, "batchGrams": 200.0, "items": []
    });
    client()
        .content_post(&token, "recipes", &body)
        .expect("post recipe");
    let p2 = client().content_pull(Some(&token)).expect("pull2");
    assert!(
        p2.recipes
            .iter()
            .any(|r| r.id == rid && r.name == "Client Posted Loaf"),
        "posted recipe appears in the pull"
    );

    // delete via the client → gone from the next pull.
    client()
        .content_delete(&token, "recipes", &rid)
        .expect("delete recipe");
    let p3 = client().content_pull(Some(&token)).expect("pull3");
    assert!(
        !p3.recipes.iter().any(|r| r.id == rid),
        "deleted recipe is gone"
    );
    VegifyData::sign_out(&db).ok();
    eprintln!("content client round-trip OK: pull → post → pull → delete → pull");
}

// Step 5/9: the full sync engine across TWO replicas of one account, against a running web shell.
// A writes a nested Biga/Dough and syncs (push); B syncs (pull) and must converge to A's content
// with the nested item FK intact — the end-to-end proof the whole arc was built for. #[ignore]'d
// (needs the bun web build on $VEGIFY_AUTH_URL):
//   VEGIFY_AUTH_URL=http://localhost:39008 \
//     cargo test --lib two_replica_round_trip -- --ignored --nocapture
#[test]
#[ignore]
fn two_replica_round_trip_against_web() {
    let tmp = std::env::temp_dir();
    let mk = |tag: &str| {
        let db = tmp.join(format!("vegify-2rep-{tag}.db"));
        let _ = fs::remove_file(&db);
        fs::copy(crate::db_path(), &db).expect("seed");
        Db::open(db.to_str().unwrap()).expect("open")
    };
    let a = mk("A");
    let b = mk("B");
    for dev in [&a, &b] {
        let _ = VegifyData::sign_out(dev);
        VegifyData::sign_in(
            dev,
            SignInInput {
                email: "dev@example.com".into(),
                password: "dev-password".into(),
            },
        )
        .expect("sign in");
    }

    // A creates a nested pair: a Biga, and a Dough that consumes the Biga's as-ingredient as an
    // item. Names carry a unique run tag so repeated runs against one served copy don't collide.
    let uniq = &new_id()[..10];
    let biga_name = format!("2rep Biga {uniq}");
    let dough_name = format!("2rep Dough {uniq}");
    let biga_rid = VegifyData::save_recipe(
        &a,
        SaveRecipeInput {
            id: None,
            as_ingredient_id: None,
            visibility: Some(Visibility::Public),
            name: biga_name.clone(),
            subtitle: None,
            directions: None,
            serving_grams: Some(100.0),
            batch_grams: Some(300.0),
            items: vec![],
            slug: None,
        },
    )
    .expect("A saves biga");
    let biga_ai: String = a
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT as_ingredient_id FROM recipes WHERE id = ?1",
            [&biga_rid],
            |r| r.get(0),
        )
        .expect("biga as-ingredient");
    VegifyData::save_recipe(
        &a,
        SaveRecipeInput {
            id: None,
            as_ingredient_id: None,
            visibility: Some(Visibility::Public),
            name: dough_name.clone(),
            subtitle: None,
            directions: None,
            serving_grams: Some(250.0),
            batch_grams: Some(500.0),
            items: vec![RecipeItemInput {
                ingredient_id: biga_ai.clone(),
                grams: 300.0,
                unit: None,
                amount: None,
            }],
            slug: None,
        },
    )
    .expect("A saves dough consuming the biga");

    // A pushes to the server; B pulls and reconciles.
    a.sync_now().expect("A sync (push + pull)");
    b.sync_now().expect("B sync (push-empty + pull)");

    let names: Vec<String> = VegifyData::list_recipes(&b, Page::default())
        .expect("B list")
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert!(names.contains(&biga_name), "B converged on A's Biga");
    assert!(names.contains(&dough_name), "B converged on A's Dough");

    // the nested FK survived the cross-replica round-trip: B's Dough item resolves to A's Biga.
    let dough = VegifyData::list_recipes(&b, Page::default())
        .expect("list")
        .into_iter()
        .find(|c| c.name == dough_name)
        .expect("dough on B");
    let view = VegifyData::recipe(&b, dough.id)
        .expect("B recipe")
        .expect("exists");
    assert_eq!(
        view.items.len(),
        1,
        "B's Dough kept its item through push→pull"
    );
    assert_eq!(
        view.items[0].name, biga_name,
        "the item resolves to A's Biga as-ingredient — nested FK stable cross-replica"
    );
    eprintln!(
        "two-replica round-trip OK: A wrote nested Biga/Dough → pushed → B pulled + converged"
    );
}

// Regression for the email-collision the GUI sign-in surfaced (P5 identity reconciliation): a
// separately-seeded cache already holds this email under a DIFFERENT id (the dev seed's john vs
// the server's john). ensure_user_local must adopt the server id + re-point the stale user's
// content, NOT trip `UNIQUE users.email`.
#[test]
fn signin_reconciles_email_collision() {
    let db_path = std::env::temp_dir().join("vegify-reconcile.db");
    let _ = fs::remove_file(&db_path);
    fs::copy(crate::db_path(), &db_path).expect("seed");
    let db = Db::open(db_path.to_str().unwrap()).expect("open");

    let (seed_id, email): (String, String) = db
        .conn
        .lock()
        .unwrap()
        .query_row("SELECT id, email FROM users LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("a seed user");
    let owned_before: i64 = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM ingredients WHERE user_id = ?1",
            [&seed_id],
            |r| r.get(0),
        )
        .expect("count");
    assert!(owned_before > 0, "the seed user owns some content");

    // sign in as the SAME email under a different (server) id — must reconcile, not collide.
    let server_id = new_id();
    db.ensure_user_local(&AuthUser {
        id: server_id.clone(),
        name: "John".into(),
        username: "john".into(),
        email: email.clone(),
        email_verified: false,
    })
    .expect("reconcile, not UNIQUE users.email");

    let conn = db.conn.lock().unwrap();
    let id_for_email: String = conn
        .query_row("SELECT id FROM users WHERE email = ?1", [&email], |r| {
            r.get(0)
        })
        .expect("one user");
    assert_eq!(id_for_email, server_id, "the cache adopted the server id");
    let stale_rows: i64 = conn
        .query_row(
            "SELECT count(*) FROM users WHERE id = ?1",
            [&seed_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stale_rows, 0, "the stale seed user row is gone");
    let owned_after: i64 = conn
        .query_row(
            "SELECT count(*) FROM ingredients WHERE user_id = ?1",
            [&server_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        owned_after, owned_before,
        "the seed user's content now belongs to the server id"
    );
    eprintln!("reconcile OK: email collision resolved, content re-pointed {seed_id} → {server_id}");
}

/// Drift guard: Drizzle (`.data/vegify.db` via `pnpm db:push`) is the content-schema source of
/// truth. Assert the embedded `schema.sql` is a faithful SUPERSET — every (table, column) the dev
/// DB has, a fresh DB built from `schema.sql` also has — so vegify-core's mutations (proven against
/// the dev DB by the tests above) hold on a shipped fresh DB too. Fails if the Drizzle schema
/// changes without `schema.sql` being regenerated.
#[test]
fn schema_sql_matches_drizzle_dev_db() {
    use std::collections::BTreeSet;
    fn table_columns(conn: &Connection) -> BTreeSet<String> {
        let names: Vec<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' \
                 AND name NOT LIKE 'sqlite_%' AND name <> '_outbox'",
            )
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let mut cols = BTreeSet::new();
        for t in names {
            let mut q = conn
                .prepare(&format!("SELECT name FROM pragma_table_info('{t}')"))
                .unwrap();
            for c in q.query_map([], |r| r.get::<_, String>(0)).unwrap() {
                cols.insert(format!("{t}.{}", c.unwrap()));
            }
        }
        cols
    }

    // Truth = the Drizzle dev DB. Copy to a temp file so the test never mutates the shared one.
    let truth_path =
        std::env::temp_dir().join(format!("vegify-schema-truth-{}.db", std::process::id()));
    let _ = fs::remove_file(&truth_path);
    fs::copy(crate::db_path(), &truth_path).expect("dev DB present — run `pnpm db:push`");
    let truth_cols = table_columns(&Connection::open(&truth_path).unwrap());

    // Candidate = a fresh in-memory DB built ONLY from the embedded schema.sql.
    let fresh = Connection::open_in_memory().unwrap();
    ensure_content_schema(&fresh).unwrap();
    let fresh_cols = table_columns(&fresh);

    let missing: Vec<_> = truth_cols.difference(&fresh_cols).collect();
    assert!(
        missing.is_empty(),
        "schema.sql drifted from the Drizzle dev DB — regenerate it; missing: {missing:?}"
    );
    let _ = fs::remove_file(&truth_path);
    eprintln!("schema.sql covers all {} dev-DB columns", truth_cols.len());
}

/// Reproduces + fixes the v0.2.x "can't be opened / no such table" crash: a SHIPPED fresh install
/// opens a brand-new app-data DB that never saw `pnpm db:push`. `Db::open` must create the content
/// schema so the first pull's per-row `do_save_ingredient` (exactly what `apply_pull` replays)
/// succeeds instead of erroring "no such table: ingredients".
#[test]
fn fresh_db_accepts_content_writes_without_dev_seed() {
    let path = std::env::temp_dir().join(format!("vegify-fresh-{}.db", std::process::id()));
    let _ = fs::remove_file(&path);
    let db = Db::open(path.to_str().unwrap()).expect("open fresh app-data DB");
    let conn = db.conn.lock().unwrap();
    let input = SaveIngredientInput {
        id: None,
        visibility: Some(Visibility::Public),
        name: "Rolled Oats".into(),
        description: None,
        price: None,
        calories_per_100g: Some(389.0),
        serving_grams: Some(40.0),
        serving_unit: None,
        package_grams: Some(1000.0),
        nutrients: vec![IngredientNutrientInput {
            name: "Protein".into(),
            amount_per_100g: 16.9,
            unit: "g".into(),
        }],
        slug: None,
    };
    let id = do_save_ingredient(&conn, &input, None).expect("save into the freshly-created schema");
    let name: String = conn
        .query_row("SELECT name FROM ingredients WHERE id = ?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(name, "Rolled Oats");
    // The on-demand catalog/amount rows landed too (find_or_create_nutrient / upsert_amount).
    let nutrients: i64 = conn
        .query_row("SELECT count(*) FROM nutrients", [], |r| r.get(0))
        .unwrap();
    let amounts: i64 = conn
        .query_row("SELECT count(*) FROM amounts", [], |r| r.get(0))
        .unwrap();
    assert!(
        nutrients >= 1 && amounts >= 1,
        "nutrient + amount rows created on demand"
    );
    drop(conn);
    let _ = fs::remove_file(&path);
    eprintln!("fresh DB accepted a content write: ingredient {id}");
}

/// The logged-out cache regression behind "@user" breadcrumbs + "No one goes by that handle":
/// the pull now carries creators, apply_pull mirrors them into `users` (placeholder email = the
/// id), so creator handles and profiles resolve on-device without a session — while an
/// auth-owned row (real email) keeps its email and stale placeholders vanish on the next pull.
#[test]
fn apply_pull_users_resolve_creators_and_profiles() {
    let path = std::env::temp_dir().join(format!("vegify-pullusers-{}.db", std::process::id()));
    let _ = fs::remove_file(&path);
    let db = Db::open(path.to_str().unwrap()).expect("open fresh app-data DB");
    let mut conn = db.conn.lock().unwrap();
    // An auth-owned row, as ensure_user_local writes it on sign-in (real email).
    conn.execute(
        "INSERT INTO users(id, name, username, email) VALUES ('u-self', 'Old Name', 'self', 'self@example.com')",
        [],
    )
    .unwrap();

    let payload = vegify_client::PullPayload {
        recipes: vec![vegify_client::PullRecipe {
            id: "r1".into(),
            asIngredientId: "r1-as".into(),
            userId: Some("u-ada".into()),
            visibility: vegify_client::Visibility::public,
            name: "Ada's Porridge".into(),
            subtitle: None,
            directions: None,
            servingGrams: None,
            batchGrams: None,
            items: vec![],
            slug: None,
        }],
        ingredients: vec![vegify_client::PullIngredient {
            id: "i1".into(),
            userId: Some("u-ada".into()),
            visibility: vegify_client::Visibility::public,
            name: "Ada's Oats".into(),
            description: None,
            price: None,
            caloriesPer100g: Some(389.0),
            servingGrams: None,
            servingUnit: None,
            packageGrams: None,
            nutrients: vec![],
            slug: None,
            deletedAt: None,
        }],
        users: vec![
            vegify_client::PullUser {
                id: "u-ada".into(),
                username: "ada".into(),
                name: "Ada".into(),
                avatarKey: Some("media/ada.jpg".into()),
            },
            // The signed-in user also appears in a pull (they own content) — public fields
            // refresh, the auth-owned email must survive.
            vegify_client::PullUser {
                id: "u-self".into(),
                username: "self".into(),
                name: "New Name".into(),
                avatarKey: None,
            },
        ],
    };
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    apply_pull(&mut conn, &payload).expect("apply pull with users");

    // Creator handle resolves on the recipe view (the "@user" breadcrumb regression).
    let view = vegify_core::recipe(&conn, "r1".into(), None)
        .unwrap()
        .expect("pulled recipe readable");
    assert_eq!(view.creator.as_deref(), Some("ada"));

    // The creator's profile resolves logged-out, with both shelves + avatar.
    let profile = vegify_core::get_profile(&conn, "ada", None)
        .unwrap()
        .expect("creator profile resolves from the cache");
    assert_eq!(profile.name, "Ada");
    assert_eq!(profile.avatar_key.as_deref(), Some("media/ada.jpg"));
    assert_eq!(profile.recipes.len(), 1);
    assert_eq!(profile.ingredients.len(), 1);

    // Placeholder rows carry the synthetic id-email; auth-owned rows keep theirs (public
    // fields refreshed).
    let ada_email: String = conn
        .query_row("SELECT email FROM users WHERE id = 'u-ada'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(ada_email, "u-ada");
    let (self_email, self_name): (String, String) = conn
        .query_row(
            "SELECT email, name FROM users WHERE id = 'u-self'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(self_email, "self@example.com");
    assert_eq!(self_name, "New Name");

    // A later pull that no longer lists ada prunes the placeholder; the auth-owned row stays.
    let empty = vegify_client::PullPayload {
        recipes: vec![],
        ingredients: vec![],
        users: vec![],
    };
    apply_pull(&mut conn, &empty).expect("apply empty pull");
    let ada_left: i64 = conn
        .query_row("SELECT COUNT(*) FROM users WHERE id = 'u-ada'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(ada_left, 0, "stale placeholder pruned");
    let self_left: i64 = conn
        .query_row("SELECT COUNT(*) FROM users WHERE id = 'u-self'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(self_left, 1, "auth-owned row survives");

    drop(conn);
    let _ = fs::remove_file(&path);
}

/// Slug generation: kebab from name, per-scope uniqueness (global for leaf ingredients, per-user
/// for recipes), rename regenerates + logs the old slug to slug_history, and a pull-provided slug
/// is stored verbatim (server-authoritative).
#[test]
fn slugs_generate_dedup_and_record_rename_history() {
    let path = std::env::temp_dir().join(format!("vegify-slug-{}.db", std::process::id()));
    let _ = fs::remove_file(&path);
    let db = Db::open(path.to_str().unwrap()).expect("open");
    let conn = db.conn.lock().unwrap();

    let ing = |name: &str, slug: Option<&str>| SaveIngredientInput {
        id: None,
        visibility: Some(Visibility::Public),
        name: name.into(),
        description: None,
        price: None,
        calories_per_100g: None,
        serving_grams: None,
        serving_unit: None,
        package_grams: None,
        nutrients: vec![],
        slug: slug.map(str::to_string),
    };
    let slug_of = |conn: &Connection, id: &str| -> String {
        conn.query_row("SELECT slug FROM ingredients WHERE id=?1", [id], |r| {
            r.get::<_, Option<String>>(0)
        })
        .unwrap()
        .unwrap_or_default()
    };

    // Leaf ingredients: kebab + global dedup.
    let a = do_save_ingredient(&conn, &ing("Rolled Oats", None), None).unwrap();
    let b = do_save_ingredient(&conn, &ing("Rolled Oats", None), None).unwrap();
    assert_eq!(slug_of(&conn, &a), "rolled-oats");
    assert_eq!(
        slug_of(&conn, &b),
        "rolled-oats-2",
        "global collision gets a numeric suffix"
    );

    // A pull-provided slug is stored verbatim (no generation).
    let c =
        do_save_ingredient(&conn, &ing("Rolled Oats", Some("server-picked-slug")), None).unwrap();
    assert_eq!(slug_of(&conn, &c), "server-picked-slug");

    // Recipes: slug unique PER USER (same name under different users may coincide). Real user
    // rows first — ingredients.user_id has an FK and foreign_keys is ON.
    conn.execute(
        "INSERT INTO users(id, name, email, password_hash, username) VALUES
         ('user-1','U1','u1@example.com','h','u1'), ('user-2','U2','u2@example.com','h','u2')",
        [],
    )
    .unwrap();
    let mk_recipe = |name: &str| SaveRecipeInput {
        id: None,
        as_ingredient_id: None,
        visibility: Some(Visibility::Public),
        name: name.into(),
        subtitle: None,
        directions: None,
        serving_grams: None,
        batch_grams: None,
        items: vec![],
        slug: None,
    };
    let r_u1a = do_save_recipe(&conn, &mk_recipe("Biga"), Some("user-1")).unwrap();
    let r_u1b = do_save_recipe(&conn, &mk_recipe("Biga"), Some("user-1")).unwrap();
    let r_u2 = do_save_recipe(&conn, &mk_recipe("Biga"), Some("user-2")).unwrap();
    let recipe_slug = |rid: &str| -> String {
        let as_ing: String = conn
            .query_row(
                "SELECT as_ingredient_id FROM recipes WHERE id=?1",
                [rid],
                |r| r.get(0),
            )
            .unwrap();
        slug_of(&conn, &as_ing)
    };
    assert_eq!(recipe_slug(&r_u1a), "biga");
    assert_eq!(
        recipe_slug(&r_u1b),
        "biga-2",
        "same user, same name → suffix"
    );
    assert_eq!(
        recipe_slug(&r_u2),
        "biga",
        "different user reuses the slug (per-user scope)"
    );

    // Rename a recipe → new slug, old one logged to slug_history for a 301.
    let as_ing_u1a: String = conn
        .query_row(
            "SELECT as_ingredient_id FROM recipes WHERE id=?1",
            [&r_u1a],
            |r| r.get(0),
        )
        .unwrap();
    do_save_recipe(
        &conn,
        &SaveRecipeInput {
            id: Some(r_u1a.clone()),
            as_ingredient_id: Some(as_ing_u1a),
            name: "Poolish".into(),
            ..mk_recipe("Poolish")
        },
        Some("user-1"),
    )
    .unwrap();
    assert_eq!(
        recipe_slug(&r_u1a),
        "poolish",
        "rename regenerates the slug"
    );
    let history: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM slug_history WHERE scope='user-1' AND slug='biga'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(history, 1, "the old slug is recorded for redirects");

    drop(conn);
    let _ = fs::remove_file(&path);
}

/// The sitemap is crawler-facing: it must list ONLY public, slugged, indexable URLs — never a
/// private/unlisted row, and never a recipe whose owner has no handle (there'd be no canonical URL).
#[test]
fn public_sitemap_excludes_private_and_headless() {
    let path = std::env::temp_dir().join(format!("vegify-sm-{}.db", std::process::id()));
    let _ = fs::remove_file(&path);
    let db = Db::open(path.to_str().unwrap()).expect("open");
    let conn = db.conn.lock().unwrap();

    // A handled user + a headless one (NULL username → no canonical recipe URL).
    conn.execute(
        "INSERT INTO users(id, name, email, password_hash, username) VALUES
         ('u1','U1','u1@example.com','h','chef'), ('u0','U0','u0@example.com','h',NULL)",
        [],
    )
    .unwrap();

    let ing = |name: &str, vis: Visibility| SaveIngredientInput {
        id: None,
        visibility: Some(vis),
        name: name.into(),
        description: None,
        price: None,
        calories_per_100g: None,
        serving_grams: None,
        serving_unit: None,
        package_grams: None,
        nutrients: vec![],
        slug: None,
    };
    do_save_ingredient(&conn, &ing("Tofu", Visibility::Public), None).unwrap();
    do_save_ingredient(&conn, &ing("Secret Sauce", Visibility::Private), None).unwrap();
    // An OWNED leaf — canonical under its creator (/<username>/ingredients/<slug>).
    do_save_ingredient(&conn, &ing("Chef Paste", Visibility::Public), Some("u1")).unwrap();

    let rec = |name: &str, vis: Visibility| SaveRecipeInput {
        id: None,
        as_ingredient_id: None,
        visibility: Some(vis),
        name: name.into(),
        subtitle: None,
        directions: None,
        serving_grams: None,
        batch_grams: None,
        items: vec![],
        slug: None,
    };
    do_save_recipe(&conn, &rec("Public Stew", Visibility::Public), Some("u1")).unwrap();
    do_save_recipe(&conn, &rec("Private Stew", Visibility::Private), Some("u1")).unwrap();
    do_save_recipe(&conn, &rec("Headless Stew", Visibility::Public), Some("u0")).unwrap();

    let sm = vegify_core::public_sitemap(&conn).unwrap();
    let ings: Vec<&str> = sm.ingredients.iter().map(|i| i.slug.as_str()).collect();
    assert!(ings.contains(&"tofu"), "public leaf ingredient listed");
    assert!(
        !ings.contains(&"secret-sauce"),
        "private ingredient excluded"
    );
    assert!(
        sm.ingredients
            .iter()
            .any(|i| i.slug == "tofu" && i.username.is_none()),
        "unowned leaf = the catalog namespace (/ingredients/<slug>)"
    );
    assert!(
        sm.ingredients
            .iter()
            .any(|i| i.slug == "chef-paste" && i.username.as_deref() == Some("chef")),
        "owned leaf carries its owner handle (canonical /<username>/ingredients/<slug>)"
    );

    let recs: Vec<&str> = sm.recipes.iter().map(|r| r.slug.as_str()).collect();
    assert!(recs.contains(&"public-stew"), "public recipe listed");
    assert!(!recs.contains(&"private-stew"), "private recipe excluded");
    assert!(
        !recs.contains(&"headless-stew"),
        "recipe under a handle-less user excluded"
    );
    assert!(
        sm.recipes
            .iter()
            .any(|r| r.username == "chef" && r.slug == "public-stew"),
        "listed recipe carries its owner handle"
    );

    drop(conn);
    let _ = fs::remove_file(&path);
}
