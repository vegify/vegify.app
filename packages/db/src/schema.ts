import { relations } from "drizzle-orm";
import {
  index,
  integer,
  real,
  sqliteTable,
  text,
  uniqueIndex,
} from "drizzle-orm/sqlite-core";
import { ulid } from "ulid";

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
    .$defaultFn(() => ulid());

const timestamps = {
  createdAt: integer("created_at", { mode: "timestamp_ms" }).$defaultFn(
    () => new Date()
  ),
  updatedAt: integer("updated_at", { mode: "timestamp_ms" })
    .$defaultFn(() => new Date())
    .$onUpdateFn(() => new Date()),
};

export const users = sqliteTable("users", {
  id: pk(),
  name: text("name").notNull(),
  email: text("email").notNull().unique(),
  passwordHash: text("password_hash"),
  ...timestamps,
});

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
    ...timestamps,
  },
  (t) => [index("sessions_user_idx").on(t.userId)]
);

export const amounts = sqliteTable("amounts", {
  id: pk(),
  unit: text("unit"),
  amount: real("amount").default(0),
  grams: real("grams").notNull().default(0),
  preferred: text("preferred", { enum: ["units", "grams"] })
    .notNull()
    .default("grams"),
  ...timestamps,
});

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
    description: text("description"),
    isVegan: integer("is_vegan", { mode: "boolean" }),
    price: integer("price"), // cents (USD)
    caloriesPer100g: real("calories_per_100g"),
    servingSizeId: text("serving_size_id").references(() => amounts.id, {
      onDelete: "cascade",
    }),
    batchSizeId: text("batch_size_id").references(() => amounts.id, {
      onDelete: "cascade",
    }),
    ...timestamps,
  },
  (t) => [
    index("ingredients_user_idx").on(t.userId),
    index("ingredients_name_idx").on(t.name),
  ]
);

export const videos = sqliteTable("videos", {
  id: pk(),
  url: text("url").notNull(),
  description: text("description"),
  ...timestamps,
});

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
    ...timestamps,
  },
  (t) => [uniqueIndex("recipes_as_ingredient_uq").on(t.asIngredientId)]
);

export const ingredientInRecipe = sqliteTable(
  "ingredient_in_recipe",
  {
    id: pk(),
    order: integer("order").notNull().default(0),
    recipeId: text("recipe_id")
      .notNull()
      .references(() => recipes.id, { onDelete: "cascade" }),
    ingredientId: text("ingredient_id").references(() => ingredients.id, {
      onDelete: "restrict",
    }),
    amountId: text("amount_id")
      .notNull()
      .references(() => amounts.id, { onDelete: "cascade" }),
    ...timestamps,
  },
  (t) => [index("iir_recipe_idx").on(t.recipeId)]
);

export const nutrients = sqliteTable("nutrients", {
  id: pk(),
  name: text("name").notNull(),
  description: text("description"),
  ...timestamps,
});

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
    ...timestamps,
  },
  (t) => [
    uniqueIndex("ingredient_nutrient_uq").on(t.ingredientId, t.nutrientId),
  ]
);

export const imgs = sqliteTable("imgs", {
  id: pk(),
  description: text("description"),
  uuid: text("uuid").notNull(),
  origName: text("orig_name").notNull(),
  extension: text("extension").notNull(),
  bucket: text("bucket").notNull(),
  contentType: text("content_type").notNull(),
  ...timestamps,
});

export const ingredientImg = sqliteTable("ingredient_img", {
  id: pk(),
  imgId: text("img_id")
    .notNull()
    .references(() => imgs.id, { onDelete: "cascade" }),
  ingredientId: text("ingredient_id")
    .notNull()
    .references(() => ingredients.id, { onDelete: "restrict" }),
  ...timestamps,
});

export const tags = sqliteTable("tags", {
  id: pk(),
  tag: text("tag").notNull().unique(),
  ...timestamps,
});

export const reviews = sqliteTable("reviews", {
  id: pk(),
  stars: real("stars").notNull(),
  title: text("title"),
  text: text("text"),
  ...timestamps,
});

export const hrefs = sqliteTable("hrefs", {
  id: pk(),
  url: text("url").notNull(),
  description: text("description"),
  ...timestamps,
});

export const products = sqliteTable("products", {
  id: pk(),
  name: text("name").notNull(),
  upc: integer("upc"),
  price: integer("price"),
  currency: text("currency", { enum: ["USD"] }),
  isVegan: integer("is_vegan", { mode: "boolean" }),
  description: text("description"),
  ...timestamps,
});

export const usersRelations = relations(users, ({ many }) => ({
  ingredients: many(ingredients),
  sessions: many(sessions),
}));

export const sessionsRelations = relations(sessions, ({ one }) => ({
  user: one(users, {
    fields: [sessions.userId],
    references: [users.id],
  }),
}));

export const ingredientsRelations = relations(ingredients, ({ one, many }) => ({
  creator: one(users, {
    fields: [ingredients.userId],
    references: [users.id],
  }),
  servingSize: one(amounts, {
    fields: [ingredients.servingSizeId],
    references: [amounts.id],
    relationName: "servingSize",
  }),
  batchSize: one(amounts, {
    fields: [ingredients.batchSizeId],
    references: [amounts.id],
    relationName: "batchSize",
  }),
  nutrients: many(ingredientNutrient),
  usedIn: many(ingredientInRecipe),
  images: many(ingredientImg),
}));

export const recipesRelations = relations(recipes, ({ one, many }) => ({
  asIngredient: one(ingredients, {
    fields: [recipes.asIngredientId],
    references: [ingredients.id],
  }),
  video: one(videos, {
    fields: [recipes.videoId],
    references: [videos.id],
  }),
  items: many(ingredientInRecipe),
}));

export const ingredientInRecipeRelations = relations(
  ingredientInRecipe,
  ({ one }) => ({
    recipe: one(recipes, {
      fields: [ingredientInRecipe.recipeId],
      references: [recipes.id],
    }),
    ingredient: one(ingredients, {
      fields: [ingredientInRecipe.ingredientId],
      references: [ingredients.id],
    }),
    amount: one(amounts, {
      fields: [ingredientInRecipe.amountId],
      references: [amounts.id],
    }),
  })
);

export const ingredientNutrientRelations = relations(
  ingredientNutrient,
  ({ one }) => ({
    ingredient: one(ingredients, {
      fields: [ingredientNutrient.ingredientId],
      references: [ingredients.id],
    }),
    nutrient: one(nutrients, {
      fields: [ingredientNutrient.nutrientId],
      references: [nutrients.id],
    }),
  })
);

export const ingredientImgRelations = relations(ingredientImg, ({ one }) => ({
  ingredient: one(ingredients, {
    fields: [ingredientImg.ingredientId],
    references: [ingredients.id],
  }),
  img: one(imgs, {
    fields: [ingredientImg.imgId],
    references: [imgs.id],
  }),
}));
