//! User media (recipe photos, avatars): the server PRESIGNS uploads — clients PUT straight to S3,
//! so image bytes never transit the t4g.nano — then records the attachment. Objects are served at
//! the API's own `/media/*` CloudFront behavior (immutable keys: a re-upload mints a new key, so
//! the edge can cache hard). Size discipline lives client-side (canvas re-encode before upload);
//! the content-type whitelist below is the server's own gate.
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppError;

/// Wire shape for an approved upload: the client PUTs the bytes to `url`, then attaches `key`.
pub use vegify_api_types::UploadTicket;

/// content-type → extension whitelist. Anything else is refused.
fn extension_for(content_type: &str) -> Option<&'static str> {
    match content_type {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

/// Mint a presigned PUT for one media object (any signed-in user; what it later ATTACHES to is
/// owner-gated separately). Key shape: `media/<ulid>.<ext>` — the `/media/*` CDN path IS the key.
pub async fn presign_upload(content_type: &str) -> Result<UploadTicket, AppError> {
    let Some(ext) = extension_for(content_type) else {
        return Err(AppError::BadRequest("Unsupported image type (jpeg/png/webp only).".into()));
    };
    let Some(bucket) = vegify_config::server::media_bucket() else {
        return Err(AppError::BadRequest("Uploads aren't configured on this server.".into()));
    };
    let key = format!("media/{}.{ext}", vegify_core::new_id().to_lowercase());
    let cfg = aws_config::defaults(aws_config::BehaviorVersion::latest()).load().await;
    let s3 = aws_sdk_s3::Client::new(&cfg);
    let presigned = s3
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .content_type(content_type)
        .presigned(
            aws_sdk_s3::presigning::PresigningConfig::expires_in(std::time::Duration::from_secs(900))
                .map_err(AppError::internal)?,
        )
        .await
        .map_err(AppError::internal)?;
    Ok(UploadTicket { key, url: presigned.uri().to_string() })
}

/// Attach an uploaded object as an ingredient's ONE photo (recipes attach via their as-ingredient —
/// a recipe IS an ingredient, so one mechanism covers both). Owner-gated; replaces any prior photo
/// row (the old object stays in the bucket — content-addressed keys make that harmless).
pub fn attach_photo(
    conn: &Connection,
    me_id: &str,
    ingredient_id: &str,
    key: &str,
    content_type: &str,
) -> Result<(), AppError> {
    if extension_for(content_type).is_none() || !key.starts_with("media/") {
        return Err(AppError::BadRequest("Invalid attachment.".into()));
    }
    let owner: Option<Option<String>> = conn
        .query_row("SELECT user_id FROM ingredients WHERE id = ?1", [ingredient_id], |r| r.get(0))
        .optional()
        .map_err(AppError::internal)?;
    let Some(owner) = owner else {
        return Err(AppError::BadRequest("No such ingredient.".into()));
    };
    if owner.as_deref() != Some(me_id) {
        return Err(AppError::Forbidden("You can only add photos to your own content.".into()));
    }
    // key = media/<stem>.<ext> → imgs stores the parts (uuid = stem, extension, content_type).
    let stem_ext = key.trim_start_matches("media/");
    let (stem, ext) = stem_ext.rsplit_once('.').unwrap_or((stem_ext, ""));
    let img_id = vegify_core::new_id();
    conn.execute(
        "INSERT INTO imgs (id, uuid, orig_name, extension, bucket, content_type) VALUES (?1, ?2, '', ?3, 'media', ?4)",
        params![img_id, stem, ext, content_type],
    )
    .map_err(AppError::internal)?;
    // ONE hero per ingredient: replace, don't accumulate.
    conn.execute("DELETE FROM ingredient_img WHERE ingredient_id = ?1", [ingredient_id])
        .map_err(AppError::internal)?;
    conn.execute(
        "INSERT INTO ingredient_img (id, img_id, ingredient_id) VALUES (?1, ?2, ?3)",
        params![vegify_core::new_id(), img_id, ingredient_id],
    )
    .map_err(AppError::internal)?;
    Ok(())
}

/// Set the signed-in user's avatar (their own row only).
pub fn attach_avatar(conn: &Connection, me_id: &str, key: &str, content_type: &str) -> Result<(), AppError> {
    if extension_for(content_type).is_none() || !key.starts_with("media/") {
        return Err(AppError::BadRequest("Invalid attachment.".into()));
    }
    conn.execute("UPDATE users SET avatar_key = ?1 WHERE id = ?2", params![key, me_id])
        .map_err(AppError::internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENT_SCHEMA: &str = include_str!("../../../../apps/desktop/src-tauri/schema.sql");

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(CLIENT_SCHEMA).unwrap();
        // username is a server-side ensure_schema addition; avatar_key is already in schema.sql.
        c.execute_batch("ALTER TABLE users ADD COLUMN username TEXT;").unwrap();
        c.execute("INSERT INTO users (id, name, email) VALUES ('u1','Ada','a@x'), ('u2','Bob','b@x')", [])
            .unwrap();
        c
    }

    fn leaf(c: &Connection, owner: &str) -> String {
        vegify_core::do_save_ingredient(
            c,
            &vegify_core::SaveIngredientInput {
                id: None,
                visibility: Some(vegify_core::Visibility::Public),
                name: "Paste".into(),
                description: None,
                price: None,
                calories_per_100g: None,
                serving_grams: None,
                package_grams: None,
                nutrients: vec![],
                slug: None,
            },
            Some(owner),
        )
        .unwrap()
    }

    #[test]
    fn attach_photo_is_owner_gated_and_replaces_the_hero() {
        let c = conn();
        let ing = leaf(&c, "u1");
        assert!(attach_photo(&c, "u2", &ing, "media/a.jpg", "image/jpeg").is_err(), "not the owner");
        attach_photo(&c, "u1", &ing, "media/a.jpg", "image/jpeg").unwrap();
        attach_photo(&c, "u1", &ing, "media/b.webp", "image/webp").unwrap();
        let (rows, uuid): (i64, String) = c
            .query_row(
                "SELECT COUNT(*), MAX(im.uuid) FROM ingredient_img ii JOIN imgs im ON im.id = ii.img_id
                 WHERE ii.ingredient_id = ?1",
                [&ing],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(rows, 1, "one hero, replaced not accumulated");
        assert_eq!(uuid, "b");
        assert!(attach_photo(&c, "u1", &ing, "media/x.gif", "image/gif").is_err(), "type whitelist");
    }

    #[test]
    fn avatar_sets_own_row() {
        let c = conn();
        attach_avatar(&c, "u1", "media/av.png", "image/png").unwrap();
        let k: Option<String> =
            c.query_row("SELECT avatar_key FROM users WHERE id='u1'", [], |r| r.get(0)).unwrap();
        assert_eq!(k.as_deref(), Some("media/av.png"));
    }
}
