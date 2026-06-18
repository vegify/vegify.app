import { createFileRoute, notFound, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { RecipeForm, type RecipeFormDefaults, type RecipeFormInput } from '@vegify/ui'

const getRecipeFn = createServerFn({ method: 'GET' })
  .validator((recipeId: string) => recipeId)
  .handler(async ({ data }) => {
    const { db, getIngredientNutrition } = await import('@vegify/db')
    const id = data
    const recipe = await db.query.recipes.findFirst({
      where: (r, { eq }) => eq(r.id, id),
      with: {
        asIngredient: { with: { servingSize: true, batchSize: true } },
        items: {
          orderBy: (iir, { asc }) => [asc(iir.order)],
          with: { ingredient: true, amount: true },
        },
      },
    })
    if (!recipe) throw notFound()
    const items = []
    for (const it of recipe.items) {
      if (!it.ingredient) continue
      const n = await getIngredientNutrition(it.ingredient.id)
      items.push({
        ingredientId: it.ingredient.id,
        name: it.ingredient.name,
        grams: it.amount?.grams ?? 0,
        caloriesPer100g: n.caloriesPer100g,
        readings: n.readings,
      })
    }
    const sg = recipe.asIngredient.servingSize?.grams ?? null
    const bg = recipe.asIngredient.batchSize?.grams ?? null
    return { recipe, items, servings: sg && bg ? bg / sg : 1 }
  })

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

const deleteFn = createServerFn({ method: 'POST' })
  .validator((id: string) => id)
  .handler(async ({ data }) => {
    const { deleteRecipe } = await import('@vegify/db')
    await deleteRecipe(data)
  })

export const Route = createFileRoute('/recipes/$recipeId/edit')({
  loader: ({ params }) => getRecipeFn({ data: params.recipeId }),
  component: EditRecipe,
})

function EditRecipe() {
  const { recipe, items, servings } = Route.useLoaderData()
  const router = useRouter()
  const defaults: RecipeFormDefaults = {
    id: recipe.id,
    name: recipe.asIngredient.name,
    subtitle: recipe.subtitle,
    directions: recipe.directions,
    servings,
    items,
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
        await deleteFn({ data: recipe.id })
        router.navigate({ to: '/recipes' })
      }}
    />
  )
}
