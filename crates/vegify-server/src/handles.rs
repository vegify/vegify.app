//! Reserved handles + username validation.
//!
//! Public profiles and recipes will live at apex-namespaced URLs: `vegify.app/<username>/<recipe-slug>`.
//! Because `<username>` sits at the top level it competes with every app route (`/login`, `/recipes`,
//! `/api`, ...), and a handle, once claimed, can never be safely reclaimed. That makes the reserved set
//! below the load-bearing decision, so it is locked now (before usernames launch) rather than retrofitted.
//!
//! This is the AUTHORITATIVE copy. The web keeps no list of its own: a client-side "is this handle free?"
//! check has to reach the server regardless (uniqueness is server-only) via a future `check-handle`
//! endpoint, so there is nothing to mirror and nothing to drift.
//!
//! Not wired into a request path yet: signups are invite-only and `users` has no handle column. When
//! usernames launch, `signup` (and a future rename endpoint) call [`validate_username`] and then check
//! DB uniqueness on the normalized handle.

use std::collections::HashSet;
use std::sync::OnceLock;

/// Canonical username length bounds. Lower bound blocks single-character squatting; upper bound keeps
/// URLs and UI sane. Adjustable, but widening the lower bound later is safe while narrowing it is not.
pub const MIN_LEN: usize = 2;
pub const MAX_LEN: usize = 30;

/// Why a candidate username was rejected. Each message is safe to show the user verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleError {
    TooShort,
    TooLong,
    BadChars,
    EdgeHyphen,
    AllDigits,
    Reserved,
}

impl HandleError {
    pub fn message(self) -> &'static str {
        match self {
            HandleError::TooShort => "Username is too short.",
            HandleError::TooLong => "Username is too long.",
            HandleError::BadChars => "Use only lowercase letters, numbers, and hyphens.",
            HandleError::EdgeHyphen => "Hyphens can't start, end, or repeat in a username.",
            HandleError::AllDigits => "Usernames can't be all numbers.",
            HandleError::Reserved => "That username isn't available.",
        }
    }
}

/// Validate and normalize a candidate username. On success returns the canonical (lowercased) handle to
/// store and to build the URL from; on failure returns the reason. Case-insensitive: `"Simone"` and
/// `"simone"` normalize to the same handle. Does NOT check DB uniqueness; the caller does that against
/// the returned normalized value.
pub fn validate_username(input: &str) -> Result<String, HandleError> {
    let lower = input.trim().to_ascii_lowercase();
    let len = lower.chars().count();
    if len < MIN_LEN {
        return Err(HandleError::TooShort);
    }
    if len > MAX_LEN {
        return Err(HandleError::TooLong);
    }
    if !lower
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(HandleError::BadChars);
    }
    if lower.starts_with('-') || lower.ends_with('-') || lower.contains("--") {
        return Err(HandleError::EdgeHyphen);
    }
    if lower.chars().all(|c| c.is_ascii_digit()) {
        return Err(HandleError::AllDigits);
    }
    if is_reserved(&lower) {
        return Err(HandleError::Reserved);
    }
    Ok(lower)
}

/// Best-effort slug of arbitrary text (a display name, an email local-part) into a CANDIDATE handle:
/// lowercased, every run of disallowed characters collapsed to a single hyphen, edges trimmed, length
/// capped to `MAX_LEN`. The result can still be empty, too short, all-digits, or reserved — callers
/// run it through [`validate_username`] and fall back on rejection.
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    for c in input.trim().chars() {
        let lc = c.to_ascii_lowercase();
        if lc.is_ascii_lowercase() || lc.is_ascii_digit() {
            out.push(lc);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    // `out` is pure ASCII ([a-z0-9-]), so truncating at a byte index can't split a char.
    out.truncate(MAX_LEN);
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Whether a normalized (already lowercased) handle is reserved.
pub fn is_reserved(normalized: &str) -> bool {
    reserved().contains(normalized)
}

fn reserved() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| RESERVED.iter().copied().collect())
}

