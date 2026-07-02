import { createFileRoute, notFound, redirect } from '@tanstack/react-router'
import { recipeQuery, RecipeDetailPage } from '../recipe-detail'

// Legacy id URL. The canonical recipe URL is /<username>/<slug> (docs/usernames.md); this 301s there
// when the recipe has an owner handle + slug, and only renders in place as a fallback (ownerless or
// pre-backfill rows with no canonical URL yet).
export const Route = createFileRoute('/recipes/$recipeId/')({
  loader: async ({ context, params }) => {
    const data = await context.queryClient.ensureQueryData(recipeQuery(params.recipeId))
    if (!data) throw notFound()
    if (data.canonical) {
      throw redirect({
        to: '/$username/$recipeSlug',
        params: { username: data.canonical.username, recipeSlug: data.canonical.slug },
        statusCode: 301, // permanent — the id URL is a legacy alias of the canonical
      })
    }
  },
  component: RecipePage,
})

function RecipePage() {
  const { recipeId } = Route.useParams()
  return <RecipeDetailPage recipeId={recipeId} />
}
