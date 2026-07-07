//! vegify-config — the single home for every environment-sourced setting the Rust binaries read.
//! Each accessor reads the process environment on every call (no caching), matching the inline
//! `env::var` reads it replaced; the value of this crate is that every knob is defined, defaulted,
//! and documented exactly once. The JS twin is packages/config. These modules ARE the config
//! documentation — there is no .env file anywhere, by design; deployed values are injected by the
//! CDK (systemd/Lambda env) from the account's own SSM decisions.
//!
//! Defaults are DEV conventions only. Anything whose wrong value would silently point a deployment
//! at someone else's site (the public URL, the From address) has NO default and returns `Option` —
//! callers refuse the operation instead of falling back. Deploys set those explicitly (the CDK
//! writes them into the instance's systemd unit — infra/lib/server-stack.ts).

use std::env;

/// An env var, treating unset and empty/whitespace as absent.
fn non_empty(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(feature = "server")]
/// Server-side deploy decisions (region, domains, email, signing).
pub mod server {
    use super::non_empty;

    /// On-disk SQLite file the server opens. Deployed: /data/vegify.db (set by the systemd unit).
    pub fn database_path() -> String {
        non_empty("DATABASE_PATH").unwrap_or_else(|| "vegify.db".to_string())
    }

    /// Listen port.
    pub fn port() -> u16 {
        non_empty("PORT")
            .and_then(|p| p.parse().ok())
            .unwrap_or(8787)
    }

    /// Signups are disabled unless VEGIFY_SIGNUPS_OPEN=1 (invite-only by default; the server is the
    /// authority — SIGNUPS_ENABLED in @vegify/ui only mirrors this in the UI).
    pub fn signups_open() -> bool {
        non_empty("VEGIFY_SIGNUPS_OPEN").as_deref() == Some("1")
    }

    /// Region the SES identity is verified in (must match the VegifyEmail stack's region).
    pub fn ses_region() -> String {
        non_empty("VEGIFY_SES_REGION").unwrap_or_else(|| "us-east-1".to_string())
    }

    /// Public site origin used as the base of links in transactional email, normalized without a
    /// trailing slash. REQUIRED to send — deliberately no default: a fallback domain would silently
    /// mail reset/verify links pointing at someone else's site. None → the send is refused + logged.
    pub fn public_url() -> Option<String> {
        non_empty("VEGIFY_PUBLIC_URL").map(|u| u.trim_end_matches('/').to_string())
    }

    /// From: header for transactional mail (e.g. `Vegify <hello@example.com>`). REQUIRED to send,
    /// like [`public_url`] — no default, so a misconfigured deploy can't send as another domain.
    pub fn email_from() -> Option<String> {
        non_empty("VEGIFY_EMAIL_FROM")
    }

    /// S3 bucket holding reference DATA (the USDA catalog artifact) — set by the CDK from the
    /// server stack's Data bucket. None (dev, or a self-host that skipped the upload) → the boot
    /// ingest logs and serves without the catalog.
    pub fn data_bucket() -> Option<String> {
        non_empty("VEGIFY_DATA_BUCKET")
    }

    /// S3 bucket for USER MEDIA (recipe photos, avatars) — presigned-PUT uploads, served at the
    /// API's /media/* CloudFront behavior. None (dev without AWS) → uploads are refused with a
    /// clear message; everything else works.
    pub fn media_bucket() -> Option<String> {
        non_empty("VEGIFY_MEDIA_BUCKET")
    }

    /// Admin email allowlist (comma-separated, VEGIFY_ADMIN_EMAILS) — accounts allowed to INVITE new
    /// users while public signups stay closed (invite-only). Empty = no admins (the default). Emails
    /// are compared lowercased/trimmed, matching how they're stored.
    pub fn admin_emails() -> Vec<String> {
        non_empty("VEGIFY_ADMIN_EMAILS")
            .map(|v| {
                v.split(',')
                    .map(|e| e.trim().to_lowercase())
                    .filter(|e| !e.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(feature = "desktop")]
/// Desktop build decisions (bundle ids, update endpoints).
pub mod desktop {
    use super::non_empty;

    /// Base URL of the standing backend that owns auth + the content API. Runtime VEGIFY_AUTH_URL
    /// wins (dev → a local `bun serve-bun.mjs` or a local `vegify-server`); otherwise the release
    /// backend is baked at BUILD time from VEGIFY_API_URL (CI sets it on the desktop build) so the
    /// open-source tree carries no deployment host; an inert placeholder stands in when unset.
    pub fn server_url() -> String {
        non_empty("VEGIFY_AUTH_URL").unwrap_or_else(|| {
            option_env!("VEGIFY_API_URL")
                .unwrap_or("https://api.example.com")
                .to_string()
        })
    }

    /// Path to the on-device SQLite DB. `DATABASE_PATH` overrides. Debug builds use the repo's
    /// seeded .data/vegify.db (sample content without a running server); release (shipped) builds
    /// use the per-user OS app-data dir — `<data_dir>/app.vegify.desktop/vegify.db`, matching
    /// Tauri's app_data_dir.
    pub fn db_path() -> String {
        if let Some(p) = non_empty("DATABASE_PATH") {
            return p;
        }
        #[cfg(debug_assertions)]
        {
            // This crate lives at crates/vegify-config, two levels below the repo root. (The import
            // lives here too: release builds compile this block out, and a module-level `use` would
            // be an unused-import warning = a broken build under -Dwarnings.)
            use std::path::PathBuf;
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../.data/vegify.db")
                .to_string_lossy()
                .into_owned()
        }
        #[cfg(not(debug_assertions))]
        {
            dirs::data_dir()
                .expect("resolve OS data dir")
                .join("app.vegify.desktop")
                .join("vegify.db")
                .to_string_lossy()
                .into_owned()
        }
    }
}
