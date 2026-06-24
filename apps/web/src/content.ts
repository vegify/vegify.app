// Server-side assembly for the P1 content API (server source of truth — [[server-source-of-truth]]).
// Produces the DESKTOP DAL's exact JSON shapes from @vegify/db, so the desktop client + its offline
// cache speak one shape and the desktop's existing view-model mappers keep working. @vegify/db is
// dynamic-imported inside each fn (server-only — keeps the libsql client out of the client bundle,
// matching the route handlers). Reads are scoped per-viewer; the routes pass the session user id.

type Visibility = 'public' | 'private' | 'unlisted'
type Reading = { name: string; amountPer100g: number; unit: string }
type Amount = { amount: number | null; unit: string | null; grams: number }
type AggregatedNutrition = { caloriesPer100g: number | null; readings: Reading[] }

export type RecipeCard = { id: string; name: string; subtitle: string | null }
export type RecipeItem = { id: string; name: string; amount: Amount; recipeId: string | null }
export type RecipeView = {
  id: string
  name: string
  subtitle: string | null
  directions: string | null
  creator: string | null
  serving: Amount | null
  batchGrams: number | null
  items: RecipeItem[]
  nutrition: AggregatedNutrition
}
export type RecipeEditItem = {
  ingredientId: string
  name: string
  grams: number
  caloriesPer100g: number | null
  readings: Reading[]
}
export type RecipeEditData = {
  id: string
  name: string
  subtitle: string | null
  directions: string | null
  servings: number | null
  visibility: Visibility
  items: RecipeEditItem[]
}
export type IngredientCard = { id: string; name: string; caloriesPer100g: number | null }
export type IngredientEditData = {
  id: string
  name: string
  description: string | null
  price: number | null
  caloriesPer100g: number | null
  servingGrams: number | null
  packageGrams: number | null
  visibility: Visibility
  nutrients: Reading[]
}

export async function listRecipeCards(me: string | null): Promise<RecipeCard[]> {
  const { db, isListed } = await import('@vegify/db')
  const recipes = await db.query.recipes.findMany({ with: { asIngredient: true } })
  return recipes
    .filter((r) => isListed(r.asIngredient.visibility, r.asIngredient.userId, me))
    .map((r) => ({ id: r.id, name: r.asIngredient.name, subtitle: r.subtitle }))
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
}

export async function getRecipeView(id: string, me: string | null): Promise<RecipeView | null> {
  const { db, getRecipeNutrition, canView } = await import('@vegify/db')
  const recipe = await db.query.recipes.findFirst({
    where: (r, { eq }) => eq(r.id, id),
    with: {
      asIngredient: { with: { creator: true, servingSize: true, batchSize: true } },
      items: { orderBy: (iir, { asc }) => [asc(iir.order)], with: { ingredient: true, amount: true } },
    },
  })
  if (!recipe) return null
  if (!canView(recipe.asIngredient.visibility, recipe.asIngredient.userId, me)) return null
  const agg = await getRecipeNutrition(id)
  // An item whose ingredient is itself a recipe's as-ingredient links to that recipe, not the
  // ingredient page (mirrors the desktop) — resolve those in one extra query.
  const itemIngredientIds = recipe.items
    .map((it) => it.ingredient?.id)
    .filter((x): x is string => Boolean(x))
  const subRecipes = itemIngredientIds.length
    ? await db.query.recipes.findMany({
        columns: { id: true, asIngredientId: true },
        where: (r, { inArray }) => inArray(r.asIngredientId, itemIngredientIds),
      })
    : []
  const recipeByIngredient = new Map(subRecipes.map((r) => [r.asIngredientId, r.id]))
  const s = recipe.asIngredient.servingSize
  return {
    id: recipe.id,
    name: recipe.asIngredient.name,
    subtitle: recipe.subtitle,
    directions: recipe.directions,
    creator: recipe.asIngredient.creator?.name ?? null,
    serving: s ? { amount: s.amount, unit: s.unit, grams: s.grams } : null,
    batchGrams: recipe.asIngredient.batchSize?.grams ?? null,
    items: recipe.items.map((item) => {
      const ing = item.ingredient
      const a = item.amount
      return {
        id: item.id,
        name: ing?.name ?? '(unknown)',
        amount: { amount: a?.amount ?? null, unit: a?.unit ?? null, grams: a?.grams ?? 0 },
        recipeId: (ing && recipeByIngredient.get(ing.id)) ?? null,
      }
    }),
    nutrition: agg,
  }
}

