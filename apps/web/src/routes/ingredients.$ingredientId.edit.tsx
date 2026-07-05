import { createFileRoute, notFound, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { queryOptions, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'
import {
  IngredientForm,
  type IngredientFormDefaults,
  type IngredientFormInput,
} from '@vegify/ui/ingredient-form'
import type { IngredientEditData } from '../content'

const getIngredientFn = createServerFn({ method: 'GET' })
  .validator((id: string) => id)
  .handler(async ({ data }): Promise<IngredientEditData | null> => {
    const { getIngredientEdit } = await import('../content')
    return getIngredientEdit(data) // backend gates isOwner; null => not owner / missing
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

const ingredientEditQuery = (id: string) =>
  queryOptions({ queryKey: ['ingredient-edit', id], queryFn: () => getIngredientFn({ data: id }) })

export const Route = createFileRoute('/ingredients/$ingredientId/edit')({
  loader: async ({ context, params }) => {
    const ingredient = await context.queryClient.ensureQueryData(ingredientEditQuery(params.ingredientId))
    if (!ingredient) throw notFound()
  },
  component: EditIngredient,
})

function EditIngredient() {
  const { ingredientId } = Route.useParams()
  const { data: ingredient } = useSuspenseQuery(ingredientEditQuery(ingredientId))
  const router = useRouter()
  const queryClient = useQueryClient()
  if (!ingredient) return <div className="p-8 text-muted-foreground">Ingredient not found.</div>

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
      amountPerServing: (n.amountPer100g ?? 0) * scale,
      unit: n.unit,
    })),
  }

  return (
    <IngredientForm
      defaults={defaults}
      onSave={async (input) => {
        const id = await saveIngredientFn({ data: input })
        await queryClient.invalidateQueries({ queryKey: ['ingredients'] })
        await queryClient.invalidateQueries({ queryKey: ['ingredient', String(id)] })
        await queryClient.invalidateQueries({ queryKey: ['ingredient-edit', String(id)] })
        router.navigate({
          to: '/ingredients/$ingredientId',
          params: { ingredientId: String(id) },
        })
      }}
      onDelete={async () => {
        await deleteIngredientFn({ data: ingredient.id })
        queryClient.removeQueries({ queryKey: ['ingredient', ingredient.id] })
        queryClient.removeQueries({ queryKey: ['ingredient-edit', ingredient.id] })
        await queryClient.invalidateQueries({ queryKey: ['ingredients'] })
        router.navigate({ to: '/ingredients', search: { sort: 'newest' } })
      }}
    />
  )
}
