//! Transactional email via Amazon SES v2 — password-reset links (A5). The client is built lazily on
//! first send from the default AWS credential chain (the EC2 instance role in prod; env/SSO locally),
//! pinned to us-east-1 (where the vegify.app identity is verified), so the server boots with no email
//! config and costs nothing until a reset is actually requested.

use std::env;

use aws_config::Region;
use aws_sdk_sesv2::types::{Body, Content, Destination, EmailContent, Message};
use aws_sdk_sesv2::Client;
use tokio::sync::OnceCell;

static CLIENT: OnceCell<Client> = OnceCell::const_new();

async fn client() -> &'static Client {
    CLIENT
        .get_or_init(|| async {
            let cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(Region::new("us-east-1"))
                .load()
                .await;
            Client::new(&cfg)
        })
        .await
}

fn from_address() -> String {
    env::var("VEGIFY_EMAIL_FROM").unwrap_or_else(|_| "Vegify <hello@vegify.app>".to_string())
}

fn reset_link(token: &str) -> String {
    let base = env::var("VEGIFY_PUBLIC_URL").unwrap_or_else(|_| "https://vegify.app".to_string());
    format!("{}/reset?token={}", base.trim_end_matches('/'), token)
}

fn verify_link(token: &str) -> String {
    let base = env::var("VEGIFY_PUBLIC_URL").unwrap_or_else(|_| "https://vegify.app".to_string());
    format!("{}/verify?token={}", base.trim_end_matches('/'), token)
}

/// Send the password-reset email. Best-effort by design: a failure is logged but NOT propagated, because
/// the request endpoint always returns 200 to avoid revealing whether an email is registered. Runs on
/// the async side, outside the blocking DB closure.
pub async fn send_password_reset(to: &str, name: &str, token: &str) {
    let link = reset_link(token);
    let text = format!(
        "Hi {name},\n\nSomeone requested a password reset for your Vegify account. Open the link below \
         within one hour to choose a new password:\n\n{link}\n\nIf you didn't request this, you can safely \
         ignore this email — your password won't change.\n\n— Vegify"
    );
    let html = format!(
        "<p>Hi {name},</p>\
         <p>Someone requested a password reset for your Vegify account. Use the button below within one \
         hour to choose a new password:</p>\
         <p><a href=\"{link}\" style=\"display:inline-block;padding:10px 16px;background:#16a34a;\
         color:#ffffff;border-radius:8px;text-decoration:none\">Reset your password</a></p>\
         <p>Or paste this link into your browser:<br><a href=\"{link}\">{link}</a></p>\
         <p>If you didn't request this, you can safely ignore this email — your password won't change.</p>\
         <p>— Vegify</p>"
    );
    match try_send(to, "Reset your Vegify password", &text, &html).await {
        Ok(()) => tracing::info!(to = %to, "password-reset email sent"),
        Err(e) => tracing::error!(to = %to, error = %e, "password-reset email send failed"),
    }
}

/// Send the email-verification email (on signup, and on an explicit resend). Best-effort by design, like
/// the reset send: a failure is logged but never propagated, so the request endpoint can always 200
/// without revealing whether an email is registered.
pub async fn send_email_verification(to: &str, name: &str, token: &str) {
    let link = verify_link(token);
    let text = format!(
        "Hi {name},\n\nWelcome to Vegify! Confirm your email address by opening the link below within \
         24 hours:\n\n{link}\n\nIf you didn't create a Vegify account, you can safely ignore this \
         email.\n\n— Vegify"
    );
    let html = format!(
        "<p>Hi {name},</p>\
         <p>Welcome to Vegify! Confirm your email address using the button below within 24 hours:</p>\
         <p><a href=\"{link}\" style=\"display:inline-block;padding:10px 16px;background:#16a34a;\
         color:#ffffff;border-radius:8px;text-decoration:none\">Confirm your email</a></p>\
         <p>Or paste this link into your browser:<br><a href=\"{link}\">{link}</a></p>\
         <p>If you didn't create a Vegify account, you can safely ignore this email.</p>\
         <p>— Vegify</p>"
    );
    match try_send(to, "Confirm your Vegify email", &text, &html).await {
        Ok(()) => tracing::info!(to = %to, "email-verification email sent"),
        Err(e) => tracing::error!(to = %to, error = %e, "email-verification email send failed"),
    }
}

async fn try_send(
    to: &str,
    subject: &str,
    text: &str,
    html: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let body = Body::builder()
        .text(Content::builder().data(text).charset("UTF-8").build()?)
        .html(Content::builder().data(html).charset("UTF-8").build()?)
        .build();
    let message = Message::builder()
        .subject(Content::builder().data(subject).charset("UTF-8").build()?)
        .body(body)
        .build();
    client()
        .await
        .send_email()
        .from_email_address(from_address())
        .destination(Destination::builder().to_addresses(to).build())
        .content(EmailContent::builder().simple(message).build())
        .send()
        .await?;
    Ok(())
}

#[cfg(test)]
mod live_tests {
    use super::*;

    // A REAL SES send — proves the production send path end to end. Ignored by default (needs AWS creds
    // + sends a real email). The recipient comes from the environment so no address is committed:
    //   VEGIFY_TEST_EMAIL=you@example.com cargo test -p vegify-server send_live -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn send_live() {
        let to = std::env::var("VEGIFY_TEST_EMAIL")
            .expect("set VEGIFY_TEST_EMAIL=you@example.com to run the live send test");
        let link = "https://vegify.app/reset?token=LIVE-TEST-TOKEN";
        try_send(
            &to,
            "Reset your Vegify password (live test)",
            &format!("Live test of the Vegify password-reset send path.\n\nLink: {link}"),
            &format!("<p>Live test of the Vegify password-reset send path.</p><p>Link: <a href=\"{link}\">reset</a></p>"),
        )
        .await
        .expect("SES send should succeed with the local credential chain");
    }
}
