import { createFileRoute, notFound, redirect } from "@tanstack/react-router"
import { createServerFn } from "@tanstack/react-start"

import { RecipeDetailPage, recipeQuery } from "../recipe-detail"

// The canonical public recipe URL: /<username>/<recipe-slug> (docs/usernames.md — "the real SEO/GEO
// engine"). Resolve the slug → recipe id (via slug_history when it's an old slug → 301 to canonical),
// then render the shared detail. Public + optionally-authed, like the profile route.
const resolveFn = createServerFn({ method: "GET" })
  .validator((p: { username: string; slug: string }) => p)
  .handler(async ({ data }) => {
    const { resolveRecipeBySlug } = await import("../content")
    return resolveRecipeBySlug(data.username, data.slug)
  })

export const Route = createFileRoute("/$username/$recipeSlug")({
  loader: async ({ context, params }) => {
    const hit = await resolveFn({
      data: { username: params.username, slug: params.recipeSlug }
    })
    if (!hit) throw notFound()
    // Old slug (slug_history) → 301 to the current canonical.
    if (hit.canonicalSlug !== params.recipeSlug) {
      throw redirect({
        to: "/$username/$recipeSlug",
        params: { username: params.username, recipeSlug: hit.canonicalSlug },
        statusCode: 301 // permanent — old slug → current canonical
      })
    }
    await context.queryClient.ensureQueryData(recipeQuery(hit.recipeId))
    return { recipeId: hit.recipeId }
  },
  component: RecipeSlugPage
})

function RecipeSlugPage() {
  const { recipeId } = Route.useLoaderData()
  return <RecipeDetailPage recipeId={recipeId} />
}
