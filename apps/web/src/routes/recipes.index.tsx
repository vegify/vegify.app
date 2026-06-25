import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { RecipeListView, type RecipeListItem } from '@vegify/ui'
import { LinkAdapter } from '../link'
import { withRetry } from '../retry'

const getRecipes = createServerFn({ method: 'GET' }).handler(async (): Promise<RecipeListItem[]> => {
  const { listRecipeCards } = await import('../content')
  return listRecipeCards() // already viewer-scoped + name-sorted by the backend
})

export const Route = createFileRoute('/recipes/')({
  loader: () => withRetry(() => getRecipes()),
  component: RecipesPage,
})

function RecipesPage() {
  return <RecipeListView recipes={Route.useLoaderData()} LinkComponent={LinkAdapter} />
}
