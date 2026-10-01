import {
  infiniteQueryOptions,
  useSuspenseInfiniteQuery
} from "@tanstack/react-query"
import { createFileRoute, stripSearchParams } from "@tanstack/react-router"
import { createServerFn } from "@tanstack/react-start"
import {
  DEFAULT_SORT,
  PAGE_SIZE,
  parseSort,
  type Sort
} from "@vegify/ui/catalog"
import { type RecipeListItem, RecipeListView } from "@vegify/ui/screens"

import { LinkAdapter } from "../link"
import { pageHead, siteOrigin } from "../seo"

type Cursor = { id: string; name: string }

const getRecipes = createServerFn({ method: "GET" })
  .validator((p: { sort: Sort; cursor?: string; cursorName?: string }) => p)
  .handler(async ({ data }): Promise<RecipeListItem[]> => {
    const { listRecipeCards, mediaUrl } = await import("../content")
    const cards = await listRecipeCards({ ...data, limit: PAGE_SIZE }) // viewer-scoped + keyset-sorted
    return cards.map((r) => ({ ...r, photoUrl: mediaUrl(r.photoKey) }))
  })

const recipesQuery = (sort: Sort) =>
  infiniteQueryOptions({
    queryKey: ["recipes", sort],
    queryFn: ({ pageParam }) =>
      getRecipes({
        data: { sort, cursor: pageParam?.id, cursorName: pageParam?.name }
      }),
    initialPageParam: undefined as Cursor | undefined,
    getNextPageParam: (last): Cursor | undefined => {
      const tail = last.at(-1)
      return !tail || last.length < PAGE_SIZE
        ? undefined
        : { id: tail.id, name: tail.name }
    }
  })

export const Route = createFileRoute("/recipes/")({
  validateSearch: (s: { sort?: string }): { sort: Sort } => ({
    sort: parseSort(s.sort)
  }),
  // The default sort stays out of the URL, so /recipes itself is the canonical page (a 200, not a
  // redirect to ?sort=newest) and every sort variant points back to it.
  search: { middlewares: [stripSearchParams({ sort: DEFAULT_SORT })] },
  loaderDeps: ({ search }) => ({ sort: search.sort }),
  loader: ({ context, deps }) =>
    context.queryClient.ensureInfiniteQueryData(recipesQuery(deps.sort)),
  head: async () =>
    pageHead({
      origin: await siteOrigin(),
      path: "/recipes",
      title: "Recipes",
      description:
        "Plant-based recipes from the Vegify community, each with its calories, macros, and every vitamin and mineral worked out per serving."
    }),
  component: RecipesPage
})

function RecipesPage() {
  const { user } = Route.useRouteContext()
  const { sort } = Route.useSearch()
  const navigate = Route.useNavigate()
  const { data, fetchNextPage, hasNextPage, isFetchingNextPage } =
    useSuspenseInfiniteQuery(recipesQuery(sort))
  return (
    <RecipeListView
      recipes={data.pages.flat()}
      canCreate={!!user}
      LinkComponent={LinkAdapter}
      sort={sort}
      onSortChange={(s) => navigate({ search: { sort: s } })}
      onLoadMore={fetchNextPage}
      hasMore={hasNextPage}
      isLoadingMore={isFetchingNextPage}
    />
  )
}
