import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { queryOptions, useSuspenseQuery } from '@tanstack/react-query'
import { RecipeListView, type RecipeListItem } from '@vegify/ui'
import { LinkAdapter } from '../link'

const getRecipes = createServerFn({ method: 'GET' }).handler(async (): Promise<RecipeListItem[]> => {
  const { listRecipeCards } = await import('../content')
  return listRecipeCards() // already viewer-scoped + name-sorted by the backend
})

const recipesQuery = queryOptions({ queryKey: ['recipes'], queryFn: () => getRecipes() })

export const Route = createFileRoute('/recipes/')({
  loader: ({ context }) => context.queryClient.ensureQueryData(recipesQuery),
  component: RecipesPage,
})

function RecipesPage() {
  const { data } = useSuspenseQuery(recipesQuery)
  return <RecipeListView recipes={data} LinkComponent={LinkAdapter} />
}
