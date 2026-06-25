import { createFileRoute, notFound, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import {
  IngredientForm,
  type IngredientFormDefaults,
  type IngredientFormInput,
} from '@vegify/ui'
import type { IngredientEditData } from '../content'
import { withRetry } from '../retry'

const getIngredientFn = createServerFn({ method: 'GET' })
  .validator((id: string) => id)
  .handler(async ({ data }): Promise<IngredientEditData> => {
    const { getIngredientEdit } = await import('../content')
    const ingredient = await getIngredientEdit(data) // backend gates isOwner; null => not owner / missing
    if (!ingredient) throw notFound()
    return ingredient
  })

const saveIngredientFn = createServerFn({ method: 'POST' })
  .validator((input: IngredientFormInput) => input)
  .handler(async ({ data }) => {
    const { saveIngredient } = await import('../content')
    return saveIngredient(data)
  })

const deleteIngredientFn = createServerFn({ method: 'POST' })
  .validator((id: string) => id)
  .handler(async ({ data }) => {
    const { deleteIngredient } = await import('../content')
    await deleteIngredient(data)
  })

export const Route = createFileRoute('/ingredients/$ingredientId/edit')({
  loader: ({ params }) => withRetry(() => getIngredientFn({ data: params.ingredientId })),
  component: EditIngredient,
})

function EditIngredient() {
  const ingredient = Route.useLoaderData()
  const router = useRouter()

  const servingGrams = ingredient.servingGrams
  const scale = servingGrams ? servingGrams / 100 : 1
  const defaults: IngredientFormDefaults = {
    id: ingredient.id,
    visibility: ingredient.visibility,
    name: ingredient.name,
    description: ingredient.description,
    priceCents: ingredient.price,
    servingGrams,
    packageGrams: ingredient.packageGrams,
    caloriesPerServing:
      ingredient.caloriesPer100g != null ? ingredient.caloriesPer100g * scale : null,
    nutrients: ingredient.nutrients.map((n) => ({
      name: n.name,
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
