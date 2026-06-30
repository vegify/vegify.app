// The web's typed view of the standing Axum backend's content API. Every function is a thin HTTP call
// to vegify-server (via ./api, which forwards the session cookie as a Bearer token); the shapes below
// mirror vegify-core's wire types (proven byte-parity with the desktop's DAL). The web SSR loaders +
// mutation server-fns call these — the web holds NO database of its own. See [[server-source-of-truth]].

import { api } from './api'
import type { RecipeFormInput, IngredientFormInput, IngredientSearchItem } from '@vegify/ui'

type Visibility = 'public' | 'private' | 'unlisted'
type Reading = { name: string; amountPer100g: number; unit: string }
type Amount = { amount: number | null; unit: string | null; grams: number }
type AggregatedNutrition = { caloriesPer100g: number | null; readings: Reading[] }

export type RecipeCard = { id: string; name: string; subtitle: string | null }
export type Profile = { username: string; name: string; recipes: RecipeCard[] }
export type RecipeItem = { id: string; name: string; amount: Amount; recipeId: string | null }
export type RecipeView = {
  id: string
  name: string
  subtitle: string | null
  directions: string | null
  creator: string | null
  canEdit: boolean
  serving: Amount | null
  batchGrams: number | null
  items: RecipeItem[]
  nutrition: AggregatedNutrition
}
export type RecipeEditItem = {
  ingredientId: string
  name: string
  grams: number
  caloriesPer100g: number | null
  readings: Reading[]
}
export type RecipeEditData = {
  id: string
  name: string
  subtitle: string | null
  directions: string | null
  servings: number | null
  visibility: Visibility
  items: RecipeEditItem[]
}
export type IngredientCard = { id: string; name: string; caloriesPer100g: number | null }
export type IngredientEditData = {
  id: string
  name: string
  description: string | null
  price: number | null
  caloriesPer100g: number | null
  servingGrams: number | null
  packageGrams: number | null
  visibility: Visibility
  canEdit: boolean
  nutrients: Reading[]
}

const byId = (id: string) => `?id=${encodeURIComponent(id)}`

// Keyset page query for the catalog reads: `cursor` (the last card's id) + a page `limit`. Empty when
// neither is set, so an un-paginated call still fetches the full (server-capped) list.
const pageParams = (cursor?: string, limit?: number) => {
  const p = new URLSearchParams()
  if (cursor) p.set('cursor', cursor)
  if (limit != null) p.set('limit', String(limit))
  const s = p.toString()
  return s ? `?${s}` : ''
}

// --- reads (the backend scopes each to the session user, from the forwarded Bearer token) ---

export const listRecipeCards = (cursor?: string, limit?: number) =>
  api<RecipeCard[]>(`/api/content/recipes${pageParams(cursor, limit)}`)
export const getRecipeView = (id: string) => api<RecipeView | null>(`/api/content/recipe-detail${byId(id)}`)
export const getRecipeEdit = (id: string) => api<RecipeEditData | null>(`/api/content/recipe-edit${byId(id)}`)
export const listIngredientCards = (cursor?: string, limit?: number) =>
  api<IngredientCard[]>(`/api/content/ingredients${pageParams(cursor, limit)}`)
export const getIngredientView = (id: string) =>
  api<IngredientEditData | null>(`/api/content/ingredient-detail${byId(id)}`)
export const getIngredientEdit = (id: string) =>
  api<IngredientEditData | null>(`/api/content/ingredient-edit${byId(id)}`)
export const searchIngredients = (q: string) =>
  api<IngredientSearchItem[]>(`/api/content/search?q=${encodeURIComponent(q)}`)
// Public profile by handle (optionally-authed: the forwarded cookie lets a viewer see their own
// non-public recipes on their own profile). Null when the handle has no account.
export const getProfile = (username: string) =>
  api<Profile | null>(`/api/content/profile?username=${encodeURIComponent(username)}`)

// --- mutations (the server stamps userId from the session — a client-supplied owner is never trusted) ---

// RecipeFormInput / IngredientFormInput are wire-compatible with vegify-core's SaveRecipeInput /
// SaveIngredientInput (camelCase; the extra optional asIngredientId + item unit default server-side),
// so the form input POSTs as-is. A supplied `id` upserts (edit); its absence mints one (create).
export const saveRecipe = (input: RecipeFormInput): Promise<string> =>
  api<{ id: string }>('/api/content/recipes', { method: 'POST', body: input }).then((r) => r.id)
export const deleteRecipe = (id: string): Promise<void> =>
  api(`/api/content/recipes${byId(id)}`, { method: 'DELETE' }).then(() => undefined)
export const saveIngredient = (input: IngredientFormInput): Promise<string> =>
  api<{ id: string }>('/api/content/ingredients', { method: 'POST', body: input }).then((r) => r.id)
export const deleteIngredient = (id: string): Promise<void> =>
  api(`/api/content/ingredients${byId(id)}`, { method: 'DELETE' }).then(() => undefined)
