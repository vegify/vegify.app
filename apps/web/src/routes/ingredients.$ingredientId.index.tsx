import { createFileRoute, notFound } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { IngredientDetailView, type IngredientDetailVM, type NutritionFactsData } from '@vegify/ui'
import { LinkAdapter } from '../link'

const getIngredient = createServerFn({ method: 'GET' })
  .validator((ingredientId: string) => ingredientId)
  .handler(async ({ data }): Promise<IngredientDetailVM> => {
    const { db } = await import('@vegify/db')
    const ingredient = await db.query.ingredients.findFirst({
      where: (i, { eq }) => eq(i.id, data),
      with: {
        creator: true,
        servingSize: true,
        batchSize: true,
        nutrients: { with: { nutrient: true } },
      },
    })
    if (!ingredient) throw notFound()

    const nutrition: NutritionFactsData = {
      heading: 'This Ingredient',
      caloriesPerServing:
        ingredient.caloriesPer100g != null
          ? ingredient.caloriesPer100g *
            (ingredient.servingSize?.grams ? ingredient.servingSize.grams / 100 : 1)
          : null,
      serving: ingredient.servingSize
        ? {
            amount: ingredient.servingSize.amount,
            unit: ingredient.servingSize.unit,
            grams: ingredient.servingSize.grams,
          }
        : null,
      servingsPerBatch:
        ingredient.batchSize && ingredient.servingSize?.grams
          ? ingredient.batchSize.grams / ingredient.servingSize.grams
          : null,
      readings: ingredient.nutrients.map((n) => ({
        name: n.nutrient.name,
        amountPer100g: n.amountPer100g,
        unit: n.unit,
      })),
    }

    return {
      id: ingredient.id,
      name: ingredient.name,
      description: ingredient.description,
      nutrition,
    }
  })

export const Route = createFileRoute('/ingredients/$ingredientId/')({
  loader: ({ params }) => getIngredient({ data: params.ingredientId }),
  component: IngredientPage,
})

function IngredientPage() {
  return <IngredientDetailView ingredient={Route.useLoaderData()} LinkComponent={LinkAdapter} />
}
