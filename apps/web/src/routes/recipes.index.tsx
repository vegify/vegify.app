import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { RecipeListView, type RecipeListItem } from '@vegify/ui'
import { LinkAdapter } from '../link'

const getRecipes = createServerFn({ method: 'GET' }).handler(async () => {
  const { db, isListed } = await import('@vegify/db')
  const { currentUserId } = await import('../auth')
  const me = await currentUserId()
  const recipes = await db.query.recipes.findMany({ with: { asIngredient: true } })
  return recipes
    .filter((r) => isListed(r.asIngredient.visibility, r.asIngredient.userId, me))
    .map((r): RecipeListItem => ({ id: r.id, name: r.asIngredient.name, subtitle: r.subtitle }))
    // Sort by name to match the desktop's `ORDER BY i.name` (the relational findMany can't orderBy
    // the joined as-ingredient's name, so sort here). Binary compare = SQLite's default collation.
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
})

export const Route = createFileRoute('/recipes/')({
  loader: () => getRecipes(),
  component: RecipesPage,
})

function RecipesPage() {
  return <RecipeListView recipes={Route.useLoaderData()} LinkComponent={LinkAdapter} />
}
