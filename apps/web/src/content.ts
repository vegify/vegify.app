// The web's typed view of the standing Axum backend's content API. Every function is a thin HTTP call
// to vegify-server (via ./api, which forwards the session cookie as a Bearer token); the shapes below
// mirror vegify-core's wire types (proven byte-parity with the desktop's DAL). The web SSR loaders +
// mutation server-fns call these — the web holds NO database of its own. See [[server-source-of-truth]].

// The wire types are GENERATED from vegify-core (@vegify/api-types — `just bindings`), so the web
// imports the server's actual serde contract instead of hand-mirroring it. Re-exported here so
// routes keep importing from './content'. Coverage is the FULL contract now — core shapes
// AND server-local responses — via the api TypeCollection (services/api/src/lib.rs).
import type {
  AggregatedNutrition,
  Amount,
  DayLog,
  IngredientCard,
  IngredientEditData,
  IngredientSlugHit,
  NutritionProfile,
  Profile,
  Reading,
  RecentIngredient,
  RecipeCard,
  RecipeEditData,
  RecipeEditItem,
  RecipeItem,
  RecipeSlugHit,
  RecipeView,
  SaveLogEntryInput,
  UploadTicket,
  Visibility
} from "@vegify/api-types"
import type { BlogPostData, BlogSummary } from "@vegify/ui/blog"
import type { IngredientFormInput } from "@vegify/ui/ingredient-form"
import type {
  IngredientSearchItem,
  RecipeFormInput
} from "@vegify/ui/recipe-form"

import { api, apiUrl } from "./api"

export type {
  AggregatedNutrition,
  Amount,
  DayLog,
  IngredientCard,
  IngredientEditData,
  IngredientSlugHit,
  NutritionProfile,
  Profile,
  Reading,
  RecentIngredient,
  RecipeCard,
  RecipeEditData,
  RecipeEditItem,
  RecipeItem,
  RecipeSlugHit,
  RecipeView,
  SaveLogEntryInput,
  Visibility
}

const byId = (id: string) => `?id=${encodeURIComponent(id)}`

// Media keys (media/<ulid>.<ext>) are served at the API's /media/* CloudFront behavior; compose the
// absolute URL for <img src>. Null-safe: no key → no photo.
export const mediaUrl = (key?: string | null): string | null =>
  key ? `${apiUrl()}/${key}` : null

// Keyset page query for the catalog reads: the sort, a cursor (the last card's id; plus its name for
// the name sorts), and a page limit. Empty when nothing is set, so an un-paginated call still fetches
// the full (server-capped) list. Param names mirror the server's `Page` (camelCase) 1:1.
export type SortOrder = "newest" | "oldest" | "name_asc" | "name_desc"
export type PageQuery = {
  sort?: SortOrder
  cursor?: string
  cursorName?: string
  limit?: number
}
const pageParams = (page: PageQuery = {}) => {
  const p = new URLSearchParams()
  if (page.sort) p.set("sort", page.sort)
  if (page.cursor) p.set("cursor", page.cursor)
  if (page.cursorName) p.set("cursorName", page.cursorName)
  if (page.limit != null) p.set("limit", String(page.limit))
  const s = p.toString()
  return s ? `?${s}` : ""
}

// --- reads (the backend scopes each to the session user, from the forwarded Bearer token) ---

export const listRecipeCards = (page?: PageQuery) =>
  api<RecipeCard[]>(`/api/content/recipes${pageParams(page)}`)
export const getRecipeView = (id: string) =>
  api<RecipeView | null>(`/api/content/recipe-detail${byId(id)}`)
export const getRecipeEdit = (id: string) =>
  api<RecipeEditData | null>(`/api/content/recipe-edit${byId(id)}`)
export const listIngredientCards = (page?: PageQuery) =>
  api<IngredientCard[]>(`/api/content/ingredients${pageParams(page)}`)
export const getIngredientView = (id: string) =>
  api<IngredientEditData | null>(`/api/content/ingredient-detail${byId(id)}`)
export const getIngredientEdit = (id: string) =>
  api<IngredientEditData | null>(`/api/content/ingredient-edit${byId(id)}`)
export const searchIngredients = (q: string) =>
  api<IngredientSearchItem[]>(`/api/content/search?q=${encodeURIComponent(q)}`)
// Public profile by handle (optionally-authed: the forwarded cookie lets a viewer see their own
// non-public recipes on their own profile). Null when the handle has no account.
export const getProfile = (username: string) =>
  api<Profile | null>(
    `/api/content/profile?username=${encodeURIComponent(username)}`
  )
// Resolve /<username>/<slug> → { recipeId, canonicalSlug } (or null → 404).
export const resolveRecipeBySlug = (username: string, slug: string) =>
  api<RecipeSlugHit | null>(
    `/api/content/recipe-by-slug?username=${encodeURIComponent(username)}&slug=${encodeURIComponent(slug)}`
  )
