//! vegify-api-types — the server's WIRE CONTRACT as data: every serde shape the HTTP API speaks
//! (this crate) plus vegify-core's shared shapes (re-registered here), assembled into ONE specta
//! TypeCollection. Deliberately logic-free so consumers get the contract without the server:
//! vegify-typegen generates packages/api-types (TS) from it today; openapi.json (specta #31) and
//! vegify-client-rs type generation are the designated next consumers. The Rust twin of the
//! generated @vegify/api-types package.
use serde::Serialize;
use serde_json::Value;

/// Wire-facing JSON for opaque per-kind payloads (mirrors the UI's JsonValue). Exists because the
/// rc.25 specta line can't export `serde_json::Value` — its Number variant carries i64 and trips
/// the BigInt guard (fixed upstream by specta PR #505; when that lands, the `#[specta(type = …)]`
/// overrides pointing here can drop away). Never constructed — a type-level wire declaration only.
#[derive(Serialize, specta::Type)]
#[serde(untagged)]
pub enum JsonValue {
    Null(()),
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(std::collections::HashMap<String, JsonValue>),
}

// ---- auth ----

/// The signed-in user — the `{id, name, username, email}` the auth routes return, and the viewer the content
/// gates scope to. Serializes with bare field names (matching the web's response).
#[derive(Serialize, Clone, specta::Type)]
pub struct User {
    pub id: String,
    pub name: String,
    /// Public handle backing `/<username>`. Assigned at signup (see [`derive_unique_username`]).
    pub username: String,
    pub email: String,
    /// Whether `users.email_verified_at` is set — surfaced to the clients so they can prompt for
    /// verification (and gate verified-only actions later) without a second round trip.
    pub email_verified: bool,
}

// ---- blog ----

/// Index-card shape (no body).
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PostSummary {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub date_published: String,
    pub date_display: String,
}

/// Full post — `body` is the parsed JSON block list the web renders.
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PostFull {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub date_published: String,
    pub date_display: String,
    /// Wire-declared as the crate's JsonValue: specta rc.25 can't export serde_json::Value (see lib.rs).
    #[specta(type = JsonValue)]
    pub body: Value,
}

// ---- content sync (the /api/content/pull payload) ----

#[derive(Serialize, specta::Type)]
pub struct PullPayload {
    pub recipes: Vec<PullRecipe>,
    pub ingredients: Vec<PullIngredient>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRecipe {
    pub id: String,
    pub as_ingredient_id: String,
    pub user_id: Option<String>,
    pub visibility: String,
    pub name: String,
    pub subtitle: Option<String>,
    pub directions: Option<String>,
    pub serving_grams: Option<f64>,
    pub batch_grams: Option<f64>,
    pub slug: Option<String>,
    pub items: Vec<PullItem>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PullItem {
    pub ingredient_id: String,
    pub grams: f64,
    pub unit: Option<String>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PullIngredient {
    pub id: String,
    pub user_id: Option<String>,
    pub visibility: String,
    pub name: String,
    pub description: Option<String>,
    /// Cents (USD). i32 END-TO-END: SaveIngredientInput (the only write path) is i32, the desktop
    /// mirror is i32, the TS side is number — an i64 here was silent width drift in the wire contract
    /// (found by the specta-side repo audit; the class of bug a generated client would make impossible).
    /// Cents (USD). i32 END-TO-END: SaveIngredientInput (the only write path) is i32, the desktop
    /// mirror is i32, the TS side is number — an i64 here was silent width drift in the wire
    /// contract (found by the specta-side repo audit; the class of bug a generated client kills).
    pub price: Option<i32>,
    pub calories_per_100g: Option<f64>,
    pub serving_grams: Option<f64>,
    pub package_grams: Option<f64>,
    pub slug: Option<String>,
    /// Soft-delete tombstone (ms). Tombstoned rows STAY in the pull — recipes that use them need
    /// the data — and clients mirror the flag so their local list/search filtering matches.
    #[specta(type = Option<f64>)] // ms epoch — f64-safe on the wire
    pub deleted_at: Option<i64>,
    pub nutrients: Vec<PullReading>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PullReading {
    pub name: String,
    pub amount_per_100g: f64,
    pub unit: String,
}

// ---- direct messages ----

/// The other party, as the conversation list + thread header shows them.
#[derive(Serialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Party {
    pub id: String,
    pub name: String,
    pub username: String,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub with: Party,
    pub last_body: String,
    // ms epoch — f64-safe on the wire (the desktop mirror declares f64).
    #[specta(type = f64)]
    pub last_at: i64,
    /// True when the last message is the viewer's own (the list renders "You: …").
    pub last_is_mine: bool,
    #[specta(type = f64)] // a count (SQLite COUNT() is i64); wire-safe as number
    pub unread: i64,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub body: String,
    #[specta(type = f64)] // ms epoch — f64-safe on the wire
    pub created_at: i64,
    /// True when the viewer sent it (clients render alignment off this, not off raw ids).
    pub mine: bool,
}

/// A thread as the thread screen consumes it: the other party (resolved even before any message
/// exists, so a profile's "Message" button lands on an empty composer) + the messages, oldest first.
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub with: Party,
    pub messages: Vec<Message>,
}

// ---- the bell ----

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    pub kind: String,
    /// Parsed payload — per-kind (kind "ingredient-updated": `{ingredient: {id,name,slug}, by: {name,username}}`).
    /// Wire-declared as the crate's JsonValue: specta rc.25 can't export serde_json::Value (see lib.rs).
    #[specta(type = JsonValue)]
    pub payload: Value,
    // ms epoch — f64-safe on the wire until the year ~287,396 (the desktop mirror declares f64).
    #[specta(type = f64)]
    pub created_at: i64,
    pub read: bool,
}

/// The FULL HTTP wire contract as one specta collection: vegify-core's shared shapes plus every
/// server-local response shape above. Single source for every generated artifact of the API.
pub fn api_types() -> specta::Types {
    specta::Types::default()
        // vegify-core (shared DAL shapes — the set the web consumes over /api/content/*)
        .register::<vegify_core::Visibility>()
        .register::<vegify_core::Reading>()
        .register::<vegify_core::Amount>()
        .register::<vegify_core::AggregatedNutrition>()
        .register::<vegify_core::RecipeCard>()
        .register::<vegify_core::Profile>()
        .register::<vegify_core::RecipeItem>()
        .register::<vegify_core::RecipeView>()
        .register::<vegify_core::RecipeEditItem>()
        .register::<vegify_core::RecipeEditData>()
        .register::<vegify_core::IngredientCard>()
        .register::<vegify_core::IngredientSearchResult>()
        .register::<vegify_core::IngredientEditData>()
        .register::<vegify_core::RecipeSlugHit>()
        .register::<vegify_core::IngredientSlugHit>()
        .register::<vegify_core::SitemapData>()
        // this crate (server-local wire shapes)
        .register::<User>()
        .register::<PostSummary>()
        .register::<PostFull>()
        .register::<PullPayload>()
        .register::<Party>()
        .register::<ConversationSummary>()
        .register::<Message>()
        .register::<Thread>()
        .register::<Notification>()
}
