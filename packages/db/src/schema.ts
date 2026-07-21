import { relations } from "drizzle-orm"
import {
  index,
  integer,
  real,
  sqliteTable,
  text,
  uniqueIndex
} from "drizzle-orm/sqlite-core"
import { ulid } from "ulid"

// Schema ported from vegify-laravel (2022) database/migrations, SQLite dialect.
// Key idea preserved: a recipe IS an ingredient (recipes.as_ingredient_id) — that row
// carries the recipe's name, creator, serving/batch sizes, and lets recipes nest.
//
// IDs are client-generated ULIDs (text), not autoincrement integers: an offline device can mint
// ids that never collide with another device's on sync (the local-first/changeset-sync
// prerequisite), and ULIDs sort lexicographically by creation time. Stays Postgres-portable.

/** Client-generated ULID primary key — sortable + offline-safe (no autoincrement). */
const pk = () =>
  text("id")
    .primaryKey()
    .$defaultFn(() => ulid())

const timestamps = {
  createdAt: integer("created_at", { mode: "timestamp_ms" }).$defaultFn(
    () => new Date()
  ),
  updatedAt: integer("updated_at", { mode: "timestamp_ms" })
    .$defaultFn(() => new Date())
    .$onUpdateFn(() => new Date())
}

export const users = sqliteTable("users", {
  id: pk(),
  name: text("name").notNull(),
  // Profile avatar's media key (media/<ulid>.<ext>, served at the API's /media/*); NULL = none.
  avatarKey: text("avatar_key"),
  // Public handle for /<username> profiles. Assigned at signup (a slug of the name, deduped); existing
  // rows are backfilled in vegify-server's ensure_schema. Lower-cased, unique.
  username: text("username").notNull().unique(),
  email: text("email").notNull().unique(),
  passwordHash: text("password_hash"),
  // Null until the user confirms their address via the verification link (A5). The reset/verify flows
  // live server-side in vegify-server; this column is the source of truth for "is this email confirmed".
  emailVerifiedAt: integer("email_verified_at", { mode: "timestamp_ms" }),
  ...timestamps
})

