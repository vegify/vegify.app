import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { queryOptions, useSuspenseQuery } from '@tanstack/react-query'
import { IngredientListView, type IngredientListItem } from '@vegify/ui'
import { LinkAdapter } from '../link'

// Standalone ingredients (recipe as-ingredients excluded) — the backend's list already does that.
const getIngredients = createServerFn({ method: 'GET' }).handler(async (): Promise<IngredientListItem[]> => {
  const { listIngredientCards } = await import('../content')
  return listIngredientCards()
})

const ingredientsQuery = queryOptions({ queryKey: ['ingredients'], queryFn: () => getIngredients() })

export const Route = createFileRoute('/ingredients/')({
  loader: ({ context }) => context.queryClient.ensureQueryData(ingredientsQuery),
  component: IngredientsPage,
})

function IngredientsPage() {
  const { data } = useSuspenseQuery(ingredientsQuery)
  return <IngredientListView ingredients={data} LinkComponent={LinkAdapter} />
}
