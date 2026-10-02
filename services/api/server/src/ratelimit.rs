//! In-process rate limiting for the auth surface. The server is a SINGLE instance (t4g.nano) by
//! design, so in-memory fixed windows are the whole story — no Redis, no shared state, and a restart
//! forgiving all counters is acceptable. Two kinds of key protect two different things: per-IP
//! budgets throttle whoever is talking to us right now, and per-identifier budgets (email) survive
//! IP rotation — the guard that still works when web logins aggregate behind the SSR Lambda's
//! egress addresses or an attacker sprays a botnet.
//!
//! Enforcement lives in the handlers (not a tower layer) because the interesting keys are in the
//! request BODY (the login email), which middleware can't see before deserialization.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use subtle::ConstantTimeEq;

/// A named budget: at most `max` hits per fixed `window`. `name` namespaces the key so the same
/// string ("1.2.3.4", an email) can carry independent budgets for different endpoints.
#[derive(Clone, Copy)]
pub struct Limit {
    pub name: &'static str,
    pub max: u32,
    pub window: Duration,
}

/// Login attempts per client IP — generous because the web's logins arrive via a handful of Lambda
/// egress IPs; the per-identifier lockout below is the real credential-stuffing guard.
pub const LOGIN_IP: Limit = Limit {
    name: "login-ip",
    max: 20,
    window: Duration::from_secs(60),
};
/// FAILED logins per account identifier (only failures count; a success clears the bucket). This is
/// IP-independent on purpose: it holds against distributed stuffing and through the SSR aggregation.
pub const LOGIN_FAILS: Limit = Limit {
    name: "login-fails",
    max: 5,
    window: Duration::from_secs(15 * 60),
};
/// Bootstrap (claiming an invited, passwordless account) per IP — a small guessing surface.
pub const BOOTSTRAP_IP: Limit = Limit {
    name: "bootstrap-ip",
    max: 5,
    window: Duration::from_secs(60 * 60),
};
/// Account creation per IP (only reachable while the signups-open decision allows it).
pub const SIGNUP_IP: Limit = Limit {
    name: "signup-ip",
    max: 5,
    window: Duration::from_secs(60 * 60),
};
/// Outbound-email endpoints (reset + verification requests) per IP…
pub const EMAIL_SEND_IP: Limit = Limit {
    name: "email-ip",
    max: 10,
    window: Duration::from_secs(60 * 60),
};
/// …and per requested address — this one protects SES reputation and the inbox being spammed, and
/// it fires identically for registered and unregistered addresses (no enumeration signal).
pub const EMAIL_SEND_ID: Limit = Limit {
    name: "email-id",
    max: 3,
    window: Duration::from_secs(60 * 60),
};
/// Token-consuming endpoints (reset/verification confirm) per IP — token-guessing budget.
pub const TOKEN_CONFIRM_IP: Limit = Limit {
    name: "confirm-ip",
    max: 10,
    window: Duration::from_secs(60),
};
/// Branded-food lookups per IP. This one protects a resource the others don't: our OUTBOUND quota at
/// USDA FoodData Central and our standing as a polite Open Food Facts consumer. Only a cache MISS
/// spends a third-party request, so a real user re-scanning the same shelf costs nothing — 60/hour is
/// far past honest grocery-logging behaviour and well short of anything that would burn the key's
/// hourly allowance (and DEMO_KEY's is only ~30/hour, which is exactly why this budget exists).
pub const BRANDED_LOOKUP_IP: Limit = Limit {
    name: "branded-ip",
    max: 60,
    window: Duration::from_secs(60 * 60),
};
/// A GENERAL per-IP budget across EVERY endpoint (applied as middleware) — defense-in-depth beyond
/// the auth-specific limits above. It bounds any single source's load on the nano and slows naive
/// scrapers of the public read endpoints (content pull, search, profiles), while staying generous
/// enough that legit native-app traffic never brushes it (600/min = 10 req/s sustained per IP). SSR
/// reads count against each VISITOR's address (see `client_ip`), not the Lambda's shared egress
/// IPs. Distributed scraping is an edge-WAF concern, not this.
pub const GENERAL_IP: Limit = Limit {
    name: "general-ip",
    max: 600,
    window: Duration::from_secs(60),
};

