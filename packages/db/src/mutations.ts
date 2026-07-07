import { eq, inArray } from "drizzle-orm";
import { isOwner, type Visibility } from "./access";
import { db } from "./index";
import { one } from "./rows";
import {
  amounts,
  ingredientInRecipe,
  ingredientNutrient,
  ingredients,
  nutrients,
  recipes,
} from "./schema";

// Shared domain mutations. Each app wraps these in its own framework-idiomatic
// transport (Next server action / TanStack createServerFn) — the logic lives once.

export type IngredientNutrientInput = {
  name: string;
  amountPer100g: number;
  unit: string;
};

export type SaveIngredientInput = {
  id?: string;
  userId?: string | null;
  visibility?: Visibility;
  name: string;
  description?: string | null;
  price?: number | null; // cents
  caloriesPer100g?: number | null;
  servingGrams?: number | null;
  packageGrams?: number | null;
  nutrients: IngredientNutrientInput[];
};

async function upsertAmount(
  id: string | null | undefined,
  grams: number | null | undefined,
  unit: string,
) {
  if (grams == null) return id ?? null;
  if (id) {
    await db.update(amounts).set({ grams, unit }).where(eq(amounts.id, id));
    return id;
  }
  const a = one(
    await db
      .insert(amounts)
      .values({ grams, unit, amount: 1, preferred: "grams" })
      .returning(),
  );
  return a.id;
}

// Every `amounts` FK (serving/batch size, recipe-item amount) cascades amount→owner, NOT
// owner→amount. So deleting an owner row orphans its amounts — they must be deleted by id.
async function deleteAmounts(ids: (string | null | undefined)[]) {
  const real = ids.filter((x): x is string => x != null);
  if (real.length) await db.delete(amounts).where(inArray(amounts.id, real));
}

async function findOrCreateNutrient(name: string) {
  const existing = await db.query.nutrients.findFirst({
    where: (t, { eq }) => eq(t.name, name),
  });
  if (existing) return existing.id;
  const n = one(await db.insert(nutrients).values({ name }).returning());
  return n.id;
}

export async function saveIngredient(
  input: SaveIngredientInput,
): Promise<string> {
  // Upsert by id: an existing row updates; a provided-but-absent id inserts WITH that id (an offline
  // create's client ULID, or a pulled row). `userId` is set only on insert — an edit never reassigns
  // ownership. The owner guard applies only when the row already exists.
  const existingId = input.id;
  const existing = existingId
    ? await db.query.ingredients.findFirst({
        where: (t, { eq }) => eq(t.id, existingId),
      })
    : null;
  if (existing && !isOwner(existing.userId, input.userId))
    throw new Error("You can only edit your own ingredients.");
  const servingSizeId = await upsertAmount(
    existing?.servingSizeId,
    input.servingGrams,
    "serving",
  );
  const batchSizeId = await upsertAmount(
    existing?.batchSizeId,
    input.packageGrams,
    "package",
  );

  let ingredientId: string;
  if (existing) {
    await db
      .update(ingredients)
      .set({
        name: input.name,
        description: input.description ?? null,
        price: input.price ?? null,
        caloriesPer100g: input.caloriesPer100g ?? null,
        visibility: input.visibility ?? "public",
        servingSizeId,
        batchSizeId,
      })
      .where(eq(ingredients.id, existing.id));
    ingredientId = existing.id;
    await db
      .delete(ingredientNutrient)
      .where(eq(ingredientNutrient.ingredientId, ingredientId));
  } else {
    const row = one(
      await db
        .insert(ingredients)
        .values({
          id: input.id, // honor a client-supplied id; undefined → drizzle mints a ULID
          userId: input.userId ?? null,
          visibility: input.visibility ?? "public",
          name: input.name,
          description: input.description ?? null,
          isVegan: true,
          price: input.price ?? null,
          caloriesPer100g: input.caloriesPer100g ?? null,
          servingSizeId,
          batchSizeId,
        })
        .returning(),
    );
    ingredientId = row.id;
  }

  const seen = new Set<string>();
  for (const n of input.nutrients) {
    if (!n.name.trim()) continue;
    const nutrientId = await findOrCreateNutrient(n.name.trim());
    if (seen.has(nutrientId)) continue; // (ingredientId, nutrientId) is unique
    seen.add(nutrientId);
    await db.insert(ingredientNutrient).values({
      ingredientId,
      nutrientId,
      amountPer100g: n.amountPer100g,
      unit: n.unit || "g",
    });
  }
  return ingredientId;
}

export async function deleteIngredient(
  id: string,
  userId?: string | null,
): Promise<void> {
  // ingredient_in_recipe / ingredient_img are onDelete:"restrict" — deleting an in-use
  // ingredient throws (intended). Its serving/batch `amounts` are not cascaded, so clean
  // them up after the row is gone.
  const existing = await db.query.ingredients.findFirst({
    where: (t, { eq }) => eq(t.id, id),
  });
  if (!existing) return;
  if (!isOwner(existing.userId, userId))
    throw new Error("You can only delete your own ingredients.");
  await db.delete(ingredients).where(eq(ingredients.id, id));
  await deleteAmounts([existing.servingSizeId, existing.batchSizeId]);
}

// --- recipes ---

