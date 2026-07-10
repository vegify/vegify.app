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
    /// JSON null.
    Null(()),
    /// JSON boolean.
    Bool(bool),
    /// JSON number (f64 on this wire; the reason this type exists).
    Number(f64),
    /// JSON string.
    String(String),
    /// JSON array.
    Array(Vec<JsonValue>),
    /// JSON object.
    Object(std::collections::HashMap<String, JsonValue>),
}

// ---- media ----

/// An approved upload: PUT the bytes to `url` (presigned, short-lived), then attach `key`.
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UploadTicket {
    /// Storage key to attach to the row once the PUT succeeds.
    pub key: String,
    /// Presigned, short-lived PUT URL for the bytes.
    pub url: String,
}

// ---- auth ----

/// The signed-in user — the `{id, name, username, email}` the auth routes return, and the viewer the content
/// gates scope to. Serializes with bare field names (matching the web's response).
#[derive(Serialize, Clone, specta::Type)]
pub struct User {
    /// User id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Public handle backing `/<username>`. Assigned at signup (see [`derive_unique_username`]).
    pub username: String,
    /// Login/notification address; visible only to the account itself.
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
    /// URL slug (`/blog/<slug>`).
    pub slug: String,
    /// Post title.
    pub title: String,
    /// One-to-two sentence summary (index cards + meta description).
    pub description: String,
    /// RFC 3339 publication timestamp (feeds `<time datetime>` + JSON-LD).
    pub date_published: String,
    /// Human-formatted publication date, preformatted server-side.
    pub date_display: String,
}

/// Full post — `body` is the parsed JSON block list the web renders.
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PostFull {
    /// URL slug (`/blog/<slug>`).
    pub slug: String,
    /// Post title.
    pub title: String,
    /// One-to-two sentence summary (meta description + JSON-LD).
    pub description: String,
    /// RFC 3339 publication timestamp.
    pub date_published: String,
    /// Human-formatted publication date, preformatted server-side.
    pub date_display: String,
    /// Wire-declared as the crate's JsonValue: specta rc.25 can't export serde_json::Value (see lib.rs).
    #[specta(type = JsonValue)]
    pub body: Value,
}

// ---- content sync (the /api/content/pull payload) ----