export async function getRecipeEdit(id: string, me: string | null): Promise<RecipeEditData | null> {
  const { db, getIngredientNutrition, isOwner } = await import('@vegify/db')
  const recipe = await db.query.recipes.findFirst({
    where: (r, { eq }) => eq(r.id, id),
    with: {
      asIngredient: { with: { servingSize: true, batchSize: true } },
      items: { orderBy: (iir, { asc }) => [asc(iir.order)], with: { ingredient: true, amount: true } },
    },
  })
  if (!recipe) return null
  if (!isOwner(recipe.asIngredient.userId, me)) return null
  const items: RecipeEditItem[] = []
  for (const it of recipe.items) {
    if (!it.ingredient) continue
    const n = await getIngredientNutrition(it.ingredient.id)
    items.push({
      ingredientId: it.ingredient.id,
      name: it.ingredient.name,
      grams: it.amount?.grams ?? 0,
      caloriesPer100g: n.caloriesPer100g,
      readings: n.readings,
    })
  }
  const sg = recipe.asIngredient.servingSize?.grams ?? null
  const bg = recipe.asIngredient.batchSize?.grams ?? null
  return {
    id: recipe.id,
    name: recipe.asIngredient.name,
    subtitle: recipe.subtitle,
    directions: recipe.directions,
    servings: sg && bg ? bg / sg : null,
    visibility: recipe.asIngredient.visibility,
    items,
  }
}

export async function listIngredientCards(me: string | null): Promise<IngredientCard[]> {
  const { db, isListed } = await import('@vegify/db')
  const [all, recipes] = await Promise.all([
    db.query.ingredients.findMany({ orderBy: (i, { asc }) => asc(i.name) }),
    db.query.recipes.findMany({ columns: { asIngredientId: true } }),
  ])
  const recipeIngredientIds = new Set(recipes.map((r) => r.asIngredientId))
  return all
    .filter((i) => !recipeIngredientIds.has(i.id) && isListed(i.visibility, i.userId, me))
    .map((i) => ({ id: i.id, name: i.name, caloriesPer100g: i.caloriesPer100g }))
}

async function loadIngredient(id: string) {
  const { db } = await import('@vegify/db')
  return db.query.ingredients.findFirst({
    where: (i, { eq }) => eq(i.id, id),
    with: { servingSize: true, batchSize: true, nutrients: { with: { nutrient: true } } },
  })
}

function toIngredientEditData(
  ing: NonNullable<Awaited<ReturnType<typeof loadIngredient>>>,
): IngredientEditData {
  return {
    id: ing.id,
    name: ing.name,
    description: ing.description,
    price: ing.price,
    caloriesPer100g: ing.caloriesPer100g,
    servingGrams: ing.servingSize?.grams ?? null,
    packageGrams: ing.batchSize?.grams ?? null,
    visibility: ing.visibility,
    nutrients: ing.nutrients.map((n) => ({
      name: n.nutrient.name,
      amountPer100g: n.amountPer100g,
      unit: n.unit,
    })),
  }
}

/** Ingredient detail (canView): public/unlisted/own. Mirrors the desktop `ingredient` proc. */
export async function getIngredientView(id: string, me: string | null): Promise<IngredientEditData | null> {
  const { canView } = await import('@vegify/db')
  const ing = await loadIngredient(id)
  if (!ing) return null
  if (!canView(ing.visibility, ing.userId, me)) return null
  return toIngredientEditData(ing)
}

/** Ingredient edit-load (isOwner). Mirrors the desktop `ingredient_for_edit` proc. */
export async function getIngredientEdit(id: string, me: string | null): Promise<IngredientEditData | null> {
  const { isOwner } = await import('@vegify/db')
  const ing = await loadIngredient(id)
  if (!ing) return null
  if (!isOwner(ing.userId, me)) return null
  return toIngredientEditData(ing)
}