export type RecipeItemInput = {
  ingredientId: string;
  grams: number;
  unit?: string | null;
};
export type SaveRecipeInput = {
  id?: string;
  asIngredientId?: string;
  userId?: string | null;
  visibility?: Visibility;
  name: string;
  subtitle?: string | null;
  directions?: string | null;
  servingGrams?: number | null;
  batchGrams?: number | null;
  items: RecipeItemInput[];
};

export async function saveRecipe(input: SaveRecipeInput): Promise<string> {
  // Upsert by id (see saveIngredient). A provided-but-absent recipe id inserts WITH that id (offline
  // create / pulled row). The as-ingredient id is threaded too (input.asIngredientId): a nested
  // recipe is consumed by id as a recipe item (a Biga inside a Dough), so its as-ingredient id must
  // also stay stable cross-replica or the consuming item's FK orphans after a pull. Owner guard only
  // when the row already exists.
  const existingId = input.id;
  const existing = existingId
    ? await db.query.recipes.findFirst({
        where: (r, { eq }) => eq(r.id, existingId),
        with: { asIngredient: true },
      })
    : null;
  if (existing && !isOwner(existing.asIngredient.userId, input.userId))
    throw new Error("You can only edit your own recipes.");

  let recipeId: string;
  if (existing) {
    const servingSizeId = await upsertAmount(
      existing.asIngredient.servingSizeId,
      input.servingGrams,
      "serving",
    );
    const batchSizeId = await upsertAmount(
      existing.asIngredient.batchSizeId,
      input.batchGrams,
      "batch",
    );
    await db
      .update(ingredients)
      .set({
        name: input.name,
        visibility: input.visibility ?? "public",
        servingSizeId,
        batchSizeId,
      })
      .where(eq(ingredients.id, existing.asIngredientId));
    await db
      .update(recipes)
      .set({
        subtitle: input.subtitle ?? null,
        directions: input.directions ?? null,
      })
      .where(eq(recipes.id, existing.id));
    recipeId = existing.id;
    // Re-attach the item list from scratch. The join rows delete fine, but the per-item
    // `amounts` they point to are not cascaded — capture and delete them, else every edit
    // leaks one `amounts` row per ingredient.
    const prevItems = await db
      .select({ amountId: ingredientInRecipe.amountId })
      .from(ingredientInRecipe)
      .where(eq(ingredientInRecipe.recipeId, recipeId));
    await db
      .delete(ingredientInRecipe)
      .where(eq(ingredientInRecipe.recipeId, recipeId));
    await deleteAmounts(prevItems.map((r) => r.amountId));
  } else {
    const servingSizeId = await upsertAmount(
      null,
      input.servingGrams,
      "serving",
    );
    const batchSizeId = await upsertAmount(null, input.batchGrams, "batch");
    const ing = one(
      await db
        .insert(ingredients)
        .values({
          id: input.asIngredientId, // honor a client/pull-supplied as-ingredient id; undefined → mint
          userId: input.userId ?? null,
          visibility: input.visibility ?? "public",
          name: input.name,
          isVegan: true,
          servingSizeId,
          batchSizeId,
        })
        .returning(),
    );
    const rec = one(
      await db
        .insert(recipes)
        .values({
          id: input.id, // honor a client-supplied recipe id; undefined → drizzle mints
          asIngredientId: ing.id,
          subtitle: input.subtitle ?? null,
          directions: input.directions ?? null,
        })
        .returning(),
    );
    recipeId = rec.id;
  }

  let order = 0;
  for (const item of input.items) {
    if (!item.ingredientId || !item.grams) continue;
    const a = one(
      await db
        .insert(amounts)
        .values({
          grams: item.grams,
          amount: item.grams,
          unit: item.unit ?? "g",
          preferred: "grams",
        })
        .returning(),
    );
    await db.insert(ingredientInRecipe).values({
      order: order++,
      recipeId,
      ingredientId: item.ingredientId,
      amountId: a.id,
    });
  }
  return recipeId;
}

export async function deleteRecipe(
  id: string,
  userId?: string | null,
): Promise<void> {
  const recipe = await db.query.recipes.findFirst({
    where: (r, { eq }) => eq(r.id, id),
    with: { asIngredient: true },
  });
  if (!recipe) return;
  if (!isOwner(recipe.asIngredient.userId, userId))
    throw new Error("You can only delete your own recipes.");

  // Capture the item amounts before the recipe delete cascades the join rows away (the
  // amounts themselves are not cascaded — FK is amount→join).
  const items = await db
    .select({ amountId: ingredientInRecipe.amountId })
    .from(ingredientInRecipe)
    .where(eq(ingredientInRecipe.recipeId, id));

  await db.delete(recipes).where(eq(recipes.id, id)); // cascades ingredient_in_recipe
  await deleteAmounts(items.map((r) => r.amountId));

  // The as-ingredient row is not cascaded (recipe→ingredient is the wrong direction). Delete
  // it — and its serving/batch amounts — only when no other recipe still consumes it as an
  // item (ingredient_in_recipe is onDelete:"restrict"; e.g. a Biga used by a dough is kept).
  const stillConsumed = await db.query.ingredientInRecipe.findFirst({
    where: (t, { eq }) => eq(t.ingredientId, recipe.asIngredientId),
  });
  if (!stillConsumed) {
    await db
      .delete(ingredients)
      .where(eq(ingredients.id, recipe.asIngredientId)); // cascades ingredient_nutrient
    await deleteAmounts([
      recipe.asIngredient.servingSizeId,
      recipe.asIngredient.batchSizeId,
    ]);
  }
}
