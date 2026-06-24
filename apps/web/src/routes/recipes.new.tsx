import { createFileRoute, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { RecipeForm, type RecipeFormInput } from '@vegify/ui'

const searchFn = createServerFn({ method: 'GET' })
  .validator((query: string) => query)
  .handler(async ({ data }) => {
    const { searchIngredients } = await import('@vegify/db')
    return searchIngredients(data)
  })

const saveFn = createServerFn({ method: 'POST' })
  .validator((input: RecipeFormInput) => input)
  .handler(async ({ data }) => {
    const { saveRecipe } = await import('@vegify/db')
    return saveRecipe(data)
  })

export const Route = createFileRoute('/recipes/new')({
  component: NewRecipe,
})

function NewRecipe() {
  const router = useRouter()
  return (
    <RecipeForm
      onSearch={(q) => searchFn({ data: q })}
      onSave={async (input) => {
        const id = await saveFn({ data: input })
        router.navigate({ to: '/recipes/$recipeId', params: { recipeId: String(id) } })
      }}
    />
  )
}
