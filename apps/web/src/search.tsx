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
    const { db, isListed } = await import('@vegify/db')
    const { currentUserId } = await import('./auth')
    const me = await currentUserId()
    const q = data.toLowerCase()
    const [recipes, ingredients] = await Promise.all([
      db.query.recipes.findMany({ with: { asIngredient: true } }),
      db.query.ingredients.findMany(),
    ])
    const recipeIngredientIds = new Set(recipes.map((r) => r.asIngredientId))
    const recipeHits: RecipeListItem[] = recipes
      .filter(
        (r) =>
          isListed(r.asIngredient.visibility, r.asIngredient.userId, me) &&
          r.asIngredient.name.toLowerCase().includes(q),
      )
      .map((r) => ({ id: r.id, name: r.asIngredient.name, subtitle: r.subtitle }))
    const ingredientHits: IngredientListItem[] = ingredients
      .filter(
        (i) =>
          !recipeIngredientIds.has(i.id) && isListed(i.visibility, i.userId, me) && i.name.toLowerCase().includes(q),
      )
      .map((i) => ({ id: i.id, name: i.name, caloriesPer100g: i.caloriesPer100g }))
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