/// Enforce a limit and, on rejection, emit a greppable `rate_limited` warn (the CloudWatch abuse
/// metric filter counts these) before returning the retry-after seconds. Use this at every
/// enforcement point so a rejection is both a 429 AND an abuse signal. The key is NOT logged — it can
/// be an email (PII); the limit name is enough for the alarm, and the request log carries the IP.
pub fn guard(rl: &RateLimiter, limit: Limit, key: &str) -> Result<(), u64> {
    rl.hit(limit, key).inspect_err(|&retry| {
        tracing::warn!(limit = limit.name, retry_after = retry, "rate_limited");
    })
}

struct Entry {
    window_start: Instant,
    /// The owning limit's window, carried per-entry so the sweep can evict exactly (the map mixes
    /// entries from limits with different windows).
    window: Duration,
    count: u32,
}

/// Above this many live buckets the next hit sweeps expired entries first — bounds memory against
/// IP spraying (50k entries ≈ a few MB, well inside the nano's budget).
const SWEEP_THRESHOLD: usize = 50_000;

#[derive(Default)]
pub struct RateLimiter {
    entries: Mutex<HashMap<(&'static str, String), Entry>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Count one event against `key` and gate it: `Err(retry_after_secs)` once the window is spent.
    /// Rejected calls do NOT extend the window (fixed windows, not sliding).
    pub fn hit(&self, limit: Limit, key: &str) -> Result<(), u64> {
        self.hit_at(limit, key, Instant::now())
    }

    /// Read-only probe: is `key` currently over budget (without counting an event)?
    pub fn over(&self, limit: Limit, key: &str) -> Option<u64> {
        self.over_at(limit, key, Instant::now())
    }

    /// The bucket map, recovering from a poisoned lock (plain counters;
    /// last-written state is valid).
    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<(&'static str, String), Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Forget `key`'s bucket — a successful login clears its failure count.
    pub fn clear(&self, limit: Limit, key: &str) {
        self.entries().remove(&(limit.name, key.to_string()));
    }

    fn hit_at(&self, limit: Limit, key: &str, now: Instant) -> Result<(), u64> {
        let mut map = self.entries();
        if map.len() >= SWEEP_THRESHOLD {
            map.retain(|_, e| now.duration_since(e.window_start) < e.window);
        }
        let entry = map.entry((limit.name, key.to_string())).or_insert(Entry {
            window_start: now,
            window: limit.window,
            count: 0,
        });
        if now.duration_since(entry.window_start) >= limit.window {
            entry.window_start = now;
            entry.count = 0;
        }
        if entry.count >= limit.max {
            return Err(retry_after(entry.window_start, limit.window, now));
        }
        entry.count += 1;
        Ok(())
    }

    fn over_at(&self, limit: Limit, key: &str, now: Instant) -> Option<u64> {
        let map = self.entries();
        let entry = map.get(&(limit.name, key.to_string()))?;
        if now.duration_since(entry.window_start) >= limit.window || entry.count < limit.max {
            return None;
        }
        Some(retry_after(entry.window_start, limit.window, now))
    }
}

fn retry_after(window_start: Instant, window: Duration, now: Instant) -> u64 {
    window
        .saturating_sub(now.duration_since(window_start))
        .as_secs()
        .max(1)
}

/// The visitor's address as the web SSR forwards it (from CloudFront-Viewer-Address).
pub const VIEWER_IP_HEADER: &str = "x-vegify-viewer-ip";
/// The origin-verify secret the SSR sends alongside, proving the forwarded address is its own.
pub const SSR_PROOF_HEADER: &str = "x-vegify-ssr-proof";

/// What a request must present to have its forwarded viewer address trusted: the origin-verify
/// secret the SSR Lambda also holds. Empty (unconfigured) trusts nothing.
#[derive(Clone, Default)]
pub struct SsrProof(Option<Arc<str>>);

impl SsrProof {
    pub fn new(secret: Option<String>) -> Self {
        Self(secret.map(Arc::from))
    }

    /// Whether a secret is configured (the server logs this at boot; never the value).
    pub fn is_configured(&self) -> bool {
        self.0.is_some()
    }
}

/// The client address for rate-limit keys. Every SSR call reaches us from the web Lambda's few
/// egress IPs, so keying those by connection address would put every logged-out visitor in one
/// budget — one fast crawler then 429s the site for everyone (it did, 2026-10-01). So a request
/// carrying the SSR proof is keyed by the visitor address it forwards; anything else falls back to
/// the RIGHTMOST `X-Forwarded-For` entry. The instance's port only admits CloudFront (the
/// prefix-list is the access control), so the last hop is always CloudFront and the rightmost entry
/// is the address CloudFront itself accepted the connection from — never a client-spoofable value
/// (anything a client sends in its own XFF sits further LEFT). Local dev has no CloudFront and no
/// XFF → one shared "local" bucket, which the generous limits tolerate.
pub fn client_ip(headers: &HeaderMap, proof: &SsrProof) -> String {
    if let Some(ip) = forwarded_viewer_ip(headers, proof) {
        return ip;
    }
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "local".to_string())
}

/// The forwarded viewer address, only when the request proves it came from the SSR (constant-time
/// comparison) and the address parses as an IP.
fn forwarded_viewer_ip(headers: &HeaderMap, proof: &SsrProof) -> Option<String> {
    let secret = proof.0.as_deref()?;
    let sent = headers.get(SSR_PROOF_HEADER)?.as_bytes();
    if !bool::from(sent.ct_eq(secret.as_bytes())) {
        return None;
    }
    let ip: IpAddr = headers
        .get(VIEWER_IP_HEADER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(ip.to_string())
}

pub struct ClientIp(pub String);

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
    SsrProof: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientIp(client_ip(
            &parts.headers,
            &SsrProof::from_ref(state),
        )))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, missing_docs)] // test code: unwrap IS the assertion
