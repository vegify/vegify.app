//! The /api/content/pull payload assembly — the Rust port of content.ts's `pullContent`. Returns the
//! viewer's listed world (public + own) in @vegify/db MUTATION shape (ids + each row's owner + items /
//! nutrients), which the desktop applies via vegify-core's do_save_*. Leaf ingredients only — recipe
//! as-ingredients are excluded (the recipe apply recreates them). The single-row reads + the lists go
//! through vegify-core directly; only this bulk mutation-shape assembly is server-specific.

use rusqlite::{params, Connection};

use crate::error::AppError;
pub use vegify_api_types::{
    PullIngredient, PullItem, PullPayload, PullReading, PullRecipe, PullUser,
};

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
        let rows = stmt
            .query_map(params![viewer], |row| {
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
        let rows = stmt
            .query_map(params![viewer], |row| {
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
    // The creators of everything listed above (public identity only). Recipe owners are covered by
    // the ingredients scan — a recipe's owner IS its as-ingredient's user_id — so one ownership scan
    // over `ingredients` (no leaf filter, tombstones included) collects every creator in the payload.
    // Users without a username can't have a profile URL; they're omitted and their rows render
    // creatorless, matching what the web serves.
    let users: Vec<PullUser> = {
        let mut stmt = conn.prepare(
            "SELECT u.id, u.username, u.name, u.avatar_key
             FROM users u
             WHERE u.username IS NOT NULL
               AND u.id IN (SELECT i.user_id FROM ingredients i
                            WHERE i.user_id IS NOT NULL
                              AND (i.visibility = 'public' OR i.user_id = ?1))
             ORDER BY u.id",
        )?;
        let rows = stmt
            .query_map(params![viewer], |row| {
                Ok(PullUser {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    name: row.get(2)?,
                    avatar_key: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    Ok(PullPayload {
        recipes,
        ingredients,
        users,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod tests {
    use super::*;
    use vegify_core::{
        do_save_ingredient, do_save_recipe, IngredientNutrientInput, RecipeItemInput,
        SaveIngredientInput, SaveRecipeInput, Visibility,
    };

    /// The REAL client schema (same fixture trick as usda.rs — the desktop's drift test pins it to
    /// the drizzle source), plus `users.username`, which postdates the drizzle base the same way it
    /// does on the live DB.
    const CLIENT_SCHEMA: &str = include_str!("../../../../apps/desktop/src-tauri/schema.sql");

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CLIENT_SCHEMA).unwrap();
        conn.execute_batch("ALTER TABLE users ADD COLUMN username TEXT;")
            .unwrap();
        conn
    }

    fn save_ingredient(conn: &Connection, name: &str, vis: Visibility, owner: Option<&str>) {
        let input = SaveIngredientInput {
            id: None,
            visibility: Some(vis),
            name: name.into(),
            description: None,
            price: None,
            calories_per_100g: Some(100.0),
            serving_grams: None,
            package_grams: None,
            nutrients: vec![IngredientNutrientInput {
                name: "Protein".into(),
                amount_per_100g: 1.0,
                unit: "g".into(),
            }],
            slug: None,
        };
        do_save_ingredient(conn, &input, owner).unwrap();
    }

    /// Anonymous and signed-in pulls carry exactly the creators of the content they list:
    /// public-content owners for everyone, plus the viewer's own private world for themselves;
    /// handle-less users are never shipped.
    #[test]
    fn pull_users_mirror_the_listed_worlds_creators() {
        let conn = test_conn();
        conn.execute_batch(
            "INSERT INTO users (id, name, email, username, avatar_key) VALUES
               ('u1', 'Ada', 'ada@x', 'ada', 'media/ada.jpg'),
               ('u2', 'Grace', 'grace@x', 'grace', NULL),
               ('u3', 'NoHandle', 'nohandle@x', NULL, NULL);",
        )
        .unwrap();
        // u1: public leaf ingredient + a public recipe (owner reached via its as-ingredient).
        save_ingredient(&conn, "Ada's Oats", Visibility::Public, Some("u1"));
        do_save_recipe(
            &conn,
            &SaveRecipeInput {
                id: None,
                as_ingredient_id: None,
                visibility: Some(Visibility::Public),
                name: "Ada's Porridge".into(),
                subtitle: None,
                directions: None,
                serving_grams: None,
                batch_grams: None,
                items: Vec::<RecipeItemInput>::new(),
                slug: None,
            },
            Some("u1"),
        )
        .unwrap();
        // u2: private-only content — invisible to anonymous viewers.
        save_ingredient(&conn, "Grace's Secret", Visibility::Private, Some("u2"));
        // u3: public content but no username — never shipped.
        save_ingredient(&conn, "Orphan Beans", Visibility::Public, Some("u3"));

        let anon = pull(&conn, None).unwrap();
        let anon_ids: Vec<&str> = anon.users.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(anon_ids, ["u1"], "anon sees only public-content creators");
        assert_eq!(anon.users[0].username, "ada");
        assert_eq!(anon.users[0].avatar_key.as_deref(), Some("media/ada.jpg"));

        let grace = pull(&conn, Some("u2")).unwrap();
        let grace_ids: Vec<&str> = grace.users.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(
            grace_ids,
            ["u1", "u2"],
            "a signed-in viewer additionally gets themselves (their own world is in the pull)"
        );
    }
}
