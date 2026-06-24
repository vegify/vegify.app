import { createFileRoute, notFound } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { RecipeDetailView, type NutritionFactsData, type RecipeDetailVM } from '@vegify/ui'
import { LinkAdapter } from '../link'

const getRecipe = createServerFn({ method: 'GET' })
  .validator((recipeId: string) => recipeId)
  .handler(async ({ data }): Promise<RecipeDetailVM> => {
    const { db, getRecipeNutrition } = await import('@vegify/db')
    const id = data
    const recipe = await db.query.recipes.findFirst({
      where: (r, { eq }) => eq(r.id, id),
      with: {
        asIngredient: { with: { creator: true, servingSize: true, batchSize: true } },
        items: {
          orderBy: (iir, { asc }) => [asc(iir.order)],
          with: { ingredient: true, amount: true },
        },
      },
    })
    if (!recipe) throw notFound()
    const agg = await getRecipeNutrition(id)

    // An item whose ingredient is itself a recipe's as-ingredient links to the recipe page, not
    // the ingredient page (mirrors the desktop) — resolve those in one extra query.
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

    const serving = recipe.asIngredient.servingSize
    const nutrition: NutritionFactsData = {
      heading: 'This Recipe',
      serving: serving ? { amount: serving.amount, unit: serving.unit, grams: serving.grams } : null,
      servingsPerBatch:
        recipe.asIngredient.batchSize && serving?.grams
          ? recipe.asIngredient.batchSize.grams / serving.grams
          : null,
      caloriesPerServing:
        agg.caloriesPer100g != null && serving?.grams
          ? (agg.caloriesPer100g * serving.grams) / 100
          : agg.caloriesPer100g,
      readings: agg.readings,
    }

    return {
      id: recipe.id,
      name: recipe.asIngredient.name,
      subtitle: recipe.subtitle,
      creator: recipe.asIngredient.creator?.name,
      directions: recipe.directions,
      items: recipe.items.map((item) => {
        const ing = item.ingredient
        const subRecipeId = ing ? recipeByIngredient.get(ing.id) : undefined
        return {
          key: item.id,
          label: `${item.amount?.amount ?? ''} ${item.amount?.unit ?? ''} ${ing?.name ?? '(unknown)'}`.trim(),
          href: subRecipeId ? `/recipes/${subRecipeId}` : `/ingredients/${ing?.id ?? ''}`,
        }
      }),
      nutrition,
    }
  })

export const Route = createFileRoute('/recipes/$recipeId/')({
  loader: ({ params }) => getRecipe({ data: params.recipeId }),
  component: RecipePage,
})

function RecipePage() {
  return <RecipeDetailView recipe={Route.useLoaderData()} LinkComponent={LinkAdapter} />
}
