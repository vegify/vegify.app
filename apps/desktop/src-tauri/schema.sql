-- Content-cache schema for the desktop's on-device SQLite DB.
--
-- The desktop is a LOCAL-FIRST CACHE of the server (see data.rs): `Db::open` creates the `_outbox`
-- push queue, and these content tables hold the pulled world. In dev the repo's `.data/vegify.db`
-- already carries this schema (from `pnpm db:push`), but a SHIPPED build opens a FRESH app-data DB
-- with no content tables — so the first sign-in pull (apply_pull → vegify-core do_save_*) failed with
-- "no such table". `ensure_content_schema` runs this on every `Db::open`, idempotently (every stmt is
-- IF NOT EXISTS, a no-op once the tables exist), mirroring the server's own boot-time `ensure_schema`.
--
-- SOURCE OF TRUTH = Drizzle (`packages/db/src/schema.ts` → `.data/vegify.db`). This file is GENERATED
-- from that DB's `.schema` (CREATE … → CREATE … IF NOT EXISTS; `_outbox`/`sqlite_sequence` dropped).
-- It is NOT hand-maintained: `schema_sql_matches_drizzle_dev_db` (data.rs) fails CI if it drifts from
-- the Drizzle schema, so regenerate it (same pipeline) when the schema changes. The desktop only
-- actively writes the content subset (users, ingredients, recipes, amounts, ingredient_in_recipe,
-- ingredient_nutrient, nutrients); the rest are kept so the schema stays a faithful, FK-complete
-- mirror of the dev/server DB.
CREATE TABLE IF NOT EXISTS `amounts` (
	`id` text PRIMARY KEY NOT NULL,
	`unit` text,
	`amount` real DEFAULT 0,
	`grams` real DEFAULT 0 NOT NULL,
	`preferred` text DEFAULT 'grams' NOT NULL,
	`created_at` integer,
	`updated_at` integer
);
CREATE TABLE IF NOT EXISTS `hrefs` (
	`id` text PRIMARY KEY NOT NULL,
	`url` text NOT NULL,
	`description` text,
	`created_at` integer,
	`updated_at` integer
);
CREATE TABLE IF NOT EXISTS `imgs` (
	`id` text PRIMARY KEY NOT NULL,
	`description` text,
	`uuid` text NOT NULL,
	`orig_name` text NOT NULL,
	`extension` text NOT NULL,
	`bucket` text NOT NULL,
	`content_type` text NOT NULL,
	`created_at` integer,
	`updated_at` integer
);
CREATE TABLE IF NOT EXISTS `ingredient_img` (
	`id` text PRIMARY KEY NOT NULL,
	`img_id` text NOT NULL,
	`ingredient_id` text NOT NULL,
	`created_at` integer,
	`updated_at` integer,
	FOREIGN KEY (`img_id`) REFERENCES `imgs`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`ingredient_id`) REFERENCES `ingredients`(`id`) ON UPDATE no action ON DELETE restrict
);
CREATE TABLE IF NOT EXISTS `ingredient_in_recipe` (
	`id` text PRIMARY KEY NOT NULL,
	`order` integer DEFAULT 0 NOT NULL,
	`recipe_id` text NOT NULL,
	`ingredient_id` text,
	`amount_id` text NOT NULL,
	`created_at` integer,
	`updated_at` integer,
	FOREIGN KEY (`recipe_id`) REFERENCES `recipes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`ingredient_id`) REFERENCES `ingredients`(`id`) ON UPDATE no action ON DELETE restrict,
	FOREIGN KEY (`amount_id`) REFERENCES `amounts`(`id`) ON UPDATE no action ON DELETE cascade
);
CREATE INDEX IF NOT EXISTS `iir_recipe_idx` ON `ingredient_in_recipe` (`recipe_id`);
CREATE TABLE IF NOT EXISTS `ingredient_nutrient` (
	`id` text PRIMARY KEY NOT NULL,
	`ingredient_id` text NOT NULL,
	`nutrient_id` text NOT NULL,
	`amount_per_100g` real NOT NULL,
	`unit` text NOT NULL,
	`created_at` integer,
	`updated_at` integer,
	FOREIGN KEY (`ingredient_id`) REFERENCES `ingredients`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`nutrient_id`) REFERENCES `nutrients`(`id`) ON UPDATE no action ON DELETE cascade
);
CREATE UNIQUE INDEX IF NOT EXISTS `ingredient_nutrient_uq` ON `ingredient_nutrient` (`ingredient_id`,`nutrient_id`);
CREATE TABLE IF NOT EXISTS `ingredients` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text,
	`name` text NOT NULL,
	`description` text,
	`is_vegan` integer,
	`price` integer,
	`calories_per_100g` real,
	`serving_size_id` text,
	`batch_size_id` text,
	`created_at` integer,
	`updated_at` integer, visibility text NOT NULL DEFAULT 'public',
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`serving_size_id`) REFERENCES `amounts`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`batch_size_id`) REFERENCES `amounts`(`id`) ON UPDATE no action ON DELETE cascade
);
CREATE INDEX IF NOT EXISTS `ingredients_user_idx` ON `ingredients` (`user_id`);
CREATE INDEX IF NOT EXISTS `ingredients_name_idx` ON `ingredients` (`name`);
CREATE TABLE IF NOT EXISTS `nutrients` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`description` text,
	`created_at` integer,
	`updated_at` integer
);
CREATE TABLE IF NOT EXISTS `products` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`upc` integer,
	`price` integer,
	`currency` text,
	`is_vegan` integer,
	`description` text,
	`created_at` integer,
	`updated_at` integer
);
CREATE TABLE IF NOT EXISTS `recipes` (
	`id` text PRIMARY KEY NOT NULL,
	`as_ingredient_id` text NOT NULL,
	`subtitle` text,
	`directions` text,
	`prep_minutes` real,
	`cook_minutes` real,
	`total_time` real,
	`video_id` text,
	`created_at` integer,
	`updated_at` integer,
	FOREIGN KEY (`as_ingredient_id`) REFERENCES `ingredients`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`video_id`) REFERENCES `videos`(`id`) ON UPDATE no action ON DELETE no action
);
CREATE UNIQUE INDEX IF NOT EXISTS `recipes_as_ingredient_uq` ON `recipes` (`as_ingredient_id`);
CREATE TABLE IF NOT EXISTS `reviews` (
	`id` text PRIMARY KEY NOT NULL,
	`stars` real NOT NULL,
	`title` text,
	`text` text,
	`created_at` integer,
	`updated_at` integer
);
CREATE TABLE IF NOT EXISTS `tags` (
	`id` text PRIMARY KEY NOT NULL,
	`tag` text NOT NULL,
	`created_at` integer,
	`updated_at` integer
);
CREATE UNIQUE INDEX IF NOT EXISTS `tags_tag_unique` ON `tags` (`tag`);
CREATE TABLE IF NOT EXISTS `users` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`email` text NOT NULL,
	`created_at` integer,
	`updated_at` integer
, `password_hash` text, `email_verified_at` integer);
CREATE UNIQUE INDEX IF NOT EXISTS `users_email_unique` ON `users` (`email`);
CREATE TABLE IF NOT EXISTS `videos` (
	`id` text PRIMARY KEY NOT NULL,
	`url` text NOT NULL,
	`description` text,
	`created_at` integer,
	`updated_at` integer
);
CREATE TABLE IF NOT EXISTS `sessions` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`hashed_token` text NOT NULL,
	`expires_at` integer NOT NULL,
	`created_at` integer,
	`updated_at` integer,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE cascade
);
CREATE UNIQUE INDEX IF NOT EXISTS `sessions_hashed_token_unique` ON `sessions` (`hashed_token`);
CREATE INDEX IF NOT EXISTS `sessions_user_idx` ON `sessions` (`user_id`);
CREATE TABLE IF NOT EXISTS `password_reset_tokens` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`hashed_token` text NOT NULL,
	`expires_at` integer NOT NULL,
	`used_at` integer,
	`created_at` integer,
	`updated_at` integer,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE cascade
);
CREATE UNIQUE INDEX IF NOT EXISTS `password_reset_tokens_hashed_token_unique` ON `password_reset_tokens` (`hashed_token`);
CREATE INDEX IF NOT EXISTS `password_reset_tokens_user_idx` ON `password_reset_tokens` (`user_id`);
