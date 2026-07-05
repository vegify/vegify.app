//! The blog as DATA, not code. Posts live in the `posts` table (server-owned — the blog is web-only,
//! so it's deliberately NOT in the shared vegify-core/desktop schema) and are served to the web over
//! HTTP, exactly like recipes. Authoring a post is a DB write, so it never bumps the app version or
//! fires the deploy cascade (the reason we moved off in-code BLOG_POSTS). Body is an opaque JSON block
//! list: the server stores + serves it verbatim; the web owns the block schema (prose / figure).
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::Value;
pub use vegify_api_types::{PostSummary, PostFull};

/// The bundled migration seed (the two originally-in-code posts). One-time: `seed_if_empty` inserts it
/// only into an empty table, so it never clobbers posts authored later.
const SEED_JSON: &str = include_str!("blog_seed.json");

#[derive(Deserialize)]
struct SeedPost {
    slug: String,
    title: String,
    description: String,
    #[serde(rename = "datePublished")]
    date_published: String,
    #[serde(rename = "dateDisplay")]
    date_display: String,
    body: Value,
}



/// Migrate the bundled posts into an EMPTY table (idempotent: no-op once any post exists).
pub fn seed_if_empty(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM posts", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(());
    }
    let seed: Vec<SeedPost> = serde_json::from_str(SEED_JSON)?;
    for p in seed {
        let body = serde_json::to_string(&p.body)?;
        conn.execute(
            "INSERT INTO posts
               (id, slug, title, description, published_at, date_display, body, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'published', strftime('%s','now'), strftime('%s','now'))",
            params![
                vegify_core::new_id(),
                p.slug,
                p.title,
                p.description,
                p.date_published,
                p.date_display,
                body
            ],
        )?;
    }
    Ok(())
}

/// Published posts, newest first — the index.
pub fn list_posts(conn: &Connection) -> rusqlite::Result<Vec<PostSummary>> {
    let mut stmt = conn.prepare(
        "SELECT slug, title, description, published_at, date_display FROM posts
         WHERE status = 'published' ORDER BY published_at DESC, created_at DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(PostSummary {
                slug: r.get(0)?,
                title: r.get(1)?,
                description: r.get(2)?,
                date_published: r.get(3)?,
                date_display: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// One published post by slug (None → 404), with its parsed block body.
pub fn get_post(conn: &Connection, slug: &str) -> rusqlite::Result<Option<PostFull>> {
    let row = conn
        .query_row(
            "SELECT slug, title, description, published_at, date_display, body FROM posts
             WHERE slug = ?1 AND status = 'published'",
            [slug],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    Ok(row.map(|(slug, title, description, date_published, date_display, body)| PostFull {
        slug,
        title,
        description,
        date_published,
        date_display,
        body: serde_json::from_str(&body).unwrap_or_else(|_| Value::Array(vec![])),
    }))
}
