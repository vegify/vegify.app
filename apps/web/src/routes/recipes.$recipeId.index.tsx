import { createFileRoute, notFound } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { queryOptions, useSuspenseQuery } from '@tanstack/react-query'
import { RecipeDetailView, type NutritionFactsData, type RecipeDetailVM } from '@vegify/ui'
import { LinkAdapter } from '../link'

const getRecipe = createServerFn({ method: 'GET' })
  .validator((recipeId: string) => recipeId)
  .handler(async ({ data }): Promise<RecipeDetailVM | null> => {
    const { getRecipeView } = await import('../content')
    const recipe = await getRecipeView(data) // backend gates canView; null => forbidden/missing
    if (!recipe) return null

    const serving = recipe.serving
    const nutrition: NutritionFactsData = {
      heading: 'This Recipe',
      serving,
      servingsPerBatch:
        recipe.batchGrams != null && serving?.grams ? recipe.batchGrams / serving.grams : null,
      caloriesPerServing:
        recipe.nutrition.caloriesPer100g != null && serving?.grams
          ? (recipe.nutrition.caloriesPer100g * serving.grams) / 100
          : recipe.nutrition.caloriesPer100g,
      readings: recipe.nutrition.readings,
    }

    return {
      id: recipe.id,
      name: recipe.name,
      subtitle: recipe.subtitle,
      creator: recipe.creator ?? undefined,
      directions: recipe.directions,
      // An item that is itself a recipe links to the recipe page (the backend resolves recipeId); a
      // leaf ingredient links to its ingredient page. item.id is the ingredient id (vegify-core shape).
      items: recipe.items.map((item, i) => ({
        key: `${item.id}-${i}`,
        label: `${item.amount.amount ?? ''} ${item.amount.unit ?? ''} ${item.name}`.trim(),
        href: item.recipeId ? `/recipes/${item.recipeId}` : `/ingredients/${item.id}`,
      })),
      nutrition,
    }
  })

const recipeQuery = (id: string) =>
  queryOptions({ queryKey: ['recipe', id], queryFn: () => getRecipe({ data: id }) })

export const Route = createFileRoute('/recipes/$recipeId/')({
  loader: async ({ context, params }) => {
    const recipe = await context.queryClient.ensureQueryData(recipeQuery(params.recipeId))
    if (!recipe) throw notFound()
  },
  component: RecipePage,
})

function RecipePage() {
  const { recipeId } = Route.useParams()
  const { data: recipe } = useSuspenseQuery(recipeQuery(recipeId))
  if (!recipe) return <div className="p-8 text-muted-foreground">Recipe not found.</div>
  return <RecipeDetailView recipe={recipe} LinkComponent={LinkAdapter} />
}
