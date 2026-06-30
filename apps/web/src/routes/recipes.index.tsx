import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { infiniteQueryOptions, useSuspenseInfiniteQuery } from '@tanstack/react-query'
import { RecipeListView, type RecipeListItem } from '@vegify/ui/screens'
import { PAGE_SIZE, parseSort, type Sort } from '@vegify/ui/catalog'
import { LinkAdapter } from '../link'

type Cursor = { id: string; name: string }

const getRecipes = createServerFn({ method: 'GET' })
  .validator((p: { sort: Sort; cursor?: string; cursorName?: string }) => p)
  .handler(async ({ data }): Promise<RecipeListItem[]> => {
    const { listRecipeCards } = await import('../content')
    return listRecipeCards({ ...data, limit: PAGE_SIZE }) // viewer-scoped + keyset-sorted by the backend
  })

const recipesQuery = (sort: Sort) =>
  infiniteQueryOptions({
    queryKey: ['recipes', sort],
    queryFn: ({ pageParam }) =>
      getRecipes({ data: { sort, cursor: pageParam?.id, cursorName: pageParam?.name } }),
    initialPageParam: undefined as Cursor | undefined,
    getNextPageParam: (last): Cursor | undefined => {
      const tail = last.at(-1)
      return !tail || last.length < PAGE_SIZE ? undefined : { id: tail.id, name: tail.name }
    },
  })

export const Route = createFileRoute('/recipes/')({
  validateSearch: (s: { sort?: string }): { sort: Sort } => ({ sort: parseSort(s.sort) }),
  loaderDeps: ({ search }) => ({ sort: search.sort }),
  loader: ({ context, deps }) => context.queryClient.ensureInfiniteQueryData(recipesQuery(deps.sort)),
  component: RecipesPage,
})

function RecipesPage() {
  const { user } = Route.useRouteContext()
  const { sort } = Route.useSearch()
  const navigate = Route.useNavigate()
  const { data, fetchNextPage, hasNextPage, isFetchingNextPage } = useSuspenseInfiniteQuery(recipesQuery(sort))
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
