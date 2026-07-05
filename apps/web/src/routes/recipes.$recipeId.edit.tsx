import { createFileRoute, notFound, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { queryOptions, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'
import { RecipeForm, type RecipeFormDefaults, type RecipeFormInput } from '@vegify/ui/recipe-form'
import type { RecipeEditData } from '../content'

const getRecipeFn = createServerFn({ method: 'GET' })
  .validator((recipeId: string) => recipeId)
  .handler(async ({ data }): Promise<RecipeEditData | null> => {
    const { getRecipeEdit } = await import('../content')
    return getRecipeEdit(data) // backend gates isOwner; null => not owner / missing
  })

const searchFn = createServerFn({ method: 'GET' })
  .validator((query: string) => query)
  .handler(async ({ data }) => {
    const { searchIngredients } = await import('../content')
    return searchIngredients(data)
  })

const saveFn = createServerFn({ method: 'POST' })
  .validator((input: RecipeFormInput) => input)
  .handler(async ({ data }) => {
    const { saveRecipe } = await import('../content')
    return saveRecipe(data)
  })

const deleteFn = createServerFn({ method: 'POST' })
  .validator((id: string) => id)
  .handler(async ({ data }) => {
    const { deleteRecipe } = await import('../content')
    await deleteRecipe(data)
  })

const recipeEditQuery = (id: string) =>
  queryOptions({ queryKey: ['recipe-edit', id], queryFn: () => getRecipeFn({ data: id }) })

export const Route = createFileRoute('/recipes/$recipeId/edit')({
  loader: async ({ context, params }) => {
    const recipe = await context.queryClient.ensureQueryData(recipeEditQuery(params.recipeId))
    if (!recipe) throw notFound()
  },
  component: EditRecipe,
})

function EditRecipe() {
  const { recipeId } = Route.useParams()
  const { data } = useSuspenseQuery(recipeEditQuery(recipeId))
  const router = useRouter()
  const queryClient = useQueryClient()
  if (!data) return <div className="p-8 text-muted-foreground">Recipe not found.</div>

  const defaults: RecipeFormDefaults = {
    id: data.id,
    visibility: data.visibility,
    name: data.name,
    subtitle: data.subtitle,
    directions: data.directions,
    servings: data.servings,
    items: data.items.map((i) => ({
      ...i,
      grams: i.grams ?? 0,
      readings: i.readings.map((r) => ({ ...r, amountPer100g: r.amountPer100g ?? 0 })),
    })),
  }
  return (
    <RecipeForm
      defaults={defaults}
      onSearch={(q) => searchFn({ data: q })}
      onSave={async (input) => {
        const id = await saveFn({ data: input })
        // The recipe changed → its list card, detail view, and edit form all refetch on next read.
        await queryClient.invalidateQueries({ queryKey: ['recipes'] })
        await queryClient.invalidateQueries({ queryKey: ['recipe', String(id)] })
        await queryClient.invalidateQueries({ queryKey: ['recipe-edit', String(id)] })
        router.navigate({ to: '/recipes/$recipeId', params: { recipeId: String(id) } })
      }}
      onDelete={async () => {
        await deleteFn({ data: data.id })
        queryClient.removeQueries({ queryKey: ['recipe', data.id] })
        queryClient.removeQueries({ queryKey: ['recipe-edit', data.id] })
        await queryClient.invalidateQueries({ queryKey: ['recipes'] })
        router.navigate({ to: '/recipes', search: { sort: 'newest' } })
      }}
    />
  )
}
