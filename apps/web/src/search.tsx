import { useEffect, useState } from 'react'
import { createServerFn } from '@tanstack/react-start'
import {
  SearchResultsView,
  type IngredientListItem,
  type NavLink,
  type RecipeListItem,
} from '@vegify/ui'

// Database-wide search: recipe names + standalone-ingredient names. Small dataset, so it filters in
// JS; swap for a LIKE query if the catalog grows. Mirrors the desktop's chrome search.
const searchAll = createServerFn({ method: 'GET' })
  .validator((query: string) => query)
  .handler(async ({ data }) => {
    // The backend's lists are already viewer-scoped (recipe as-ingredients excluded); filter by name
    // in JS — the catalog is small. Swap for a server-side query if it grows. Mirrors the desktop.
    const { listRecipeCards, listIngredientCards } = await import('./content')
    const q = data.toLowerCase()
    const [recipes, ingredients] = await Promise.all([listRecipeCards(), listIngredientCards()])
    const recipeHits: RecipeListItem[] = recipes.filter((r) => r.name.toLowerCase().includes(q))
    const ingredientHits: IngredientListItem[] = ingredients.filter((i) => i.name.toLowerCase().includes(q))
    return { recipes: recipeHits, ingredients: ingredientHits }
  })

/** Chrome search overlay — renders the shared SearchResultsView from a server-fn query. */
export function SearchOverlay({ query, LinkComponent }: { query: string; LinkComponent: NavLink }) {
  const [res, setRes] = useState<{ recipes: RecipeListItem[]; ingredients: IngredientListItem[] }>({
    recipes: [],
    ingredients: [],
  })
  useEffect(() => {
    let active = true
    searchAll({ data: query })
      .then((r) => {
        if (active) setRes(r)
      })
      .catch(() => {
        if (active) setRes({ recipes: [], ingredients: [] })
      })
    return () => {
      active = false
    }
  }, [query])
  return (
    <SearchResultsView
      query={query}
      recipes={res.recipes}
      ingredients={res.ingredients}
      LinkComponent={LinkComponent}
    />
  )
}
