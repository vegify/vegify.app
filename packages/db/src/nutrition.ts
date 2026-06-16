import { db } from "./index";

// Aggregate a recipe's nutrition from its ingredients, recursively: a recipe IS an
// ingredient (recipes.as_ingredient_id), so a recipe-ingredient's per-100g values are
// the weighted average of its items, each of which may itself be a recipe (e.g. the
// Biga inside the Neapolitan dough). Leaf ingredients carry ingredient_nutrient +
// caloriesPer100g directly.

export type AggregatedNutrition = {
  caloriesPer100g: number | null;
  readings: { name: string; amountPer100g: number; unit: string }[];
};

type Acc = {
  cal: number;
  calKnown: boolean;
  byNutrient: Map<string, { amt: number; unit: string }>;
};

async function per100gForIngredient(ingredientId: number, seen: Set<number>): Promise<Acc> {
  if (seen.has(ingredientId)) return { cal: 0, calKnown: false, byNutrient: new Map() };
  const seen2 = new Set(seen);
  seen2.add(ingredientId);

  // Is this ingredient actually a recipe? Then aggregate over its items.
  const recipe = await db.query.recipes.findFirst({
    where: (r, { eq }) => eq(r.asIngredientId, ingredientId),
    with: { items: { with: { ingredient: true, amount: true } } },
  });

  if (recipe) {
    let totalGrams = 0;
    let calTotal = 0;
    let calKnown = false;
    const acc = new Map<string, { amt: number; unit: string }>();
    for (const item of recipe.items) {
      if (!item.ingredient) continue;
      const grams = item.amount?.grams ?? 0;
      if (!grams) continue;
      totalGrams += grams;
      const child = await per100gForIngredient(item.ingredient.id, seen2);
      if (child.calKnown) {
        calTotal += (child.cal * grams) / 100;
        calKnown = true;
      }
      for (const [name, { amt, unit }] of child.byNutrient) {
        const prev = acc.get(name) ?? { amt: 0, unit };
        acc.set(name, { amt: prev.amt + (amt * grams) / 100, unit: prev.unit });
      }
    }
    const per100 = new Map<string, { amt: number; unit: string }>();
    for (const [name, { amt, unit }] of acc) {
      per100.set(name, { amt: totalGrams ? (amt / totalGrams) * 100 : 0, unit });
    }
    return {
      cal: calKnown && totalGrams ? (calTotal / totalGrams) * 100 : 0,
      calKnown,
      byNutrient: per100,
    };
  }

  // Leaf ingredient: its own per-100g data.
  const ing = await db.query.ingredients.findFirst({
    where: (i, { eq }) => eq(i.id, ingredientId),
    with: { nutrients: { with: { nutrient: true } } },
  });
  const byNutrient = new Map<string, { amt: number; unit: string }>();
  for (const n of ing?.nutrients ?? []) {
    byNutrient.set(n.nutrient.name, { amt: n.amountPer100g, unit: n.unit });
  }
  return {
    cal: ing?.caloriesPer100g ?? 0,
    calKnown: ing?.caloriesPer100g != null,
    byNutrient,
  };
}

function toAggregated(agg: Acc): AggregatedNutrition {
  return {
    caloriesPer100g: agg.calKnown ? agg.cal : null,
    readings: [...agg.byNutrient].map(([name, { amt, unit }]) => ({
      name,
      amountPer100g: amt,
      unit,
    })),
  };
}

export async function getRecipeNutrition(recipeId: number): Promise<AggregatedNutrition> {
  const recipe = await db.query.recipes.findFirst({
    where: (r, { eq }) => eq(r.id, recipeId),
  });
  if (!recipe) return { caloriesPer100g: null, readings: [] };
  return toAggregated(await per100gForIngredient(recipe.asIngredientId, new Set()));
}

/** Effective per-100g nutrition of any ingredient (leaf or recipe). */
export async function getIngredientNutrition(ingredientId: number): Promise<AggregatedNutrition> {
  return toAggregated(await per100gForIngredient(ingredientId, new Set()));
}

export type IngredientSearchResult = AggregatedNutrition & {
  id: number;
  name: string;
  servingGrams: number | null;
};

/** Search ingredients by name, each with its effective per-100g nutrition (for live recipe aggregation). */
export async function searchIngredients(
  query: string,
  limit = 12,
): Promise<IngredientSearchResult[]> {
  const q = query.trim();
  const rows = await db.query.ingredients.findMany({
    where: q ? (i, { like }) => like(i.name, `%${q}%`) : undefined,
    with: { servingSize: true },
    orderBy: (i, { asc }) => [asc(i.name)],
    limit,
  });
  const out: IngredientSearchResult[] = [];
  for (const r of rows) {
    const n = await getIngredientNutrition(r.id);
    out.push({ id: r.id, name: r.name, servingGrams: r.servingSize?.grams ?? null, ...n });
  }
  return out;
}
