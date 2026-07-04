//! Transactional email via Amazon SES v2 — password-reset links (A5). The client is built lazily on
//! first send from the default AWS credential chain (the EC2 instance role in prod; env/SSO locally), in
//! vegify_config::server::ses_region(), so the server boots with no email config and costs nothing until
//! a reset is actually requested.
//!
//! FAIL CLOSED on missing link-base/From config: VEGIFY_PUBLIC_URL and VEGIFY_EMAIL_FROM have NO
//! fallback — a default domain here would silently mail reset/verify links that point at someone
//! else's site (exactly what a misconfigured self-host would do). A send with either unset is refused
//! and logged; deploys set both explicitly (the CDK writes them into the systemd unit).

use aws_config::Region;
use aws_sdk_sesv2::types::{Body, Content, Destination, EmailContent, Message};
use aws_sdk_sesv2::Client;
use tokio::sync::OnceCell;
use vegify_config::server as config;

static CLIENT: OnceCell<Client> = OnceCell::const_new();

async fn client() -> &'static Client {
    CLIENT
        .get_or_init(|| async {
            let cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(Region::new(config::ses_region()))
                .load()
                .await;
            Client::new(&cfg)
        })
        .await
}

fn reset_link(token: &str) -> Option<String> {
    config::public_url().map(|base| format!("{base}/reset?token={token}"))
}

fn verify_link(token: &str) -> Option<String> {
    config::public_url().map(|base| format!("{base}/verify?token={token}"))
}

/// Send the password-reset email. Best-effort by design: a failure is logged but NOT propagated, because
/// the request endpoint always returns 200 to avoid revealing whether an email is registered. Runs on
/// the async side, outside the blocking DB closure.
pub async fn send_password_reset(to: &str, name: &str, token: &str) {
    let Some(link) = reset_link(token) else {
        tracing::error!("VEGIFY_PUBLIC_URL is not set; refusing to send a password-reset email (links would point at the wrong site)");
        return;
    };
    let text = format!(
        "Hi {name},\n\nSomeone asked to reset your Vegify password. Open this link within one hour \
         to choose a new one:\n\n{link}\n\nIf you didn't request this, ignore this email; your \
         password won't change.\n\nThe Vegify team"
    );
    let html = format!(
        "<p>Hi {name},</p>\
         <p>Someone asked to reset your Vegify password. Use the button below within one hour to \
         choose a new one:</p>\
         <p><a href=\"{link}\" style=\"display:inline-block;padding:10px 16px;background:#16a34a;\
         color:#ffffff;border-radius:8px;text-decoration:none\">Reset your password</a></p>\
         <p>Or paste this link into your browser:<br><a href=\"{link}\">{link}</a></p>\
         <p>If you didn't request this, ignore this email; your password won't change.</p>\
         <p>The Vegify team</p>"
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
    let Some(link) = verify_link(token) else {
        tracing::error!("VEGIFY_PUBLIC_URL is not set; refusing to send an email-verification email (links would point at the wrong site)");
        return;
    };
    let text = format!(
        "Hi {name},\n\nWelcome to Vegify! One click to confirm your email, and you're set:\n\n{link}\n\n\
         Then go see exactly what your plants are feeding you. (The link works for 24 hours.)\n\n\
         If you didn't create a Vegify account, ignore this email and nothing happens.\n\n\
         Happy cooking,\nThe Vegify team"
    );
    let html = format!(
        "<p>Hi {name},</p>\
         <p>Welcome to Vegify! One click to confirm your email, and you're set:</p>\
         <p><a href=\"{link}\" style=\"display:inline-block;padding:10px 16px;background:#16a34a;\
         color:#ffffff;border-radius:8px;text-decoration:none\">Confirm your email</a></p>\
         <p>Or paste this link into your browser:<br><a href=\"{link}\">{link}</a></p>\
         <p>Then go see exactly what your plants are feeding you. (The link works for 24 hours.)</p>\
         <p>If you didn't create a Vegify account, ignore this email and nothing happens.</p>\
         <p>Happy cooking,<br>The Vegify team</p>"
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
    let from = config::email_from()
        .ok_or("VEGIFY_EMAIL_FROM is not set; refusing to send email as an unconfigured sender")?;
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
        .from_email_address(from)
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
    // + sends a real email). The recipient AND sender come from the environment so no address is
    // committed (try_send fails closed without a configured From):
    //   VEGIFY_TEST_EMAIL=you@example.com VEGIFY_EMAIL_FROM='You <hello@your.domain>' \
    //     cargo test -p vegify-server send_live -- --ignored --nocapture
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
