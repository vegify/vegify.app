import { useQuery } from "@tanstack/react-query"
import { createServerFn } from "@tanstack/react-start"
import {
  type IngredientListItem,
  type NavLink,
  type RecipeListItem,
  SearchResultsView
} from "@vegify/ui/screens"

// Database-wide search: recipe names + standalone-ingredient names, ranked server-side (FTS5,
// exact-prefix > word-prefix > substring — P2.4). Mirrors the desktop's chrome search.
const searchAll = createServerFn({ method: "GET" })
  .validator((query: string) => query)
  .handler(async ({ data }) => {
    const { searchContent, mediaUrl } = await import("./content")
    const { recipes, ingredients } = await searchContent(data)
    // Mirrors the recipe list route's card→list-item mapping (photoKey -> absolute photoUrl).
    const recipeHits: RecipeListItem[] = recipes.map((r) => ({
      ...r,
      photoUrl: mediaUrl(r.photoKey)
    }))
    const ingredientHits: IngredientListItem[] = ingredients
    return { recipes: recipeHits, ingredients: ingredientHits }
  })

const EMPTY = {
  recipes: [] as RecipeListItem[],
  ingredients: [] as IngredientListItem[]
}

/** Chrome search overlay — renders the shared SearchResultsView from a TanStack Query (client-side
 *  live search). placeholderData keeps the prior results on screen while the next keystroke's query
 *  loads, so the list doesn't flicker to empty between debounced inputs. */
export function SearchOverlay({
  query,
  LinkComponent
}: {
  query: string
  LinkComponent: NavLink
}) {
  const { data } = useQuery({
    queryKey: ["search", query],
    queryFn: () => searchAll({ data: query }),
    placeholderData: (prev) => prev
  })
  const res = data ?? EMPTY
  return (
    <SearchResultsView
      query={query}
      recipes={res.recipes}
      ingredients={res.ingredients}
      LinkComponent={LinkComponent}
    />
  )
}