mod tests {
    use super::*;

    const TEST: Limit = Limit {
        name: "test",
        max: 3,
        window: Duration::from_secs(60),
    };

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().unwrap());
        }
        h
    }

    const XFF: (&str, &str) = ("x-forwarded-for", "6.6.6.6, 203.0.113.9");

    #[test]
    fn keys_by_the_address_cloudfront_saw_without_ssr_proof() {
        let proof = SsrProof::new(Some("s3cret".into()));
        assert_eq!(client_ip(&headers(&[XFF]), &proof), "203.0.113.9");
        // A forwarded viewer address alone (no proof, or a wrong one) is ignored.
        let spoof = headers(&[XFF, (VIEWER_IP_HEADER, "198.51.100.7")]);
        assert_eq!(client_ip(&spoof, &proof), "203.0.113.9");
        let wrong = headers(&[
            XFF,
            (VIEWER_IP_HEADER, "198.51.100.7"),
            (SSR_PROOF_HEADER, "guess"),
        ]);
        assert_eq!(client_ip(&wrong, &proof), "203.0.113.9");
    }

    #[test]
    fn keys_ssr_traffic_by_the_forwarded_visitor_when_the_proof_matches() {
        let proof = SsrProof::new(Some("s3cret".into()));
        let v4 = headers(&[
            XFF,
            (VIEWER_IP_HEADER, "198.51.100.7"),
            (SSR_PROOF_HEADER, "s3cret"),
        ]);
        assert_eq!(client_ip(&v4, &proof), "198.51.100.7");
        let v6 = headers(&[
            XFF,
            (VIEWER_IP_HEADER, "2001:db8::1"),
            (SSR_PROOF_HEADER, "s3cret"),
        ]);
        assert_eq!(client_ip(&v6, &proof), "2001:db8::1");
    }

    #[test]
    fn never_trusts_a_forwarded_value_that_is_not_an_ip() {
        let proof = SsrProof::new(Some("s3cret".into()));
        let junk = headers(&[
            XFF,
            (VIEWER_IP_HEADER, "not-an-ip"),
            (SSR_PROOF_HEADER, "s3cret"),
        ]);
        assert_eq!(client_ip(&junk, &proof), "203.0.113.9");
    }

    #[test]
    fn trusts_nothing_when_no_secret_is_configured() {
        let unset = SsrProof::new(None);
        let h = headers(&[
            XFF,
            (VIEWER_IP_HEADER, "198.51.100.7"),
            (SSR_PROOF_HEADER, ""),
        ]);
        assert_eq!(client_ip(&h, &unset), "203.0.113.9");
        assert_eq!(client_ip(&HeaderMap::new(), &unset), "local");
    }

    #[test]
    fn allows_up_to_max_then_rejects_with_retry_after() {
        let rl = RateLimiter::new();
        let t0 = Instant::now();
        for _ in 0..3 {
            assert!(rl.hit_at(TEST, "k", t0).is_ok());
        }
        let retry = rl
            .hit_at(TEST, "k", t0 + Duration::from_secs(10))
            .unwrap_err();
        assert_eq!(retry, 50, "retry-after counts down the remaining window");
    }

    #[test]
    fn window_resets_after_expiry() {
        let rl = RateLimiter::new();
        let t0 = Instant::now();
        for _ in 0..3 {
            rl.hit_at(TEST, "k", t0).unwrap();
        }
        assert!(rl.hit_at(TEST, "k", t0 + Duration::from_secs(59)).is_err());
        assert!(
            rl.hit_at(TEST, "k", t0 + Duration::from_secs(60)).is_ok(),
            "a fresh window opens"
        );
    }

    #[test]
    fn keys_and_limit_names_are_independent() {
        let rl = RateLimiter::new();
        let t0 = Instant::now();
        for _ in 0..3 {
            rl.hit_at(TEST, "a", t0).unwrap();
        }
        assert!(rl.hit_at(TEST, "b", t0).is_ok(), "other keys unaffected");
        const OTHER: Limit = Limit {
            name: "other",
            max: 3,
            window: Duration::from_secs(60),
        };
        assert!(
            rl.hit_at(OTHER, "a", t0).is_ok(),
            "same key under another limit name unaffected"
        );
    }

    #[test]
    fn over_probes_without_counting_and_clear_resets() {
        let rl = RateLimiter::new();
        let t0 = Instant::now();
        assert!(
            rl.over_at(TEST, "k", t0).is_none(),
            "unknown key is not over"
        );
        for _ in 0..3 {
            rl.hit_at(TEST, "k", t0).unwrap();
        }
        assert!(rl.over_at(TEST, "k", t0).is_some());
        assert!(
            rl.over_at(TEST, "k", t0 + Duration::from_secs(60))
                .is_none(),
            "expired window is not over"
        );
        rl.clear(TEST, "k");
        assert!(
            rl.hit_at(TEST, "k", t0).is_ok(),
            "clear() forgets the bucket"
        );
    }

    #[test]
    fn rejections_do_not_extend_the_window() {
        let rl = RateLimiter::new();
        let t0 = Instant::now();
        for _ in 0..3 {
            rl.hit_at(TEST, "k", t0).unwrap();
        }
        for i in 0..10 {
            let _ = rl.hit_at(TEST, "k", t0 + Duration::from_secs(i));
        }
        assert!(
            rl.hit_at(TEST, "k", t0 + Duration::from_secs(60)).is_ok(),
            "fixed window, not sliding"
        );
    }
}
