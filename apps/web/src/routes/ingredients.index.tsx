import {
  infiniteQueryOptions,
  queryOptions,
  useSuspenseInfiniteQuery,
  useSuspenseQuery
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

// The A–Z index: every ingredient under one letter, rendered whole (no infinite scroll), so each
// catalog page is two links from /ingredients for crawlers and people alike. "0" = names that don't
// start with a letter.
const parseInitial = (v: unknown): string | undefined =>
  typeof v === "string" && /^[a-z0]$/i.test(v) ? v.toLowerCase() : undefined
const initialLabel = (initial: string) =>
  initial === "0" ? "#" : initial.toUpperCase()

const API_PAGE_MAX = 100 // the backend's page cap

const getIngredientsByInitial = createServerFn({ method: "GET" })
  .validator((initial: string) => initial)
  .handler(async ({ data }): Promise<IngredientListItem[]> => {
    const { listIngredientCards } = await import("../content")
    const all: IngredientListItem[] = []
    let cursor: Cursor | undefined
    // Keyset pages until a short one; a letter is a few hundred names, and 50 pages bounds the loop.
    for (let i = 0; i < 50; i++) {
      const page = await listIngredientCards({
        sort: "name_asc",
        initial: data,
        limit: API_PAGE_MAX,
        cursor: cursor?.id,
        cursorName: cursor?.name
      })
      all.push(...page)
      const tail = page.at(-1)
      if (!tail || page.length < API_PAGE_MAX) break
      cursor = { id: tail.id, name: tail.name }
    }
    // The backend's name order is case-sensitive ("Beans" before "banana"); show it the way people read.
    return all.sort((a, b) =>
      a.name.localeCompare(b.name, "en", { sensitivity: "base" })
    )
  })

const byInitialQuery = (initial: string) =>
  queryOptions({
    queryKey: ["ingredients", "initial", initial],
    queryFn: () => getIngredientsByInitial({ data: initial })
  })

const initialHref = (initial?: string) =>
  initial ? `/ingredients?letter=${initial}` : "/ingredients"

export const Route = createFileRoute("/ingredients/")({
  validateSearch: (s: {
    sort?: string
    letter?: string
  }): { sort: Sort; letter?: string } => ({
    sort: parseSort(s.sort),
    letter: parseInitial(s.letter)
  }),
  // The default sort stays out of the URL, so /ingredients itself is the canonical page (see /recipes).
  search: { middlewares: [stripSearchParams({ sort: DEFAULT_SORT })] },
  loaderDeps: ({ search }) => ({ sort: search.sort, letter: search.letter }),
  loader: async ({ context, deps }) => {
    if (deps.letter)
      await context.queryClient.ensureQueryData(byInitialQuery(deps.letter))
    else
      await context.queryClient.ensureInfiniteQueryData(
        ingredientsQuery(deps.sort)
      )
  },
  head: async ({ match }) => {
    const origin = await siteOrigin()
    const letter = match.search.letter
    if (!letter)
      return pageHead({
        origin,
        path: "/ingredients",
        title: "Ingredients",
        description:
          "Nutrition facts for plant foods: calories, macros, and the full vitamin and mineral profile, from USDA FoodData Central, Open Food Facts, and Vegify cooks."
      })
    const count =
      match.context.queryClient.getQueryData(byInitialQuery(letter).queryKey)
        ?.length ?? 0
    const label = initialLabel(letter)
    return pageHead({
      origin,
      path: initialHref(letter),
      title: `Ingredients starting with ${label}`,
      description: `All ${count} ingredients on Vegify starting with ${label}, each with its calories, macros, and vitamin and mineral profile.`,
      noindex: count === 0
    })
  },
  component: IngredientsPage
})

function IngredientsPage() {
  const { letter } = Route.useSearch()
  return letter ? <IngredientsByInitial initial={letter} /> : <AllIngredients />
}

function AllIngredients() {
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
      initialHref={initialHref}
    />
  )
}

function IngredientsByInitial({ initial }: { initial: string }) {
  const { user } = Route.useRouteContext()
  const { data } = useSuspenseQuery(byInitialQuery(initial))
  return (
    <IngredientListView
      ingredients={data}
      canCreate={!!user}
      LinkComponent={LinkAdapter}
      initial={initial}
      initialHref={initialHref}
    />
  )
}