#[derive(Serialize, specta::Type)]
/// One full sync pull: every server row visible to the device's user.
/// Server-authoritative — the desktop mirrors these verbatim.
pub struct PullPayload {
    /// All visible recipes, with their lines.
    pub recipes: Vec<PullRecipe>,
    /// All visible ingredients, with their nutrient rows.
    pub ingredients: Vec<PullIngredient>,
    /// The creators of the rows above — public identity only. The desktop mirrors these into its
    /// local `users` cache so creator handles and `/<username>` profiles resolve on-device, logged
    /// out included. Users without a username are omitted (their content renders creatorless,
    /// exactly as the server serves it).
    pub users: Vec<PullUser>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
/// One creator as pulled — the public identity surface for content in this payload.
pub struct PullUser {
    /// User id (what PullRecipe/PullIngredient `user_id` points at).
    pub id: String,
    /// Profile handle (the `/<username>` URL segment).
    pub username: String,
    /// Display name.
    pub name: String,
    /// Media key of the profile avatar; clients compose `<api base>/<key>`.
    pub avatar_key: Option<String>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
/// One recipe row as pulled (server-authoritative mirror source).
pub struct PullRecipe {
    /// Recipe id (stable cross-replica).
    pub id: String,
    /// The recipe's as-ingredient pair id — must survive the mirror so
    /// consuming items' FKs stay intact.
    pub as_ingredient_id: String,
    /// Owner id; None = ownerless seed content.
    pub user_id: Option<String>,
    /// Visibility as stored (public/private/unlisted).
    pub visibility: String,
    /// Recipe title.
    pub name: String,
    /// Optional subtitle.
    pub subtitle: Option<String>,
    /// Free-text directions.
    pub directions: Option<String>,
    /// Serving size in grams, when declared.
    pub serving_grams: Option<f64>,
    /// Total batch mass in grams, when declared.
    pub batch_grams: Option<f64>,
    /// Current slug; mirrored verbatim so local links match the server.
    pub slug: Option<String>,
    /// The recipe's lines, in order.
    pub items: Vec<PullItem>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
/// One recipe line as pulled.
pub struct PullItem {
    /// The ingredient the line references.
    pub ingredient_id: String,
    /// Line quantity in grams (canonical).
    pub grams: f64,
    /// Display unit the author picked; None = grams.
    pub unit: Option<String>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
/// One ingredient row as pulled (server-authoritative mirror source).
pub struct PullIngredient {
    /// Ingredient id (stable cross-replica).
    pub id: String,
    /// Owner id; None = the communal catalog.
    pub user_id: Option<String>,
    /// Visibility as stored (public/private/unlisted).
    pub visibility: String,
    /// Ingredient name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Cents (USD). i32 END-TO-END: SaveIngredientInput (the only write path) is i32, the desktop
    /// mirror is i32, the TS side is number — an i64 here was silent width drift in the wire contract
    /// (found by the specta-side repo audit; the class of bug a generated client would make impossible).
    /// Cents (USD). i32 END-TO-END: SaveIngredientInput (the only write path) is i32, the desktop
    /// mirror is i32, the TS side is number — an i64 here was silent width drift in the wire
    /// contract (found by the specta-side repo audit; the class of bug a generated client kills).
    pub price: Option<i32>,
    /// Calories per 100 g, when known.
    pub calories_per_100g: Option<f64>,
    /// Serving size in grams, when declared.
    pub serving_grams: Option<f64>,
    /// Package mass in grams, when declared.
    pub package_grams: Option<f64>,
    /// Current slug; mirrored verbatim so local links match the server.
    pub slug: Option<String>,
    /// Soft-delete tombstone (ms). Tombstoned rows STAY in the pull — recipes that use them need
    /// the data — and clients mirror the flag so their local list/search filtering matches.
    #[specta(type = Option<f64>)] // ms epoch — f64-safe on the wire
    pub deleted_at: Option<i64>,
    /// Per-100 g nutrient rows.
    pub nutrients: Vec<PullReading>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
/// One nutrient reading as pulled, per 100 g.
pub struct PullReading {
    /// Nutrient name.
    pub name: String,
    /// Quantity per 100 g.
    pub amount_per_100g: f64,
    /// Unit for the quantity (g, mg, µg).
    pub unit: String,
}

// ---- direct messages ----

/// The other party, as the conversation list + thread header shows them.
#[derive(Serialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Party {
    /// User id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Public handle (`/<username>`).
    pub username: String,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
/// One conversation row of the DM list, newest-message first.
pub struct ConversationSummary {
    /// Conversation id.
    pub id: String,
    /// The other party.
    pub with: Party,
    /// Body of the newest message (the list's preview line).
    pub last_body: String,
    // ms epoch — f64-safe on the wire (the desktop mirror declares f64).
    #[specta(type = f64)]
    /// Newest message timestamp, ms epoch.
    pub last_at: i64,
    /// True when the last message is the viewer's own (the list renders "You: …").
    pub last_is_mine: bool,
    #[specta(type = f64)] // a count (SQLite COUNT() is i64); wire-safe as number
    /// Count of messages the viewer has not read.
    pub unread: i64,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
/// One DM as the thread screen renders it.
pub struct Message {
    /// Message id.
    pub id: String,
    /// Message body (plain text).
    pub body: String,
    #[specta(type = f64)] // ms epoch — f64-safe on the wire
    /// Send timestamp, ms epoch.
    pub created_at: i64,
    /// True when the viewer sent it (clients render alignment off this, not off raw ids).
    pub mine: bool,
}

/// A thread as the thread screen consumes it: the other party (resolved even before any message
/// exists, so a profile's "Message" button lands on an empty composer) + the messages, oldest first.
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    /// The other party (resolved even for an empty thread).
    pub with: Party,
    /// The messages, oldest first.
    pub messages: Vec<Message>,
}

// ---- the bell ----

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
/// One bell notification.
pub struct Notification {
    /// Notification id.
    pub id: String,
    /// Notification kind tag (e.g. "ingredient-updated"); selects the
    /// payload shape and the client-side renderer.
    pub kind: String,
    /// Parsed payload — per-kind (kind "ingredient-updated": `{ingredient: {id,name,slug}, by: {name,username}}`).
    /// Wire-declared as the crate's JsonValue: specta rc.25 can't export serde_json::Value (see lib.rs).
    #[specta(type = JsonValue)]
    pub payload: Value,
    // ms epoch — f64-safe on the wire until the year ~287,396 (the desktop mirror declares f64).
    #[specta(type = f64)]
    /// Creation timestamp, ms epoch.
    pub created_at: i64,
    /// Whether the viewer has opened it.
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
        .register::<UploadTicket>()
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
