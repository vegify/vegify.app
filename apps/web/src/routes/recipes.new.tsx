import { createFileRoute, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { useQueryClient } from '@tanstack/react-query'
import { RecipeForm, type RecipeFormInput } from '@vegify/ui'

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

export const Route = createFileRoute('/recipes/new')({
  component: NewRecipe,
})

function NewRecipe() {
  const router = useRouter()
  const queryClient = useQueryClient()
  return (
    <RecipeForm
      onSearch={(q) => searchFn({ data: q })}
      onSave={async (input) => {
        const id = await saveFn({ data: input })
        await queryClient.invalidateQueries({ queryKey: ['recipes'] }) // the list gains the new recipe
        router.navigate({ to: '/recipes/$recipeId', params: { recipeId: String(id) } })
      }}
    />
  )
}
