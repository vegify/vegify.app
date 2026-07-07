import { client, db } from "./index"

// Aggregate a recipe's nutrition from its ingredients, recursively: a recipe IS an
// ingredient (recipes.as_ingredient_id), so a recipe-ingredient's per-100g values are
// the weighted average of its items, each of which may itself be a recipe (e.g. the
// Biga inside the Neapolitan dough). Leaf ingredients carry ingredient_nutrient +
// caloriesPer100g directly.
//
// This is computed with ONE recursive CTE (in-DB graph walk) rather than a per-ingredient
// query cascade. `expand` starts at the target ingredient and walks down to leaves, carrying
// each leaf's effective grams within the whole batch (eff_grams) and the batch total (denom);
// per-100g of the (possibly nested) recipe-as-ingredient is then SUM(per100g_leaf * eff_grams)
// / denom. A leaf ingredient anchors with eff_grams = denom = 1, so it returns its own per-100g.
// The single CTE costs ~the same for a 20-ingredient recipe as for a 4-ingredient one, where the
// former per-ingredient recursion was O(ingredients × depth) round-trips (N+1).

export type AggregatedNutrition = {
  caloriesPer100g: number | null
  readings: { name: string; amountPer100g: number; unit: string }[]
}

const CTE = `
WITH RECURSIVE
recipe_total AS (
  SELECT r.id AS recipe_id, r.as_ingredient_id AS as_ingredient_id, SUM(a.grams) AS total_grams
  FROM recipes r
  JOIN ingredient_in_recipe iir ON iir.recipe_id = r.id
  JOIN amounts a ON a.id = iir.amount_id
  GROUP BY r.id
),
expand(ingredient_id, eff_grams, denom, depth) AS (
  -- anchor: the target ingredient. If it's a recipe-as-ingredient, eff_grams = denom = its batch
  -- total; if it's a leaf (no recipe_total row), eff_grams = denom = 1 so it returns its own per-100g.
  SELECT i.id, COALESCE(rt.total_grams, 1.0), COALESCE(rt.total_grams, 1.0), 0
  FROM ingredients i
  LEFT JOIN recipe_total rt ON rt.as_ingredient_id = i.id
  WHERE i.id = :id
  UNION ALL
  -- expand any row that is itself a recipe into its items, scaling grams into the batch.
  SELECT iir.ingredient_id, e.eff_grams * a.grams / rt.total_grams, e.denom, e.depth + 1
  FROM expand e
  JOIN recipes r ON r.as_ingredient_id = e.ingredient_id
  JOIN recipe_total rt ON rt.recipe_id = r.id
  JOIN ingredient_in_recipe iir ON iir.recipe_id = r.id
  JOIN amounts a ON a.id = iir.amount_id
  WHERE e.depth < 32 AND rt.total_grams > 0
)
SELECT 'cal' AS kind, NULL AS name, NULL AS unit,
       SUM(i.calories_per_100g * e.eff_grams / e.denom) AS per100g
FROM expand e
JOIN ingredients i ON i.id = e.ingredient_id
WHERE i.calories_per_100g IS NOT NULL
UNION ALL
SELECT 'nut' AS kind, n.name AS name, inu.unit AS unit,
       SUM(inu.amount_per_100g * e.eff_grams / e.denom) AS per100g
FROM expand e
JOIN ingredient_nutrient inu ON inu.ingredient_id = e.ingredient_id
JOIN nutrients n ON n.id = inu.nutrient_id
GROUP BY n.id
ORDER BY name`

/** Effective per-100g nutrition of any ingredient (leaf or recipe) — one recursive CTE. */
async function per100gForIngredient(
  ingredientId: string
): Promise<AggregatedNutrition> {
  const rs = await client.execute({ sql: CTE, args: { id: ingredientId } })
  let caloriesPer100g: number | null = null
  const readings: AggregatedNutrition["readings"] = []
  for (const row of rs.rows) {
    const v = row.per100g == null ? null : Number(row.per100g)
    if (row.kind === "cal") {
      caloriesPer100g = v
    } else if (v != null) {
      readings.push({
        name: String(row.name),
        amountPer100g: v,
        unit: String(row.unit)
      })
    }
  }
  return { caloriesPer100g, readings }
}

export async function getRecipeNutrition(
  recipeId: string
): Promise<AggregatedNutrition> {
  const recipe = await db.query.recipes.findFirst({
    where: (r, { eq }) => eq(r.id, recipeId)
  })
  if (!recipe) return { caloriesPer100g: null, readings: [] }
  return per100gForIngredient(recipe.asIngredientId)
}

/** Effective per-100g nutrition of any ingredient (leaf or recipe). */
export async function getIngredientNutrition(
  ingredientId: string
): Promise<AggregatedNutrition> {
  return per100gForIngredient(ingredientId)
}

export type IngredientSearchResult = AggregatedNutrition & {
  id: string
  name: string
  servingGrams: number | null
}

/** Search ingredients by name, each with its effective per-100g nutrition (for live recipe aggregation). */
export async function searchIngredients(
  query: string,
  userId?: string | null,
  limit = 12
): Promise<IngredientSearchResult[]> {
  const q = query.trim()
  const rows = await db.query.ingredients.findMany({
    // public catalog + your own (any visibility), name-filtered
    where: (i, { and, or, eq, like }) =>
      and(
        q ? like(i.name, `%${q}%`) : undefined,
        or(
          eq(i.visibility, "public"),
          userId ? eq(i.userId, userId) : undefined
        )
      ),
    with: { servingSize: true },
    orderBy: (i, { asc }) => [asc(i.name)],
    limit
  })
  const out: IngredientSearchResult[] = []
  for (const r of rows) {
    const n = await getIngredientNutrition(r.id)
    out.push({
      id: r.id,
      name: r.name,
      servingGrams: r.servingSize?.grams ?? null,
      ...n
    })
  }
  return out
}
