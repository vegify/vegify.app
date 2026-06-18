import { eq, inArray } from "drizzle-orm";
import { db } from "./index";
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
  id?: number;
  userId?: number | null;
  name: string;
  description?: string | null;
  price?: number | null; // cents
  caloriesPer100g?: number | null;
  servingGrams?: number | null;
  packageGrams?: number | null;
  nutrients: IngredientNutrientInput[];
};

async function upsertAmount(
  id: number | null | undefined,
  grams: number | null | undefined,
  unit: string,
) {
  if (grams == null) return id ?? null;
  if (id) {
    await db.update(amounts).set({ grams, unit }).where(eq(amounts.id, id));
    return id;
  }
  const [a] = await db
    .insert(amounts)
    .values({ grams, unit, amount: 1, preferred: "grams" })
    .returning();
  return a.id;
}

// Every `amounts` FK (serving/batch size, recipe-item amount) cascades amount→owner, NOT
// owner→amount. So deleting an owner row orphans its amounts — they must be deleted by id.
async function deleteAmounts(ids: (number | null | undefined)[]) {
  const real = ids.filter((x): x is number => x != null);
  if (real.length) await db.delete(amounts).where(inArray(amounts.id, real));
}

async function findOrCreateNutrient(name: string) {
  const existing = await db.query.nutrients.findFirst({
    where: (t, { eq }) => eq(t.name, name),
  });
  if (existing) return existing.id;
  const [n] = await db.insert(nutrients).values({ name }).returning();
  return n.id;
}

export async function saveIngredient(input: SaveIngredientInput): Promise<number> {
  let ingredientId: number;

  if (input.id) {
    const existing = await db.query.ingredients.findFirst({
      where: (t, { eq }) => eq(t.id, input.id!),
    });
    const servingSizeId = await upsertAmount(existing?.servingSizeId, input.servingGrams, "serving");
    const batchSizeId = await upsertAmount(existing?.batchSizeId, input.packageGrams, "package");
    await db
      .update(ingredients)
      .set({
        name: input.name,
        description: input.description ?? null,
        price: input.price ?? null,
        caloriesPer100g: input.caloriesPer100g ?? null,
        servingSizeId,
        batchSizeId,
      })
      .where(eq(ingredients.id, input.id));
    ingredientId = input.id;
    await db.delete(ingredientNutrient).where(eq(ingredientNutrient.ingredientId, ingredientId));
  } else {
    const servingSizeId = await upsertAmount(null, input.servingGrams, "serving");
    const batchSizeId = await upsertAmount(null, input.packageGrams, "package");
    const [row] = await db
      .insert(ingredients)
      .values({
        userId: input.userId ?? null,
        name: input.name,
        description: input.description ?? null,
        isVegan: true,
        price: input.price ?? null,
        caloriesPer100g: input.caloriesPer100g ?? null,
        servingSizeId,
        batchSizeId,
      })
      .returning();
    ingredientId = row.id;
  }

  const seen = new Set<number>();
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

export async function deleteIngredient(id: number): Promise<void> {
  // ingredient_in_recipe / ingredient_img are onDelete:"restrict" — deleting an in-use
  // ingredient throws (intended). Its serving/batch `amounts` are not cascaded, so clean
  // them up after the row is gone.
  const existing = await db.query.ingredients.findFirst({
    where: (t, { eq }) => eq(t.id, id),
  });
  if (!existing) return;
  await db.delete(ingredients).where(eq(ingredients.id, id));
  await deleteAmounts([existing.servingSizeId, existing.batchSizeId]);
}

// --- recipes ---

export type RecipeItemInput = { ingredientId: number; grams: number; unit?: string | null };
export type SaveRecipeInput = {
  id?: number;
  userId?: number | null;
  name: string;
  subtitle?: string | null;
  directions?: string | null;
  servingGrams?: number | null;
  batchGrams?: number | null;
  items: RecipeItemInput[];
};

export async function saveRecipe(input: SaveRecipeInput): Promise<number> {
  let recipeId: number;

  if (input.id) {
    const existing = await db.query.recipes.findFirst({
      where: (r, { eq }) => eq(r.id, input.id!),
      with: { asIngredient: true },
    });
    if (!existing) throw new Error(`recipe ${input.id} not found`);
    const servingSizeId = await upsertAmount(existing.asIngredient.servingSizeId, input.servingGrams, "serving");
    const batchSizeId = await upsertAmount(existing.asIngredient.batchSizeId, input.batchGrams, "batch");
    await db
      .update(ingredients)
      .set({ name: input.name, servingSizeId, batchSizeId })
      .where(eq(ingredients.id, existing.asIngredientId));
    await db
      .update(recipes)
      .set({ subtitle: input.subtitle ?? null, directions: input.directions ?? null })
      .where(eq(recipes.id, input.id));
    recipeId = input.id;
    // Re-attach the item list from scratch. The join rows delete fine, but the per-item
    // `amounts` they point to are not cascaded — capture and delete them, else every edit
    // leaks one `amounts` row per ingredient.
    const prevItems = await db
      .select({ amountId: ingredientInRecipe.amountId })
      .from(ingredientInRecipe)
      .where(eq(ingredientInRecipe.recipeId, recipeId));
    await db.delete(ingredientInRecipe).where(eq(ingredientInRecipe.recipeId, recipeId));
    await deleteAmounts(prevItems.map((r) => r.amountId));
  } else {
    const servingSizeId = await upsertAmount(null, input.servingGrams, "serving");
    const batchSizeId = await upsertAmount(null, input.batchGrams, "batch");
    const [ing] = await db
      .insert(ingredients)
      .values({
        userId: input.userId ?? null,
        name: input.name,
        isVegan: true,
        servingSizeId,
        batchSizeId,
      })
      .returning();
    const [rec] = await db
      .insert(recipes)
      .values({
        asIngredientId: ing.id,
        subtitle: input.subtitle ?? null,
        directions: input.directions ?? null,
      })
      .returning();
    recipeId = rec.id;
  }

  let order = 0;
  for (const item of input.items) {
    if (!item.ingredientId || !item.grams) continue;
    const [a] = await db
      .insert(amounts)
      .values({ grams: item.grams, amount: item.grams, unit: item.unit ?? "g", preferred: "grams" })
      .returning();
    await db.insert(ingredientInRecipe).values({
      order: order++,
      recipeId,
      ingredientId: item.ingredientId,
      amountId: a.id,
    });
  }
  return recipeId;
}

export async function deleteRecipe(id: number): Promise<void> {
  const recipe = await db.query.recipes.findFirst({
    where: (r, { eq }) => eq(r.id, id),
    with: { asIngredient: true },
  });
  if (!recipe) return;

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
    await db.delete(ingredients).where(eq(ingredients.id, recipe.asIngredientId)); // cascades ingredient_nutrient
    await deleteAmounts([recipe.asIngredient.servingSizeId, recipe.asIngredient.batchSizeId]);
  }
}
