import { createFileRoute, notFound, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { RecipeForm, type RecipeFormDefaults, type RecipeFormInput } from '@vegify/ui'
import type { RecipeEditData } from '../content'
import { withRetry } from '../retry'

const getRecipeFn = createServerFn({ method: 'GET' })
  .validator((recipeId: string) => recipeId)
  .handler(async ({ data }): Promise<RecipeEditData> => {
    const { getRecipeEdit } = await import('../content')
    const recipe = await getRecipeEdit(data) // backend gates isOwner; null => not owner / missing
    if (!recipe) throw notFound()
    return recipe
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

export const Route = createFileRoute('/recipes/$recipeId/edit')({
  loader: ({ params }) => withRetry(() => getRecipeFn({ data: params.recipeId })),
  component: EditRecipe,
})

function EditRecipe() {
  const data = Route.useLoaderData()
  const router = useRouter()
  const defaults: RecipeFormDefaults = {
    id: data.id,
    visibility: data.visibility,
    name: data.name,
    subtitle: data.subtitle,
    directions: data.directions,
    servings: data.servings,
    items: data.items,
  }
  return (
    <RecipeForm
      defaults={defaults}
      onSearch={(q) => searchFn({ data: q })}
      onSave={async (input) => {
        const id = await saveFn({ data: input })
        router.navigate({ to: '/recipes/$recipeId', params: { recipeId: String(id) } })
      }}
      onDelete={async () => {
        await deleteFn({ data: data.id })
        router.navigate({ to: '/recipes' })
      }}
    />
  )
}
