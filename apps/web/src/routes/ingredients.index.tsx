import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { IngredientListView, type IngredientListItem } from '@vegify/ui'
import { LinkAdapter } from '../link'
import { withRetry } from '../retry'

// Standalone ingredients = those not used as a recipe's as-ingredient (those are recipes).
const getIngredients = createServerFn({ method: 'GET' }).handler(async () => {
  const { db, isListed } = await import('@vegify/db')
  const { currentUserId } = await import('../auth')
  const me = await currentUserId()
  const [all, recipes] = await Promise.all([
    db.query.ingredients.findMany({ orderBy: (i, { asc }) => asc(i.name) }),
    db.query.recipes.findMany({ columns: { asIngredientId: true } }),
  ])
  const recipeIngredientIds = new Set(recipes.map((r) => r.asIngredientId))
  return all
    .filter((i) => !recipeIngredientIds.has(i.id) && isListed(i.visibility, i.userId, me))
    .map((i): IngredientListItem => ({ id: i.id, name: i.name, caloriesPer100g: i.caloriesPer100g }))
})

export const Route = createFileRoute('/ingredients/')({
  loader: () => withRetry(() => getIngredients()),
  component: IngredientsPage,
})

function IngredientsPage() {
  return <IngredientListView ingredients={Route.useLoaderData()} LinkComponent={LinkAdapter} />
}
