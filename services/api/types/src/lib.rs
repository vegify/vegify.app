//! vegify-api-types — the server's WIRE CONTRACT as data: every serde shape the HTTP API speaks
//! (this crate) plus vegify-core's shared shapes (re-registered here), assembled into ONE specta
//! TypeCollection. Deliberately logic-free so consumers get the contract without the server:
//! vegify-typegen generates packages/api-types (TS) from it today; openapi.json (specta #31) and
//! vegify-client-rs type generation are the designated next consumers. The Rust twin of the
//! generated @vegify/api-types package.
use serde::{Deserialize, Serialize};
use serde_json::Value;

// This crate declares HONEST Rust widths (i64 ms epochs and counts, serde_json::Value for
// opaque JSON) — no per-field `#[specta(type = …)]` narrowing. The JS-facing exporters
// (vegify-typegen's TS emission, the desktop's ttipc bindings) apply specta-util's
// `Remapper::dangerous_bigints_as_number()` at export, which is wire-sound: serde serializes
// these i64s as JSON numbers and every live value sits far below 2^53. Exporters that can say
// int64 (openapi.json) get the true widths.

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
    /// The parsed JSON block list, exported as specta's own `serde_json::Value` shape.
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
    /// Visibility as stored — the shared enum, not a loose string (same wire bytes; the
    /// TS side tightens to the "public" | "private" | "unlisted" union).
    pub visibility: vegify_core::Visibility,
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
    /// Visibility as stored — the shared enum, not a loose string (same wire bytes).
    pub visibility: vegify_core::Visibility,
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
    /// Soft-delete tombstone (ms epoch). Tombstoned rows STAY in the pull — recipes that use them
    /// need the data — and clients mirror the flag so their local list/search filtering matches.
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
    /// Newest message timestamp, ms epoch.
    pub last_at: i64,
    /// True when the last message is the viewer's own (the list renders "You: …").
    pub last_is_mine: bool,
    /// Count of messages the viewer has not read (SQLite COUNT() is i64).
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
    /// Parsed payload — per-kind (kind "ingredient-updated": `{ingredient: {id,name,slug}, by: {name,username}}`),
    /// exported as specta's own `serde_json::Value` shape.
    pub payload: Value,
    /// Creation timestamp, ms epoch.
    pub created_at: i64,
    /// Whether the viewer has opened it.
    pub read: bool,
}

/// A DM send, as the client must state it. The server's own deserializer stays lenient (missing
/// fields answer 400 with a message rather than a deserialization error); the CONTRACT is that
/// both fields are required.
#[derive(Deserialize, specta::Type)]
pub struct SendMessageBody {
    /// Recipient handle (a username).
    pub to: String,
    /// Message body (plain text).
    pub body: String,
}

// ---- write acks (the first typed write responses — the diary graduates them; see api_operations) ----

/// A create/update ack carrying the affected row's id (the `{ id }` shape the content writes return).
#[derive(Serialize, specta::Type)]
pub struct SavedId {
    /// The created or updated row's id.
    pub id: String,
}

/// A minimal ok ack for a mutation with no return payload (the `{ ok: true }` shape a delete returns).
#[derive(Serialize, specta::Type)]
pub struct Ack {
    /// Always true on success (failures answer a non-200 with `{ error }`).
    pub ok: bool,
}

/// The FULL HTTP wire contract as one specta collection: vegify-core's shared shapes plus every
/// server-local response shape above. Single source for every generated artifact of the API.
pub fn api_types() -> specta::Types {
    specta::Types::default()
        // vegify-core (shared DAL shapes — the set the web consumes over /api/content/*)
        .register::<vegify_core::Visibility>()
        .register::<vegify_core::Sort>()
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
        // vegify-core diary shapes (the private food log — P1.1)
        .register::<vegify_core::SaveLogEntryInput>()
        .register::<vegify_core::LogEntryView>()
        .register::<vegify_core::NutrientTotal>()
        .register::<vegify_core::DayLog>()
        .register::<vegify_core::RecentIngredient>()
        .register::<vegify_core::LogPullNutrient>()
        .register::<vegify_core::LogPullEntry>()
        .register::<vegify_core::LogPull>()
        // vegify-core nutrition profile + personalized targets (P1.3 — private, vegan-aware). DayLog
        // now carries `targets: NutrientTarget[]`, so those shapes must register here too.
        .register::<vegify_core::DriSex>()
        .register::<vegify_core::NutritionProfile>()
        .register::<vegify_core::TargetBasis>()
        .register::<vegify_core::NutrientTarget>()
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
        .register::<SendMessageBody>()
        .register::<SavedId>()
        .register::<Ack>()
}