// Opaque server-side sessions. The raw token is handed to the client (httpOnly cookie on web,
// OS keychain on desktop); only its SHA-256 hash lives here, so a DB leak yields no usable
// tokens. Lookups are by hashed_token (unique); expired rows are filtered at validate time.
export const sessions = sqliteTable(
  "sessions",
  {
    id: pk(),
    userId: text("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    hashedToken: text("hashed_token").notNull().unique(),
    expiresAt: integer("expires_at", { mode: "timestamp_ms" }).notNull(),
    ...timestamps
  },
  (t) => [index("sessions_user_idx").on(t.userId)]
)

// Single-use, expiring password-reset tokens. Like `sessions`, only the SHA-256 hash of the raw token
// is stored — the raw token rides the reset link emailed to the user — so a DB leak yields no usable
// tokens. `used_at` enforces single use; expired or already-used rows are rejected at confirm time.
export const passwordResetTokens = sqliteTable(
  "password_reset_tokens",
  {
    id: pk(),
    userId: text("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    hashedToken: text("hashed_token").notNull().unique(),
    expiresAt: integer("expires_at", { mode: "timestamp_ms" }).notNull(),
    usedAt: integer("used_at", { mode: "timestamp_ms" }),
    ...timestamps
  },
  (t) => [index("password_reset_tokens_user_idx").on(t.userId)]
)

export const amounts = sqliteTable("amounts", {
  id: pk(),
  unit: text("unit"),
  amount: real("amount").default(0),
  grams: real("grams").notNull().default(0),
  preferred: text("preferred", { enum: ["units", "grams"] })
    .notNull()
    .default("grams"),
  ...timestamps
})

export const ingredients = sqliteTable(
  "ingredients",
  {
    id: pk(),
    userId: text("user_id").references(() => users.id),
    // UGC visibility (the app is public-default sharing): public = anyone lists+reads; unlisted =
    // readable by direct link but not listed; private = owner only. A recipe IS an ingredient
    // (as_ingredient_id), so this one field covers recipes too — the recipe form's visibility fills
    // through to its as-ingredient. Ownership (`userId`) gates EDITING, not reading.
    visibility: text("visibility", { enum: ["public", "private", "unlisted"] })
      .notNull()
      .default("public"),
    name: text("name").notNull(),
    // SEO/GEO slug (kebab of the name), the human URL segment. One column serves both roles this row
    // can play: a RECIPE (has a recipes row) is canonical at /<username>/<slug> — slug unique per
    // owner; a LEAF ingredient (no recipes row) is canonical at /ingredients/<slug> — slug unique
    // globally. The two namespaces are independent, so a recipe slug and an ingredient slug may
    // coincide. Uniqueness is enforced per-scope in the shared DAL at save time (not a single DB
    // constraint — the scope differs), with a numeric suffix on collision. Nullable only for the
    // pre-backfill window; the DAL always writes it. Renames regenerate it and log the old one to
    // slug_history for 301s.
    slug: text("slug"),
    // Provenance for imported reference data (docs/usernames.md): user content leaves this NULL;
    // catalog imports stamp their source (e.g. "USDA FoodData Central"). Unowned (user_id NULL) +
    // sourced rows are the communal reference catalog — uneditable by users, listed for everyone.
    source: text("source"),
    // Soft-delete tombstone (ms epoch). Set when the owner deletes an ingredient that recipes still
    // use: the row and its readings survive so every referencing recipe keeps working; browse/search
    // and the sitemap exclude it; the owner's own recipes grey it out with a restore affordance.
    // NULL = live. Unreferenced deletes stay hard (row removed).
    deletedAt: integer("deleted_at"),
    description: text("description"),
    isVegan: integer("is_vegan", { mode: "boolean" }),
    price: integer("price"), // cents (USD)
    caloriesPer100g: real("calories_per_100g"),
    servingSizeId: text("serving_size_id").references(() => amounts.id, {
      onDelete: "cascade"
    }),
    batchSizeId: text("batch_size_id").references(() => amounts.id, {
      onDelete: "cascade"
    }),
    ...timestamps
  },
  (t) => [
    index("ingredients_user_idx").on(t.userId),
    index("ingredients_name_idx").on(t.name),
    index("ingredients_slug_idx").on(t.slug)
  ]
)

// Old→current slug redirects. On a rename that changes the slug, the previous slug is logged here so
// /<username>/<old-slug> and /ingredients/<old-slug> 301 to the row's current canonical URL. `scope`
// disambiguates the two namespaces: the owner's user_id for a recipe slug, NULL for a global
// ingredient slug — so the same old slug can live in both without clashing. `targetId` is the
// ingredients.id whose current slug wins (resolve it to the live canonical URL at redirect time).
export const slugHistory = sqliteTable(
  "slug_history",
  {
    id: pk(),
    slug: text("slug").notNull(),
    scope: text("scope"), // user_id for recipe slugs; NULL = global ingredient scope
    targetId: text("target_id")
      .notNull()
      .references(() => ingredients.id, { onDelete: "cascade" }),
    ...timestamps
  },
  (t) => [uniqueIndex("slug_history_scope_slug_uq").on(t.scope, t.slug)]
)

export const videos = sqliteTable("videos", {
  id: pk(),
  url: text("url").notNull(),
  description: text("description"),
  ...timestamps
})

export const recipes = sqliteTable(
  "recipes",
  {
    id: pk(),
    asIngredientId: text("as_ingredient_id")
      .notNull()
      .references(() => ingredients.id, { onDelete: "cascade" }),
    subtitle: text("subtitle"),
    directions: text("directions"),
    prepMinutes: real("prep_minutes"),
    cookMinutes: real("cook_minutes"),
    totalTime: real("total_time"),
    videoId: text("video_id").references(() => videos.id),
    ...timestamps
  },
  (t) => [uniqueIndex("recipes_as_ingredient_uq").on(t.asIngredientId)]
)

export const ingredientInRecipe = sqliteTable(
  "ingredient_in_recipe",
  {
    id: pk(),
    order: integer("order").notNull().default(0),
    recipeId: text("recipe_id")
      .notNull()
      .references(() => recipes.id, { onDelete: "cascade" }),
    ingredientId: text("ingredient_id").references(() => ingredients.id, {
      onDelete: "restrict"
    }),
    amountId: text("amount_id")
      .notNull()
      .references(() => amounts.id, { onDelete: "cascade" }),
    ...timestamps
  },
  (t) => [index("iir_recipe_idx").on(t.recipeId)]
)

// The food DIARY — a user's dated log of what they ate. PRIVATE, full stop: unlike the public-default
// content tables above, log entries are personal data — never listed, never in the anonymous content
// pull or the sitemap; the server's every /api/log/* endpoint is hard-authed to the owner. The privacy
// carve-out is enforced at the query/endpoint layer; this table just stores the rows.
//
// A row logs `grams` of an ingredient against a user-local calendar `date` ('YYYY-MM-DD', chosen
// client-side — no server timezone modeling). Because a recipe IS an ingredient (recipes.as_ingredient_id),
// logging a recipe serving is just logging its as-ingredient id: the SAME recursive nutrition CTE that
// rolls up recipes rolls up a logged recipe, so day totals need no parallel math. Quantity mirrors
// ingredient_in_recipe's amount pattern (an `amounts` row via amountId — grams canonical, display unit
// preserved). `slot` (breakfast/lunch/dinner/snack) is a cheap column the UI ignores for now.
//
// v0 accepted drift: entries reference LIVE ingredients, so editing a referenced ingredient later
// changes historical days. Acceptable while the catalog is USDA-locked-ish; the durable fix
// (copy-on-edit / versioning) lands with provenance in P2.5.
export const logEntries = sqliteTable(
  "log_entries",
  {
    id: pk(),
    userId: text("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    // User-local calendar date 'YYYY-MM-DD' the food is logged against (client-chosen).
    date: text("date").notNull(),
    // Meal slot (breakfast/lunch/dinner/snack); nullable, UI-ignored until a later phase.
    slot: text("slot"),
    // The logged ingredient — or a recipe's as_ingredient_id (logging a recipe). RESTRICT mirrors
    // ingredient_in_recipe: a logged ingredient can't be hard-deleted out from under history (the DAL
    // soft-deletes/tombstones it instead), so a day always resolves.
    ingredientId: text("ingredient_id")
      .notNull()
      .references(() => ingredients.id, { onDelete: "restrict" }),
    // Quantity as an amounts row (grams canonical + display unit) — the ingredient_in_recipe pattern.
    amountId: text("amount_id")
      .notNull()
      .references(() => amounts.id, { onDelete: "cascade" }),
    // When the entry was recorded (ms epoch); orders "recents" and the day list. May be client-supplied
    // on offline create / sync replay so ordering survives round-trips.
    loggedAt: integer("logged_at", { mode: "timestamp_ms" }).$defaultFn(
      () => new Date()
    ),
    // Soft-delete tombstone (ms epoch); NULL = live. Kept (not hard-deleted) so an undo/versioning
    // story stays possible and sync can propagate the deletion.
    deletedAt: integer("deleted_at"),
    ...timestamps
  },
  (t) => [
    index("log_entries_user_date_idx").on(t.userId, t.date),
    index("log_entries_user_logged_idx").on(t.userId, t.loggedAt)
  ]
)

export const nutrients = sqliteTable("nutrients", {
  id: pk(),
  name: text("name").notNull(),
  description: text("description"),
  ...timestamps
})

// NEW relative to vegify-laravel: it had a nutrients table but never the join.
// Nutrient content per 100 g of an ingredient — the micronutrition core.
export const ingredientNutrient = sqliteTable(
  "ingredient_nutrient",
  {
    id: pk(),
    ingredientId: text("ingredient_id")
      .notNull()
      .references(() => ingredients.id, { onDelete: "cascade" }),
    nutrientId: text("nutrient_id")
      .notNull()
      .references(() => nutrients.id, { onDelete: "cascade" }),
    amountPer100g: real("amount_per_100g").notNull(),
    unit: text("unit").notNull(),
    ...timestamps
  },
  (t) => [
    uniqueIndex("ingredient_nutrient_uq").on(t.ingredientId, t.nutrientId)
  ]
)

export const imgs = sqliteTable("imgs", {
  id: pk(),
  description: text("description"),
  uuid: text("uuid").notNull(),
  origName: text("orig_name").notNull(),
  extension: text("extension").notNull(),
  bucket: text("bucket").notNull(),
  contentType: text("content_type").notNull(),
  ...timestamps
})

export const ingredientImg = sqliteTable("ingredient_img", {
  id: pk(),
  imgId: text("img_id")
    .notNull()
    .references(() => imgs.id, { onDelete: "cascade" }),
  ingredientId: text("ingredient_id")
    .notNull()
    .references(() => ingredients.id, { onDelete: "restrict" }),
  ...timestamps
})

export const tags = sqliteTable("tags", {
  id: pk(),
  tag: text("tag").notNull().unique(),
  ...timestamps
})

export const reviews = sqliteTable("reviews", {
  id: pk(),
  stars: real("stars").notNull(),
  title: text("title"),
  text: text("text"),
  ...timestamps
})

export const hrefs = sqliteTable("hrefs", {
  id: pk(),
  url: text("url").notNull(),
  description: text("description"),
  ...timestamps
})

export const products = sqliteTable("products", {
  id: pk(),
  name: text("name").notNull(),
  upc: integer("upc"),
  price: integer("price"),
  currency: text("currency", { enum: ["USD"] }),
  isVegan: integer("is_vegan", { mode: "boolean" }),
  description: text("description"),
  ...timestamps
})

export const usersRelations = relations(users, ({ many }) => ({
  ingredients: many(ingredients),
  sessions: many(sessions),
  logEntries: many(logEntries)
}))

export const logEntriesRelations = relations(logEntries, ({ one }) => ({
  user: one(users, {
    fields: [logEntries.userId],
    references: [users.id]
  }),
  ingredient: one(ingredients, {
    fields: [logEntries.ingredientId],
    references: [ingredients.id]
  }),
  amount: one(amounts, {
    fields: [logEntries.amountId],
    references: [amounts.id]
  })
}))

export const sessionsRelations = relations(sessions, ({ one }) => ({
  user: one(users, {
    fields: [sessions.userId],
    references: [users.id]
  })
}))

export const ingredientsRelations = relations(ingredients, ({ one, many }) => ({
  creator: one(users, {
    fields: [ingredients.userId],
    references: [users.id]
  }),
  servingSize: one(amounts, {
    fields: [ingredients.servingSizeId],
    references: [amounts.id],
    relationName: "servingSize"
  }),
  batchSize: one(amounts, {
    fields: [ingredients.batchSizeId],
    references: [amounts.id],
    relationName: "batchSize"
  }),
  nutrients: many(ingredientNutrient),
  usedIn: many(ingredientInRecipe),
  images: many(ingredientImg)
}))

export const recipesRelations = relations(recipes, ({ one, many }) => ({
  asIngredient: one(ingredients, {
    fields: [recipes.asIngredientId],
    references: [ingredients.id]
  }),
  video: one(videos, {
    fields: [recipes.videoId],
    references: [videos.id]
  }),
  items: many(ingredientInRecipe)
}))

export const ingredientInRecipeRelations = relations(
  ingredientInRecipe,
  ({ one }) => ({
    recipe: one(recipes, {
      fields: [ingredientInRecipe.recipeId],
      references: [recipes.id]
    }),
    ingredient: one(ingredients, {
      fields: [ingredientInRecipe.ingredientId],
      references: [ingredients.id]
    }),
    amount: one(amounts, {
      fields: [ingredientInRecipe.amountId],
      references: [amounts.id]
    })
  })
)

export const ingredientNutrientRelations = relations(
  ingredientNutrient,
  ({ one }) => ({
    ingredient: one(ingredients, {
      fields: [ingredientNutrient.ingredientId],
      references: [ingredients.id]
    }),
    nutrient: one(nutrients, {
      fields: [ingredientNutrient.nutrientId],
      references: [nutrients.id]
    })
  })
)

export const ingredientImgRelations = relations(ingredientImg, ({ one }) => ({
  ingredient: one(ingredients, {
    fields: [ingredientImg.ingredientId],
    references: [ingredients.id]
  }),
  img: one(imgs, {
    fields: [ingredientImg.imgId],
    references: [imgs.id]
  })
}))
