import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { IngredientListView, type IngredientListItem } from '@vegify/ui'
import { LinkAdapter } from '../link'
import { withRetry } from '../retry'

// Standalone ingredients (recipe as-ingredients excluded) — the backend's list already does that.
const getIngredients = createServerFn({ method: 'GET' }).handler(async (): Promise<IngredientListItem[]> => {
  const { listIngredientCards } = await import('../content')
  return listIngredientCards()
})

export const Route = createFileRoute('/ingredients/')({
  loader: () => withRetry(() => getIngredients()),
  component: IngredientsPage,
})

function IngredientsPage() {
  return <IngredientListView ingredients={Route.useLoaderData()} LinkComponent={LinkAdapter} />
}