/// The `paths` object for openapi.json: every endpoint whose request AND response shapes are typed
/// in this crate's collection — the read surface, the pull, messages, notifications, the session
/// probe, and (as of P1.1) the private diary's full write+read surface, whose acks are the first
/// typed write responses (`SavedId`/`Ack`). The auth flows and the older content write acks still
/// answer ad-hoc `{ok,…}`/`{token,…}` JSON; they join here as their shapes graduate into this crate
/// (declaring them untyped would make the document lie). Bearer-auth requirements are stated in each
/// description; the server enforces them regardless.
#[cfg(feature = "openapi")]
pub fn api_operations() -> Vec<specta_openapi::Operation> {
    use specta_openapi::Operation;
    use vegify_core as core;
    vec![
        Operation::get("/health")
            .summary("Liveness probe")
            .description("Plain-text ok; the deploy gate and uptime checks poll this.")
            .response::<String>(200, "The literal string \"ok\""),
        Operation::get("/api/content/recipes")
            .summary("Recipe catalog, one keyset page")
            .description(
                "Public. Sort + keyset cursor; viewer's own non-public rows included when authed.",
            )
            .query_param::<core::Sort>("sort")
            .query_param::<String>("cursor")
            .query_param::<String>("cursor_name")
            .query_param::<u32>("limit")
            .response::<Vec<core::RecipeCard>>(200, "One page of recipe cards"),
        Operation::get("/api/content/ingredients")
            .summary("Ingredient catalog, one keyset page")
            .description("Public. Same paging contract as /api/content/recipes.")
            .query_param::<core::Sort>("sort")
            .query_param::<String>("cursor")
            .query_param::<String>("cursor_name")
            .query_param::<u32>("limit")
            .response::<Vec<core::IngredientCard>>(200, "One page of ingredient cards"),
        Operation::get("/api/content/recipe-detail")
            .summary("One recipe, render-ready")
            .description(
                "Public (visibility-gated). Null when absent or not visible to the viewer.",
            )
            .query_param::<String>("id")
            .response::<Option<core::RecipeView>>(200, "The recipe view, or null"),
        Operation::get("/api/content/recipe-edit")
            .summary("One recipe, edit-mode source data")
            .description("Requires bearer; owner-gated. Null when absent or not editable.")
            .query_param::<String>("id")
            .response::<Option<core::RecipeEditData>>(200, "The edit payload, or null"),
        Operation::get("/api/content/ingredient-detail")
            .summary("One ingredient, render-ready")
            .description("Public (visibility-gated). Null when absent or not visible.")
            .query_param::<String>("id")
            .response::<Option<core::IngredientEditData>>(200, "The ingredient payload, or null"),
        Operation::get("/api/content/ingredient-edit")
            .summary("One ingredient, edit-mode source data")
            .description("Requires bearer; owner-gated. Null when absent or not editable.")
            .query_param::<String>("id")
            .response::<Option<core::IngredientEditData>>(200, "The edit payload, or null"),
        Operation::get("/api/content/search")
            .summary("Ingredient search")
            .description("Public. The recipe composer's search box.")
            .query_param::<String>("q")
            .response::<Vec<core::IngredientSearchResult>>(200, "Matching ingredients"),
        Operation::get("/api/content/pull")
            .summary("Full content sync pull")
            .description(
                "Public; optional bearer widens the payload to the viewer's own non-public rows.",
            )
            .response::<PullPayload>(200, "Every visible row, in mutation shape"),
        Operation::get("/api/content/profile")
            .summary("Public profile by handle")
            .description("Public; optional bearer lets owners see their own non-public shelves.")
            .query_param::<String>("username")
            .response::<Option<core::Profile>>(200, "The profile, or null"),
        Operation::get("/api/content/recipe-by-slug")
            .summary("Resolve /<username>/<slug> to a recipe")
            .description("Public. Null → 404; a canonicalSlug differing from the request → 301.")
            .query_param::<String>("username")
            .query_param::<String>("slug")
            .response::<Option<core::RecipeSlugHit>>(200, "The resolution, or null"),
        Operation::get("/api/content/ingredient-by-slug")
            .summary("Resolve /ingredients/<slug> to an ingredient")
            .description("Public. Null → 404; a canonicalSlug differing from the request → 301.")
            .query_param::<String>("slug")
            .response::<Option<core::IngredientSlugHit>>(200, "The resolution, or null"),
        Operation::get("/api/content/sitemap")
            .summary("Sitemap source data")
            .description("Public. The web shell renders sitemap.xml from this.")
            .response::<core::SitemapData>(200, "Every public canonical URL's source row"),
        Operation::get("/api/content/blog")
            .summary("Blog index")
            .description("Public. Card shapes only, no bodies.")
            .response::<Vec<PostSummary>>(200, "All published posts, newest first"),
        Operation::get("/api/content/blog-detail")
            .summary("One blog post")
            .description("Public. Null when the slug matches nothing.")
            .query_param::<String>("slug")
            .response::<Option<PostFull>>(200, "The post with its block-list body, or null"),
        Operation::get("/api/auth/session")
            .summary("Who am I")
            .description("Requires bearer. The signed-in user, or 401.")
            .response::<User>(200, "The signed-in user"),
        Operation::get("/api/messages/conversations")
            .summary("DM conversation list")
            .description("Requires bearer. Newest-message first.")
            .response::<Vec<ConversationSummary>>(200, "The viewer's conversations"),
        Operation::get("/api/messages/thread")
            .summary("One DM thread")
            .description(
                "Requires bearer. Oldest message first; resolves the party even when empty.",
            )
            .query_param::<String>("with")
            .response::<Thread>(200, "The thread with the named party"),
        Operation::post("/api/messages/send")
            .summary("Send a DM")
            .description("Requires bearer. Blocked pairs answer 403.")
            .request_body::<SendMessageBody>()
            .response::<Message>(200, "The created message"),
        Operation::get("/api/notifications")
            .summary("The bell")
            .description("Requires bearer. Per-kind payload parsed inline.")
            .response::<Vec<Notification>>(200, "The viewer's notifications, newest first"),
        Operation::post("/api/log/entries")
            .summary("Log a diary entry")
            .description(
                "Requires bearer; PRIVATE to the viewer. Logs `grams` of an ingredient (or a recipe \
                 via its as-ingredient id) against a client-chosen calendar date. Upsert-by-id: a body \
                 id updates that entry (owner-gated), else a new one is minted.",
            )
            .request_body::<core::SaveLogEntryInput>()
            .response::<SavedId>(200, "The created/updated entry's id"),
        Operation::patch("/api/log/entries")
            .summary("Update a diary entry")
            .description("Requires bearer; owner-gated. Same body as POST, with the entry id set.")
            .request_body::<core::SaveLogEntryInput>()
            .response::<SavedId>(200, "The updated entry's id"),
        Operation::delete("/api/log/entries")
            .summary("Delete a diary entry")
            .description("Requires bearer; owner-gated. Soft-deletes by id (the row survives).")
            .query_param::<String>("id")
            .response::<Ack>(200, "Deletion acknowledged"),
        Operation::get("/api/log/day")
            .summary("One diary day + rolled-up totals + personalized targets")
            .description(
                "Requires bearer; PRIVATE to the viewer. The date's entries plus server-computed \
                 nutrient totals (each entry rolled up through the same nested-recipe CTE recipes use) \
                 and the viewer's personalized vegan-aware daily targets (from their nutrition profile; \
                 generic-adult when unset).",
            )
            .query_param::<String>("date")
            .response::<core::DayLog>(200, "The day's entries + nutrient totals + targets"),
        Operation::get("/api/log/recents")
            .summary("Recently logged ingredients")
            .description(
                "Requires bearer; PRIVATE to the viewer. Distinct ingredients the viewer has logged, \
                 newest first, to prepend the add-flow's search.",
            )
            .query_param::<u32>("limit")
            .response::<Vec<core::RecentIngredient>>(200, "The viewer's recents, newest first"),
        Operation::get("/api/log/pull")
            .summary("Full diary pull (authed device sync)")
            .description(
                "Requires bearer; PRIVATE to the viewer. The viewer's ENTIRE diary, each entry with its \
                 frozen snapshot — the desktop's local-first cache sync channel, never the anonymous \
                 content pull.",
            )
            .response::<core::LogPull>(200, "Every live diary entry with its frozen snapshot"),
        Operation::get("/api/profile")
            .summary("The viewer's nutrition profile")
            .description(
                "Requires bearer; PRIVATE to the viewer. Age/sex/weight/pregnancy/supplement inputs \
                 that drive personalized targets. All fields optional — absent ones default to the \
                 generic-adult DRI tier (an unset profile returns all nulls).",
            )
            .response::<core::NutritionProfile>(200, "The viewer's nutrition profile (defaults when unset)"),
        Operation::post("/api/profile")
            .summary("Upsert the viewer's nutrition profile")
            .description(
                "Requires bearer; owner-scoped. Replaces the single per-user profile row; changes \
                 personalized targets on the next diary-day read.",
            )
            .request_body::<core::NutritionProfile>()
            .response::<Ack>(200, "Profile saved"),
    ]
}
