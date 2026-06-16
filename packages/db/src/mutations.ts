import { eq } from "drizzle-orm";
import { db } from "./index";
import { amounts, ingredientNutrient, ingredients, nutrients } from "./schema";

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
  // ingredient_in_recipe has onDelete:"restrict" — deleting an in-use ingredient throws.
  await db.delete(ingredients).where(eq(ingredients.id, id));
}
