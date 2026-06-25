import { createFileRoute, notFound } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { IngredientDetailView, type IngredientDetailVM, type NutritionFactsData } from '@vegify/ui'
import { LinkAdapter } from '../link'
import { withRetry } from '../retry'

const getIngredient = createServerFn({ method: 'GET' })
  .validator((ingredientId: string) => ingredientId)
  .handler(async ({ data }): Promise<IngredientDetailVM> => {
    const { getIngredientView } = await import('../content')
    const ing = await getIngredientView(data) // backend gates canView; null => forbidden/missing
    if (!ing) throw notFound()

    const scale = ing.servingGrams ? ing.servingGrams / 100 : 1
    const nutrition: NutritionFactsData = {
      heading: 'This Ingredient',
      caloriesPerServing: ing.caloriesPer100g != null ? ing.caloriesPer100g * scale : null,
      // The backend's IngredientEditData carries serving GRAMS only (no amount/unit) — the same shape
      // the desktop renders. Enriching it with the serving amount/unit is a possible follow-up.
      serving: ing.servingGrams != null ? { amount: null, unit: null, grams: ing.servingGrams } : null,
      servingsPerBatch:
        ing.packageGrams != null && ing.servingGrams ? ing.packageGrams / ing.servingGrams : null,
      readings: ing.nutrients,
    }

    return {
      id: ing.id,
      name: ing.name,
      description: ing.description,
      nutrition,
    }
  })

export const Route = createFileRoute('/ingredients/$ingredientId/')({
  loader: ({ params }) => withRetry(() => getIngredient({ data: params.ingredientId })),
  component: IngredientPage,
})

function IngredientPage() {
  const ingredient = Route.useLoaderData()
  if (!ingredient) return <div className="p-8 text-muted-foreground">Ingredient not found.</div>
  return <IngredientDetailView ingredient={ingredient} LinkComponent={LinkAdapter} />
}
