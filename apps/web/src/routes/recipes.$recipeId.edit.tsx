import { createFileRoute, notFound, useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { RecipeForm, type RecipeFormDefaults, type RecipeFormInput } from '@vegify/ui'
import { withRetry } from '../retry'

const getRecipeFn = createServerFn({ method: 'GET' })
  .validator((recipeId: string) => recipeId)
  .handler(async ({ data }) => {
    const { db, getIngredientNutrition, isOwner } = await import('@vegify/db')
    const { currentUserId } = await import('../auth')
    const me = await currentUserId()
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
    if (!isOwner(recipe.asIngredient.userId, me)) throw notFound()
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
    const { currentUserId } = await import('../auth')
    return searchIngredients(data, await currentUserId())
  })

const saveFn = createServerFn({ method: 'POST' })
  .validator((input: RecipeFormInput) => input)
  .handler(async ({ data }) => {
    const { saveRecipe } = await import('@vegify/db')
    const { currentUserId } = await import('../auth')
    return saveRecipe({ ...data, userId: await currentUserId() })
  })

const deleteFn = createServerFn({ method: 'POST' })
  .validator((id: string) => id)
  .handler(async ({ data }) => {
    const { deleteRecipe } = await import('@vegify/db')
    const { currentUserId } = await import('../auth')
    await deleteRecipe(data, await currentUserId())
  })

export const Route = createFileRoute('/recipes/$recipeId/edit')({
  loader: ({ params }) => withRetry(() => getRecipeFn({ data: params.recipeId })),
  component: EditRecipe,
})

function EditRecipe() {
  const { recipe, items, servings } = Route.useLoaderData()
  const router = useRouter()
  const defaults: RecipeFormDefaults = {
    id: recipe.id,
    visibility: recipe.asIngredient.visibility,
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
