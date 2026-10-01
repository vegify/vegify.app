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
import { type IngredientListItem, IngredientListView } from "@vegify/ui/screens"

import { LinkAdapter } from "../link"
import { pageHead, siteOrigin } from "../seo"

type Cursor = { id: string; name: string }

// Standalone ingredients (recipe as-ingredients excluded) — the backend's list already does that.
const getIngredients = createServerFn({ method: "GET" })
  .validator((p: { sort: Sort; cursor?: string; cursorName?: string }) => p)
  .handler(async ({ data }): Promise<IngredientListItem[]> => {
    const { listIngredientCards } = await import("../content")
    return listIngredientCards({ ...data, limit: PAGE_SIZE })
  })

const ingredientsQuery = (sort: Sort) =>
  infiniteQueryOptions({
    queryKey: ["ingredients", sort],
    queryFn: ({ pageParam }) =>
      getIngredients({
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

export const Route = createFileRoute("/ingredients/")({
  validateSearch: (s: { sort?: string }): { sort: Sort } => ({
    sort: parseSort(s.sort)
  }),
  // The default sort stays out of the URL, so /ingredients itself is the canonical page (see /recipes).
  search: { middlewares: [stripSearchParams({ sort: DEFAULT_SORT })] },
  loaderDeps: ({ search }) => ({ sort: search.sort }),
  loader: ({ context, deps }) =>
    context.queryClient.ensureInfiniteQueryData(ingredientsQuery(deps.sort)),
  head: async () =>
    pageHead({
      origin: await siteOrigin(),
      path: "/ingredients",
      title: "Ingredients",
      description:
        "Nutrition facts for plant foods: calories, macros, and the full vitamin and mineral profile, from USDA FoodData Central, Open Food Facts, and Vegify cooks."
    }),
  component: IngredientsPage
})

function IngredientsPage() {
  const { user } = Route.useRouteContext()
  const { sort } = Route.useSearch()
  const navigate = Route.useNavigate()
  const { data, fetchNextPage, hasNextPage, isFetchingNextPage } =
    useSuspenseInfiniteQuery(ingredientsQuery(sort))
  return (
    <IngredientListView
      ingredients={data.pages.flat()}
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
