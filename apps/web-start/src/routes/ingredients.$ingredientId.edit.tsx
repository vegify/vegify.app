import { createFileRoute, notFound, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import {
  IngredientForm,
  type IngredientFormDefaults,
  type IngredientFormInput,
} from '@vegify/ui'

const getIngredientFn = createServerFn({ method: 'GET' })
  .validator((id: string) => id)
  .handler(async ({ data }) => {
    const { db } = await import('@vegify/db')
    const ingredient = await db.query.ingredients.findFirst({
      where: (i, { eq }) => eq(i.id, data),
      with: {
        servingSize: true,
        batchSize: true,
        nutrients: { with: { nutrient: true } },
      },
    })
    if (!ingredient) throw notFound()
    return ingredient
  })

const saveIngredientFn = createServerFn({ method: 'POST' })
  .validator((input: IngredientFormInput) => input)
  .handler(async ({ data }) => {
    const { saveIngredient } = await import('@vegify/db')
    return saveIngredient(data)
  })

const deleteIngredientFn = createServerFn({ method: 'POST' })
  .validator((id: string) => id)
  .handler(async ({ data }) => {
    const { deleteIngredient } = await import('@vegify/db')
    await deleteIngredient(data)
  })

export const Route = createFileRoute('/ingredients/$ingredientId/edit')({
  loader: ({ params }) => getIngredientFn({ data: params.ingredientId }),
  component: EditIngredient,
})

function EditIngredient() {
  const ingredient = Route.useLoaderData()
  const router = useRouter()

  const servingGrams = ingredient.servingSize?.grams ?? null
  const scale = servingGrams ? servingGrams / 100 : 1
  const defaults: IngredientFormDefaults = {
    id: ingredient.id,
    name: ingredient.name,
    description: ingredient.description,
    priceCents: ingredient.price,
    servingGrams,
    packageGrams: ingredient.batchSize?.grams ?? null,
    caloriesPerServing:
      ingredient.caloriesPer100g != null ? ingredient.caloriesPer100g * scale : null,
    nutrients: ingredient.nutrients.map((n) => ({
      name: n.nutrient.name,
      amountPerServing: n.amountPer100g * scale,
      unit: n.unit,
    })),
  }

  return (
    <IngredientForm
      defaults={defaults}
      onSave={async (input) => {
        const id = await saveIngredientFn({ data: input })
        router.navigate({
          to: '/ingredients/$ingredientId',
          params: { ingredientId: String(id) },
        })
      }}
      onDelete={async () => {
        await deleteIngredientFn({ data: ingredient.id })
        router.navigate({ to: '/recipes' })
      }}
    />
  )
}