/// The reserved set. Entries are lowercase and matched case-insensitively (input is lowercased first).
/// Dotted static files (`robots.txt`, `sitemap.xml`) need no entry — a username can't contain a dot, so
/// it can never collide with them; the bare stems are reserved anyway for clarity. Grouped for review;
/// duplicates across groups are harmless (collapsed into a set).
const RESERVED: &[&str] = &[
    // --- Current app route segments (a username here would shadow a live route) ---
    "login",
    "logout",
    "signin",
    "sign-in",
    "signup",
    "sign-up",
    "register",
    "registration",
    "forgot",
    "reset",
    "verify",
    "verification",
    "confirm",
    "recipes",
    "recipe",
    "ingredients",
    "ingredient",
    "search",
    "new",
    "edit",
    "create",
    "delete",
    "api",
    "auth",
    "ws",
    "graphql",
    "rpc",
    "_serverfn",
    "__ingest",
    "assets",
    "static",
    "public",
    "media",
    "img",
    "image",
    "images",
    "files",
    "file",
    "uploads",
    "cdn",
    "robots",
    "sitemap",
    "favicon",
    "manifest",
    "well-known",
    // --- Planned app route segments (from the site map: meal plans, social, settings) ---
    "home",
    "dashboard",
    "profile",
    "profiles",
    "settings",
    "preferences",
    "account",
    "accounts",
    "user",
    "users",
    "me",
    "you",
    "my",
    "self",
    "meal-plan",
    "meal-plans",
    "mealplan",
    "mealplans",
    "plan",
    "plans",
    "planner",
    "planning",
    "calendar",
    "shopping",
    "shopping-list",
    "groceries",
    "grocery",
    "pantry",
    "nutrition",
    "feed",
    "explore",
    "discover",
    "browse",
    "trending",
    "popular",
    "featured",
    "saved",
    "bookmarks",
    "favorites",
    "favourites",
    "collection",
    "collections",
    "lists",
    "tags",
    "tag",
    "categories",
    "social",
    "notifications",
    "inbox",
    "messages",
    "message",
    "chat",
    "following",
    "followers",
    "follow",
    "friends",
    "invite",
    "invites",
    "invitation",
    "onboarding",
    "welcome",
    "share",
    // --- System / security / impersonation ---
    "admin",
    "admins",
    "administrator",
    "root",
    "superuser",
    "sysadmin",
    "system",
    "staff",
    "team",
    "official",
    "vegify-team",
    "mod",
    "mods",
    "moderator",
    "moderators",
    "support",
    "help",
    "helpdesk",
    "contact",
    "feedback",
    "abuse",
    "report",
    "security",
    "trust",
    "safety",
    "billing",
    "payment",
    "payments",
    "checkout",
    "subscribe",
    "subscription",
    "pricing",
    "upgrade",
    "oauth",
    "sso",
    "saml",
    "token",
    "tokens",
    "session",
    "sessions",
    "password",
    "passwords",
    "credentials",
    "secret",
    "secrets",
    "key",
    "keys",
    "webhook",
    "webhooks",
    "callback",
    // --- Brand / company / infra / email ---
    "vegify",
    "vegifyapp",
    "vegify-app",
    "app",
    "apps",
    "www",
    "web",
    "mobile",
    "ios",
    "android",
    "desktop",
    "mac",
    "macos",
    "windows",
    "linux",
    "download",
    "downloads",
    "install",
    "docs",
    "documentation",
    "api-docs",
    "developer",
    "developers",
    "dev",
    "developers",
    "blog",
    "news",
    "press",
    "media-kit",
    "brand",
    "logo",
    "about",
    "about-us",
    "company",
    "careers",
    "jobs",
    "legal",
    "terms",
    "tos",
    "privacy",
    "policy",
    "policies",
    "cookies",
    "cookie",
    "dmca",
    "gdpr",
    "ccpa",
    "license",
    "licenses",
    "licensing",
    "compliance",
    "status",
    "health",
    "healthz",
    "ping",
    "metrics",
    "debug",
    "mail",
    "email",
    "smtp",
    "imap",
    "pop",
    "ftp",
    "ssh",
    "ns",
    "ns1",
    "ns2",
    "mx",
    "dns",
    "noreply",
    "no-reply",
    "postmaster",
    "hostmaster",
    "webmaster",
    "abuse-team",
    // --- Generic placeholders / errors / squat bait ---
    "null",
    "undefined",
    "none",
    "nil",
    "true",
    "false",
    "nan",
    "void",
    "empty",
    "blank",
    "anonymous",
    "anon",
    "guest",
    "nobody",
    "everyone",
    "unknown",
    "deleted",
    "removed",
    "banned",
    "test",
    "testing",
    "tests",
    "demo",
    "demos",
    "example",
    "examples",
    "sample",
    "samples",
    "default",
    "placeholder",
    "temp",
    "tmp",
    "foo",
    "bar",
    "baz",
    "qux",
    "404",
    "403",
    "500",
    "error",
    "errors",
    "oops",
    "favicon-ico",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_handles() {
        for h in [
            "simone",
            "best-cook",
            "jordan-pierce",
            "x9",
            "vegan123",
            "a1",
            "the-biga-guy",
        ] {
            assert_eq!(
                validate_username(h),
                Ok(h.to_string()),
                "{h} should be valid"
            );
        }
    }

    #[test]
    fn slugify_makes_candidate_handles() {
        assert_eq!(slugify("John Carmack"), "john-carmack");
        assert_eq!(slugify("a.b_c!"), "a-b-c");
        assert_eq!(slugify("  Élodie  "), "lodie");
        assert_eq!(slugify("123"), "123"); // a valid slug; validate_username then rejects all-digits
        assert_eq!(slugify(""), "");
        assert_eq!(validate_username(&slugify("Best Cook")), Ok("best-cook".to_string()));
    }

    #[test]
    fn normalizes_case_and_whitespace() {
        assert_eq!(validate_username("Simone"), Ok("simone".to_string()));
        assert_eq!(
            validate_username("  Best-Cook  "),
            Ok("best-cook".to_string())
        );
    }

    #[test]
    fn rejects_length_violations() {
        assert_eq!(validate_username("a"), Err(HandleError::TooShort));
        assert_eq!(validate_username(""), Err(HandleError::TooShort));
        assert_eq!(
            validate_username(&"a".repeat(MAX_LEN + 1)),
            Err(HandleError::TooLong)
        );
        assert!(validate_username(&"a".repeat(MAX_LEN)).is_ok());
    }

    #[test]
    fn rejects_bad_chars() {
        for h in ["joe.smith", "joe_smith", "joe smith", "joe!", "joé", "JOE@"] {
            assert_eq!(
                validate_username(h),
                Err(HandleError::BadChars),
                "{h} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_edge_and_doubled_hyphens() {
        for h in ["-joe", "joe-", "jo--e"] {
            assert_eq!(
                validate_username(h),
                Err(HandleError::EdgeHyphen),
                "{h} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_all_digits() {
        assert_eq!(validate_username("12345"), Err(HandleError::AllDigits));
    }

    #[test]
    fn rejects_reserved_case_insensitively() {
        for h in [
            "login",
            "API",
            "Admin",
            "recipes",
            "vegify",
            "settings",
            "meal-plans",
            "well-known",
        ] {
            assert_eq!(
                validate_username(h),
                Err(HandleError::Reserved),
                "{h} should be reserved"
            );
        }
    }

    #[test]
    fn every_current_route_segment_is_reserved() {
        // Guard: the top-level segments the web app actually routes today MUST stay reserved, or a user
        // could claim a handle that shadows them. Update this list if a new top-level route is added.
        for seg in [
            "login",
            "signup",
            "forgot",
            "reset",
            "verify",
            "recipes",
            "ingredients",
            "search",
            "api",
        ] {
            assert!(
                is_reserved(seg),
                "route segment /{seg} must be reserved as a handle"
            );
        }
    }
}
