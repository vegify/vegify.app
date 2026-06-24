import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { RecipeListView, type RecipeListItem } from '@vegify/ui'
import { LinkAdapter } from '../link'

const getRecipes = createServerFn({ method: 'GET' }).handler(async () => {
  const { db } = await import('@vegify/db')
  const recipes = await db.query.recipes.findMany({ with: { asIngredient: true } })
  return recipes.map(
    (r): RecipeListItem => ({ id: r.id, name: r.asIngredient.name, subtitle: r.subtitle }),
  )
})

export const Route = createFileRoute('/recipes/')({
  loader: () => getRecipes(),
  component: RecipesPage,
})

function RecipesPage() {
  return <RecipeListView recipes={Route.useLoaderData()} LinkComponent={LinkAdapter} />
}