// Resolve /ingredients/<slug> → { ingredientId, canonicalSlug } (or null → 404).
export const resolveIngredientBySlug = (slug: string) =>
  api<IngredientSlugHit | null>(
    `/api/content/ingredient-by-slug?slug=${encodeURIComponent(slug)}`
  )
// Blog (DB-backed CMS) — public, unauthenticated content.
export const listBlogPosts = () => api<BlogSummary[]>(`/api/content/blog`)
export const getBlogPost = (slug: string) =>
  api<BlogPostData | null>(
    `/api/content/blog-detail?slug=${encodeURIComponent(slug)}`
  )

// --- diary (PRIVATE per-user food log; the backend hard-401s without a session, and it is never in
// the anonymous content pull or the sitemap). `date` is a user-local YYYY-MM-DD chosen client-side. ---
export const getLogDay = (date: string) =>
  api<DayLog>(`/api/log/day?date=${encodeURIComponent(date)}`)
export const getLogRecents = (limit = 20) =>
  api<RecentIngredient[]>(`/api/log/recents?limit=${limit}`)
export const saveLogEntry = (input: SaveLogEntryInput): Promise<string> =>
  api<{ id: string }>("/api/log/entries", { method: "POST", body: input }).then(
    (r) => r.id
  )
export const deleteLogEntry = (id: string): Promise<void> =>
  api(`/api/log/entries${byId(id)}`, { method: "DELETE" }).then(() => undefined)

// Nutrition PROFILE (PRIVATE; drives personalized vegan-aware targets). GET returns defaults (all
// null) when never set; POST upserts the single per-user row.
export const getNutritionProfile = () => api<NutritionProfile>("/api/profile")
export const saveNutritionProfile = (input: NutritionProfile): Promise<void> =>
  api("/api/profile", { method: "POST", body: input }).then(() => undefined)

// --- mutations (the server stamps userId from the session — a client-supplied owner is never trusted) ---

// RecipeFormInput / IngredientFormInput are wire-compatible with vegify-core's SaveRecipeInput /
// SaveIngredientInput (camelCase; the extra optional asIngredientId + item unit default server-side),
// so the form input POSTs as-is. A supplied `id` upserts (edit); its absence mints one (create).
export const saveRecipe = (input: RecipeFormInput): Promise<string> =>
  api<{ id: string }>("/api/content/recipes", {
    method: "POST",
    body: input
  }).then((r) => r.id)
export const deleteRecipe = (id: string): Promise<void> =>
  api(`/api/content/recipes${byId(id)}`, { method: "DELETE" }).then(
    () => undefined
  )
export const saveIngredient = (input: IngredientFormInput): Promise<string> =>
  api<{ id: string }>("/api/content/ingredients", {
    method: "POST",
    body: input
  }).then((r) => r.id)
export const deleteIngredient = (id: string): Promise<void> =>
  api(`/api/content/ingredients${byId(id)}`, { method: "DELETE" }).then(
    () => undefined
  )
// Media: the server mints a presigned PUT (the client uploads straight to S3), then the key attaches.
export const requestUploadUrl = (contentType: string): Promise<UploadTicket> =>
  api<UploadTicket>("/api/content/upload-url", {
    method: "POST",
    body: { contentType }
  })
export const attachPhoto = (p: {
  recipeId?: string
  ingredientId?: string
  key: string
  contentType: string
}): Promise<void> =>
  api("/api/content/attach-photo", { method: "POST", body: p }).then(
    () => undefined
  )
export const attachAvatar = (p: {
  key: string
  contentType: string
}): Promise<void> =>
  api("/api/content/attach-avatar", { method: "POST", body: p }).then(
    () => undefined
  )

// --- UGC safety (App Review 1.2): report content/users, block users ---
export type ReportInput = {
  targetType: "ingredient" | "recipe" | "user" | "message"
  targetId: string
  reason: "spam" | "abuse" | "sexual" | "violence" | "other"
  note?: string | null
}
export const reportContent = (input: ReportInput): Promise<void> =>
  api("/api/content/report", { method: "POST", body: input }).then(
    () => undefined
  )
export const blockUser = (username: string): Promise<void> =>
  api("/api/users/block", { method: "POST", body: { username } }).then(
    () => undefined
  )
export const unblockUser = (username: string): Promise<void> =>
  api("/api/users/unblock", { method: "POST", body: { username } }).then(
    () => undefined
  )

// Undo a soft delete (the greyed recipe row's "restore?" affordance). Owner-gated server-side.
export const restoreIngredient = (id: string): Promise<void> =>
  api(`/api/content/ingredient-restore${byId(id)}`, {
    method: "POST",
    body: {}
  }).then(() => undefined)
