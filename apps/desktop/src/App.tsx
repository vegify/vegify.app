import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState
} from "react"
import {
  infiniteQueryOptions,
  QueryClient,
  QueryClientProvider,
  queryOptions,
  useQuery,
  useQueryClient,
  useSuspenseInfiniteQuery,
  useSuspenseQuery
} from "@tanstack/react-query"
import {
  createHashHistory,
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Link,
  Outlet,
  RouterProvider,
  useNavigate,
  useParams,
  useRouter,
  useRouterState
} from "@tanstack/react-router"
import { listen } from "@tauri-apps/api/event"
import {
  getCurrent as getCurrentDeepLinks,
  onOpenUrl
} from "@tauri-apps/plugin-deep-link"
import {
  isPermissionGranted,
  requestPermission,
  sendNotification
} from "@tauri-apps/plugin-notification"
import { AppShell, type AppShellLinkProps } from "@vegify/ui/app-shell"
import {
  EmailVerificationBanner,
  ForgotPasswordView,
  LoginView,
  SignupView
} from "@vegify/ui/auth-form"
import { PAGE_SIZE, parseSort, type Sort } from "@vegify/ui/catalog"
import {
  addDays,
  type DayLogAdapter,
  DayView,
  type DayVM,
  todayLocal
} from "@vegify/ui/day"
import {
  IngredientForm,
  type IngredientFormDefaults,
  type IngredientFormInput
} from "@vegify/ui/ingredient-form"
import {
  type ConversationSummary,
  MessagesView,
  ThreadView,
  type ThreadVM
} from "@vegify/ui/messages"
import {
  describeNotification,
  NotificationsView,
  type NotificationVM
} from "@vegify/ui/notifications"
import type { NutritionFactsData } from "@vegify/ui/nutrition-facts"
import {
  composeRecipeInput,
  type RecipeEditState,
  RecipeForm,
  type RecipeFormDefaults,
  type RecipeFormInput
} from "@vegify/ui/recipe-form"
import {
  HomeView,
  IngredientDetailView,
  type IngredientDetailVM,
  type IngredientEditAdapter,
  type IngredientListItem,
  IngredientListView,
  ProfileView,
  type ProfileVM,
  RecipeDetailView,
  type RecipeDetailVM,
  type RecipeEditAdapter,
  type RecipeEditRow,
  type RecipeListItem,
  RecipeListView,
  SearchResultsView,
  SettingsView
} from "@vegify/ui/screens"
import { useChromeSearch } from "@vegify/ui/use-chrome-search"
import { useEditHistory } from "@vegify/ui/use-edit-history"

import {
  type AuthUser,
  type IngredientEditData,
  type NutritionProfile,
  type RecipeCard,
  type RecipeView,
  vegifyData
} from "./bindings"

// The desktop renders the SAME shared screens (@vegify/ui) as web, over the on-device Rust DAL
// (vegifyData, typed IPC — no server). Like web it uses TanStack Router; the only differences are a
// MEMORY history (a desktop window has no URL bar) and loaders that call the DAL instead of server
// fns. This file is purely the data + routing adapter — the screens live once, in @vegify/ui.

// specta types every f64 wire field as `number | null` (JSON can't carry NaN/Inf), so coerce reads.
const num = (n: number | null) => n ?? 0

// --- TanStack Query: the DAL reads flow through one QueryClient (mirrors web's router-context
// pattern — queryClient in router context, loaders prefetch via ensureQueryData, components read via
// useSuspenseQuery). A MODULE-LEVEL singleton is correct here: the desktop is a single-window,
// single-session SPA, so there's no per-request cache isolation to worry about (unlike web SSR). The
// auto-sync pull + every mutation invalidate the relevant keys, so the screens refetch reactively
// when the local cache changes — no router.invalidate() / manual refresh. See [[server-source-of-truth]].
const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 30_000, retry: 1 } }
})

// --- form helpers (shared by create + edit) ---
const searchForForm = async (q: string) => {
  const results = await vegifyData.searchIngredients(q)
  return results.map((r) => ({
    id: r.id,
    name: r.name,
    servingGrams: r.servingGrams,
    servingUnit: r.servingUnit,
    caloriesPer100g: r.caloriesPer100g,
    readings: r.readings.map((x) => ({
      name: x.name,
      amountPer100g: num(x.amountPer100g),
      unit: x.unit
    }))
  }))
}

// --- auto-sync: a debounced, single-flight push+pull so local writes propagate to the server (and
// other devices' changes flow back) without a manual trigger. Each write schedules one; RootChrome
// also fires it periodically + on reconnect. Quiet by design — no status UI; it just keeps the local
// cache current. Invalidates the QueryClient so loaders refetch.

let syncTimer: ReturnType<typeof setTimeout> | undefined
let syncInFlight = false
let syncPending = false
function runAutoSync() {
  if (syncInFlight) {
    syncPending = true // coalesce: a write during an in-flight sync re-runs once it finishes
    return
  }
  syncInFlight = true
  syncPending = false
  vegifyData
    .syncNow()
    // A pull can change any cached entity (new/edited rows from this or another device), so invalidate
    // every active query — useSuspenseQuery refetches in the background and the screen updates in place.
    .then(() => queryClient.invalidateQueries())
    .catch(() => {}) // offline / transient — the periodic + reconnect triggers retry
    .finally(() => {
      syncInFlight = false
      if (syncPending) scheduleSync(0)
    })
}
function scheduleSync(delayMs = 1000) {
  clearTimeout(syncTimer)
  syncTimer = setTimeout(runAutoSync, delayMs)
}

const saveRecipeFromForm = (input: RecipeFormInput) =>
  vegifyData
    .saveRecipe({
      id: input.id ?? null,
      asIngredientId: null, // form never sets it; the sync pull supplies it when mirroring server rows
      visibility: input.visibility,
      name: input.name,
      subtitle: input.subtitle,
      directions: input.directions,
      servingGrams: input.servingGrams,
      batchGrams: input.batchGrams,
      items: input.items.map((it) => ({
        ingredientId: it.ingredientId,
        grams: it.grams,
        amount: it.amount,
        unit: it.unit
      }))
    })
    .then((id) => {
      scheduleSync()
      return id
    })
const saveIngredientFromForm = (input: IngredientFormInput) =>
  vegifyData
    .saveIngredient({
      id: input.id ?? null,
      visibility: input.visibility,
      name: input.name,
      description: input.description,
      price: input.price,
      caloriesPer100g: input.caloriesPer100g,
      servingGrams: input.servingGrams,
      servingUnit: input.servingUnit,
      packageGrams: input.packageGrams,
      nutrients: input.nutrients
    })
    .then((id) => {
      scheduleSync()
      return id
    })

// --- IPC results → shared view-models (the desktop's data adapter) ---
// Photos are served from the server's CloudFront (not the local cache); resolve the base ONCE at
// boot so the synchronous VM mappers can compose `<base>/<photoKey>`. Empty until resolved → the
// card/hero placeholders show, then a post-resolve invalidate fills them in.
let MEDIA_BASE = ""
export async function initMediaBase() {
  MEDIA_BASE = await vegifyData.mediaBase().catch(() => "")
}
const mediaUrl = (key?: string | null): string | null =>
  key && MEDIA_BASE ? `${MEDIA_BASE}/${key}` : null

const toRecipeListItem = (r: RecipeCard): RecipeListItem => ({
  id: r.id,
  name: r.name,
  subtitle: r.subtitle,
  photoUrl: mediaUrl(r.photoKey)
})

function recipeViewToNutrition(recipe: RecipeView): NutritionFactsData {
  const serving = recipe.serving
  return {
    heading: "This Recipe",
    serving: serving
      ? {
          amount: num(serving.amount),
          unit: serving.unit ?? "g",
          grams: num(serving.grams)
        }
      : null,
    servingsPerBatch:
      recipe.batchGrams && serving?.grams
        ? recipe.batchGrams / serving.grams
        : null,
    caloriesPerServing:
      recipe.nutrition.caloriesPer100g != null && serving?.grams
        ? (recipe.nutrition.caloriesPer100g * serving.grams) / 100
        : recipe.nutrition.caloriesPer100g,
    readings: recipe.nutrition.readings.map((r) => ({
      name: r.name,
      amountPer100g: num(r.amountPer100g),
      unit: r.unit
    }))
  }
}
function recipeViewToVM(id: string, recipe: RecipeView): RecipeDetailVM {
  return {
    id,
    name: recipe.name,
    subtitle: recipe.subtitle,
    creator: recipe.creator,
    canEdit: recipe.canEdit,
    directions: recipe.directions,
    items: recipe.items.map((item) => ({
      key: item.id,
      label:
        `${item.amount.amount ?? ""} ${item.amount.unit ?? ""} ${item.name}`.trim(),
      href: item.recipeId
        ? `/recipes/${item.recipeId}`
        : `/ingredients/${item.id}`,
      ingredientId: item.id,
      deleted: item.deleted
    })),
    nutrition: recipeViewToNutrition(recipe)
  }
}
function ingredientEditToVM(data: IngredientEditData): IngredientDetailVM {
  const grams = data.servingGrams
  const nutrition: NutritionFactsData = {
    heading: "This Ingredient",
    caloriesPerServing:
      data.caloriesPer100g != null
        ? data.caloriesPer100g * (grams ? grams / 100 : 1)
        : null,
    serving: grams != null ? { amount: grams, unit: "g", grams } : null,
    servingsPerBatch:
      data.packageGrams && grams ? data.packageGrams / grams : null,
    readings: data.nutrients.map((n) => ({
      name: n.name,
      amountPer100g: num(n.amountPer100g),
      unit: n.unit
    }))
  }
  return {
    id: data.id,
    name: data.name,
    description: data.description,
    canEdit: data.canEdit,
    deleted: data.deleted,
    creator: data.creator,
    nutrition
  }
}

// --- query definitions (the DAL reads, as queryOptions; loaders prefetch them, components read them).
// Keys mirror web's exactly so the two shells stay legible side by side. The queryFn does the IPC call
// + the view-model transform, so the cache holds ready-to-render data (same as web's server fns).
type Cursor = { id: string; name: string }
const recipesQuery = (sort: Sort) =>
  infiniteQueryOptions({
    queryKey: ["recipes", sort],
    queryFn: async ({ pageParam }) =>
      (
        await vegifyData.listRecipes({
          sort,
          cursor: pageParam?.id ?? null,
          cursorName: pageParam?.name ?? null,
          limit: PAGE_SIZE
        })
      ).map(toRecipeListItem),
    initialPageParam: undefined as Cursor | undefined,
    getNextPageParam: (last): Cursor | undefined => {
      const tail = last.at(-1)
      return !tail || last.length < PAGE_SIZE
        ? undefined
        : { id: tail.id, name: tail.name }
    }
  })
// The detail payload mirrors web's: the read VM plus, for an owner, the editable state the inline
// editor patches (visibility/servings/item-grams) and the display rows (href from recipeId). Same
// shape both shells so they stay legible side by side.
type RecipeDetailPayload = {
  vm: RecipeDetailVM
  edit: { state: RecipeEditState; rows: RecipeEditRow[] } | null
}
const recipeDetailQuery = (id: string) =>
  queryOptions({
    queryKey: ["recipe", id],
    queryFn: async (): Promise<RecipeDetailPayload | null> => {
      const r = await vegifyData.recipe(id)
      if (!r) return null
      const vm = recipeViewToVM(id, r)
      let edit: RecipeDetailPayload["edit"] = null
      if (r.canEdit) {
        const editData = await vegifyData.recipeForEdit(id)
        if (editData) {
          const hrefById = new Map(
            r.items.map((it) => [
              it.id,
              it.recipeId ? `/recipes/${it.recipeId}` : `/ingredients/${it.id}`
            ])
          )
          edit = {
            state: {
              id: editData.id,
              visibility: editData.visibility,
              name: editData.name,
              subtitle: editData.subtitle,
              directions: editData.directions,
              servings: editData.servings,
              items: editData.items.map((i) => ({
                ingredientId: i.ingredientId,
                grams: i.grams ?? 0
              }))
            },
            rows: editData.items.map((i) => ({
              ingredientId: i.ingredientId,
              name: i.name,
              grams: i.grams ?? 0,
              href:
                hrefById.get(i.ingredientId) ??
                `/ingredients/${i.ingredientId}`,
              // Per-item readings feed the LIVE nutrition recompute (scrub/type) — same as web.
              caloriesPer100g: i.caloriesPer100g,
              readings: i.readings.map((r) => ({
                ...r,
                amountPer100g: r.amountPer100g ?? 0
              }))
            }))
          }
        }
      }
      return { vm, edit }
    }
  })
const recipeEditQuery = (id: string) =>
  queryOptions({
    queryKey: ["recipe-edit", id],
    queryFn: () => vegifyData.recipeForEdit(id)
  })
const profileQuery = (username: string) =>
  queryOptions({
    queryKey: ["profile", username],
    queryFn: async (): Promise<ProfileVM | null> => {
      const p = await vegifyData.getProfile(username)
      if (!p) return null
      return {
        username: p.username,
        name: p.name,
        avatarUrl: mediaUrl(p.avatarKey),
        recipes: p.recipes.map(toRecipeListItem),
        // id-only mapping (no slug/username): the desktop links by id everywhere — offline-first,
        // no slug-resolution routes in the shell.
        ingredients: p.ingredients.map((i) => ({
          id: i.id,
          name: i.name,
          caloriesPer100g: i.caloriesPer100g
        }))
      }
    }
  })
const ingredientsQuery = (sort: Sort) =>
  infiniteQueryOptions({
    queryKey: ["ingredients", sort],
    queryFn: async ({ pageParam }): Promise<IngredientListItem[]> =>
      (
        await vegifyData.listIngredients({
          sort,
          cursor: pageParam?.id ?? null,
          cursorName: pageParam?.name ?? null,
          limit: PAGE_SIZE
        })
      ).map((i) => ({
        id: i.id,
        name: i.name,
        caloriesPer100g: i.caloriesPer100g
      })),
    initialPageParam: undefined as Cursor | undefined,
    getNextPageParam: (last): Cursor | undefined => {
      const tail = last.at(-1)
      return !tail || last.length < PAGE_SIZE
        ? undefined
        : { id: tail.id, name: tail.name }
    }
  })
// Mirrors the recipe payload: the read VM plus, for an owner, the full editable data the inline
// editor patches (a 1:1 save map — the ingredient stores everything per-100g, no derivation).
type IngredientDetailPayload = {
  vm: IngredientDetailVM
  edit: IngredientEditData | null
}
const ingredientDetailQuery = (id: string) =>
  queryOptions({
    queryKey: ["ingredient", id],
    queryFn: async (): Promise<IngredientDetailPayload | null> => {
      const d = await vegifyData.ingredient(id)
      if (!d) return null
      return { vm: ingredientEditToVM(d), edit: d.canEdit ? d : null }
    }
  })
const ingredientEditQuery = (id: string) =>
  queryOptions({
    queryKey: ["ingredient-edit", id],
    queryFn: () => vegifyData.ingredientForEdit(id)
  })

// --- nav port: a TanStack Router <Link> (over memory history). The SAME adapter web uses. ---
function LinkComponent({ href, ...props }: AppShellLinkProps) {
  return <Link to={href} {...props} />
}

// Auth state + actions, provided by App and consumed by the chrome and the /login route. Always present
// once App has mounted; `user` is null when logged out (the app stays fully usable — public browsing).
const AuthContext = createContext<{
  user: AuthUser | null
  onSignOut: () => void
  onAuthed: (user: AuthUser) => void
} | null>(null)

function NotFound({ what }: { what: string }) {
  return <div className="p-8 text-muted-foreground">No {what} found.</div>
}

// Logged-out guard for the create/edit routes. The entry points (New buttons, edit FABs) are already
// hidden when logged out and the DAL refuses anonymous writes — this just gives a direct-URL visitor a way
// in instead of an empty form they can't save.
function SignInRequired({ action }: { action: string }) {
  return (
    <div className="mx-auto max-w-3xl p-8 text-center">
      <p className="mb-4 text-muted-foreground">Sign in to {action}.</p>
      <Link
        to="/login"
        className="inline-flex items-center rounded-lg bg-green-dark px-4 py-2 font-semibold text-sm text-white transition hover:opacity-90"
      >
        Sign in
      </Link>
    </div>
  )
}

// Chrome search overlay — queries the DAL and renders the SHARED SearchResultsView (same as web).
// A TanStack Query keyed on the query string; placeholderData keeps the prior results on screen while
// the next keystroke loads, so the list doesn't flicker to empty between inputs (mirrors web).
function SearchOverlay({ query }: { query: string }) {
  const { data } = useQuery({
    queryKey: ["search", query],
    queryFn: async () => {
      const [recipes, ings] = await Promise.all([
        vegifyData.listRecipes({}),
        vegifyData.searchIngredients(query)
      ])
      const q = query.toLowerCase()
      return {
        recipes: recipes
          .filter((r) => r.name.toLowerCase().includes(q))
          .map(toRecipeListItem),
        ingredients: ings.map((i) => ({
          id: i.id,
          name: i.name,
          caloriesPer100g: i.caloriesPer100g
        }))
      }
    },
    placeholderData: (prev) => prev
  })
  const res = data ?? {
    recipes: [] as RecipeListItem[],
    ingredients: [] as IngredientListItem[]
  }
  return (
    <SearchResultsView
      query={query}
      recipes={res.recipes}
      ingredients={res.ingredients}
      LinkComponent={LinkComponent}
    />
  )
}

// The verify-email resend, lifted out of RootChrome so `email` crosses into the async onResend closure as
// a plain string — the now-nullable auth.user can't keep its narrowing across that callback boundary.
function EmailVerificationNotice({ email }: { email: string }) {
  return (
    <EmailVerificationBanner
      email={email}
      onResend={async () => {
        await vegifyData.requestEmailVerification({ email })
      }}
    />
  )
}

// Root layout: the shared AppShell wraps the routed <Outlet/>. Holds the chrome search state, the
// local-first Sync/Compact controls, and the keyboard shortcuts (⌘1/2/3 nav, ⌘[ /⌘] history,
// ⌘K // focus search, Esc clear).
function RootChrome() {
  const auth = useContext(AuthContext)
  const router = useRouter()
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const { search, setSearch, query } = useChromeSearch(pathname)
  // Unread-DM badge. The WS push → scheduleSync → invalidateQueries() chain keeps it realtime; the
  // interval is the quiet-network fallback. Signed out there is nothing to count.
  const { data: unreadMessages } = useQuery({
    queryKey: ["messages-unread"],
    queryFn: () => vegifyData.messagesUnread(),
    enabled: !!auth?.user,
    refetchInterval: 60_000
  })
  const { data: unreadNotifications } = useQuery({
    queryKey: ["notifications-unread"],
    queryFn: () => vegifyData.notificationsUnread(),
    enabled: !!auth?.user,
    refetchInterval: 60_000
  })

  useEffect(() => {
    const focusSearch = () =>
      document.querySelector<HTMLInputElement>('input[type="search"]')?.focus()
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey
      const el = e.target as HTMLElement | null
      const typing =
        el?.tagName === "INPUT" ||
        el?.tagName === "TEXTAREA" ||
        el?.isContentEditable === true
      if (meta && e.key === "1") {
        e.preventDefault()
        setSearch("")
        navigate({ to: "/" })
      } else if (meta && e.key === "2") {
        e.preventDefault()
        setSearch("")
        navigate({ to: "/recipes", search: { sort: "newest" } })
      } else if (meta && e.key === "3") {
        e.preventDefault()
        setSearch("")
        navigate({ to: "/ingredients", search: { sort: "newest" } })
      } else if (meta && e.key === ",") {
        e.preventDefault()
        setSearch("")
        navigate({ to: "/settings" })
      } else if (meta && e.key === "[") {
        e.preventDefault()
        setSearch("")
        router.history.back()
      } else if (meta && e.key === "]") {
        e.preventDefault()
        setSearch("")
        router.history.forward()
      } else if (
        (meta && e.key.toLowerCase() === "k") ||
        (e.key === "/" && !typing)
      ) {
        e.preventDefault()
        focusSearch()
      } else if (e.key === "Escape") {
        setSearch("")
        ;(document.activeElement as HTMLElement | null)?.blur?.()
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [navigate, router, setSearch])

  // Bootstrap + keep sync flowing: an initial pull on mount (fresh sign-in or a restored session), a
  // realtime WebSocket push (the primary trigger — the Rust client emits `server-content-changed`), a
  // 5-min safety-net poll, and a reconnect trigger. Writes schedule their own (debounced) sync. All of it
  // runs through the quiet single-flight scheduler — no manual Sync button.
  useEffect(() => {
    void initMediaBase().then(() => queryClient.invalidateQueries()) // resolve the media base, then let photos fill in
    scheduleSync(0)
    // WS push is the primary trigger now, so the interval drops to a 5-min net for missed pushes / WS gaps.
    const interval = setInterval(() => scheduleSync(0), 5 * 60_000)
    const onOnline = () => scheduleSync(0)
    window.addEventListener("online", onOnline)
    // Realtime: pull the instant the server says content changed (another device — or our own push echo).
    const unlisten = listen("server-content-changed", () => scheduleSync(0))
    // Bell events additionally refetch the badges and — when the window isn't focused — fire a native
    // OS toast for the newest unread entry (the sender's own client gets the event too, but it has no
    // unread rows, so toastNewestNotification no-ops).
    const unlistenNotif = listen("server-notification", () => {
      queryClient.invalidateQueries({ queryKey: ["notifications"] })
      queryClient.invalidateQueries({ queryKey: ["notifications-unread"] })
      queryClient.invalidateQueries({ queryKey: ["messages-unread"] })
      queryClient.invalidateQueries({ queryKey: ["conversations"] })
      if (!document.hasFocus()) void toastNewestNotification()
    })
    return () => {
      clearInterval(interval)
      window.removeEventListener("online", onOnline)
      unlisten.then((f) => f())
      unlistenNotif.then((f) => f())
    }
  }, [])

  return (
    <AppShell
      currentPath={pathname}
      LinkComponent={LinkComponent}
      ingredientsNav
      searchValue={search}
      onSearchChange={setSearch}
      user={
        auth?.user
          ? {
              name: auth.user.name,
              email: auth.user.email,
              username: auth.user.username
            }
          : undefined
      }
      onSignOut={auth?.onSignOut}
      unreadMessages={unreadMessages ?? 0}
      unreadNotifications={unreadNotifications ?? 0}
    >
      {auth?.user && !auth.user.emailVerified ? (
        <EmailVerificationNotice email={auth.user.email} />
      ) : null}
      {query ? <SearchOverlay query={query} /> : <Outlet />}
    </AppShell>
  )
}

// ---- routes (mirror web's route files; loaders prefetch the on-device DAL into the QueryClient) ----
const rootRoute = createRootRouteWithContext<{ queryClient: QueryClient }>()({
  component: RootChrome
})

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => <HomeView LinkComponent={LinkComponent} />
})

const recipesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/recipes",
  validateSearch: (s: { sort?: string }): { sort: Sort } => ({
    sort: parseSort(s.sort)
  }),
  loaderDeps: ({ search }) => ({ sort: search.sort }),
  loader: ({ context, deps }) =>
    context.queryClient.ensureInfiniteQueryData(recipesQuery(deps.sort)),
  component: function RecipesList() {
    const auth = useContext(AuthContext)
    const { sort } = recipesRoute.useSearch()
    const navigate = useNavigate()
    const { data, fetchNextPage, hasNextPage, isFetchingNextPage } =
      useSuspenseInfiniteQuery(recipesQuery(sort))
    return (
      <RecipeListView
        recipes={data.pages.flat()}
        canCreate={!!auth?.user}
        LinkComponent={LinkComponent}
        sort={sort}
        onSortChange={(s) => navigate({ to: "/recipes", search: { sort: s } })}
        onLoadMore={fetchNextPage}
        hasMore={hasNextPage}
        isLoadingMore={isFetchingNextPage}
      />
    )
  }
})

const recipeNewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/recipes/new",
  component: function NewRecipe() {
    const auth = useContext(AuthContext)
    const navigate = useNavigate()
    const queryClient = useQueryClient()
    if (!auth?.user) return <SignInRequired action="add a recipe" />
    return (
      <div className="mx-auto max-w-3xl p-6 lg:p-8">
        <RecipeForm
          onSearch={searchForForm}
          onSave={async (input) => {
            await saveRecipeFromForm(input)
            await queryClient.invalidateQueries({ queryKey: ["recipes"] }) // the list gains the new recipe
            navigate({ to: "/recipes", search: { sort: "newest" } })
          }}
        />
      </div>
    )
  }
})

const recipeDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/recipes/$recipeId",
  loader: ({ context, params }) =>
    context.queryClient.ensureQueryData(recipeDetailQuery(params.recipeId)),
  component: function RecipeDetail() {
    const auth = useContext(AuthContext)
    const { recipeId } = useParams({ from: "/recipes/$recipeId" })
    const { data } = useSuspenseQuery(recipeDetailQuery(recipeId))
    const navigate = useNavigate()
    const queryClient = useQueryClient()
    // Each inline commit patches one field, composes the whole-object save (shared helper — same
    // math the form uses), persists over IPC (local-first, ~instant), and invalidates so the read
    // view + list refetch. Optimistic in the primitives; a rejected save reverts the field.
    // (commit + history before the early return to keep hook order stable.)
    const commit = async (next: RecipeEditState) => {
      const id = await saveRecipeFromForm(composeRecipeInput(next))
      await queryClient.invalidateQueries({ queryKey: ["recipe", String(id)] })
      await queryClient.invalidateQueries({ queryKey: ["recipes"] })
    }
    const history = useEditHistory(commit)
    if (!data) return <NotFound what="recipe" />

    const editState = data.edit?.state
    const patch = (from: RecipeEditState, p: Partial<RecipeEditState>) => {
      history.record(from)
      return commit({ ...from, ...p })
    }

    const edit: RecipeEditAdapter | undefined =
      data.edit && editState
        ? {
            visibility: editState.visibility,
            items: data.edit.rows,
            rename: (name) => patch(editState, { name }),
            setSubtitle: (subtitle) =>
              patch(editState, { subtitle: subtitle || null }),
            setDirections: (directions) =>
              patch(editState, { directions: directions || null }),
            setVisibility: (visibility) => patch(editState, { visibility }),
            setItemAmount: (ingredientId, grams) =>
              patch(editState, {
                items: editState.items.map((i) =>
                  i.ingredientId === ingredientId ? { ...i, grams } : i
                )
              }),
            addItem: (ingredient) =>
              patch(editState, {
                items: [
                  ...editState.items,
                  {
                    ingredientId: ingredient.id,
                    grams: ingredient.servingGrams ?? 100
                  }
                ]
              }),
            removeItem: (ingredientId) =>
              patch(editState, {
                items: editState.items.filter(
                  (i) => i.ingredientId !== ingredientId
                )
              }),
            remove: async () => {
              await vegifyData.deleteRecipe(editState.id)
              scheduleSync()
              queryClient.removeQueries({ queryKey: ["recipe", editState.id] })
              await queryClient.invalidateQueries({ queryKey: ["recipes"] })
              navigate({ to: "/recipes", search: { sort: "newest" } })
            },
            search: searchForForm,
            undo: () => void history.undo(editState),
            redo: () => void history.redo(editState),
            canUndo: history.canUndo,
            canRedo: history.canRedo
          }
        : undefined

    // Restore mirrors the web: owner-only (edit presence), local-first over IPC, then sync.
    const onRestoreIngredient = edit
      ? async (ingredientId: string) => {
          await vegifyData.restoreIngredient(ingredientId)
          scheduleSync()
          await queryClient.invalidateQueries({
            queryKey: ["recipe", recipeId]
          })
          await queryClient.invalidateQueries({ queryKey: ["ingredients"] })
        }
      : undefined

    return (
      <RecipeDetailView
        recipe={data.vm}
        LinkComponent={LinkComponent}
        edit={edit}
        onRestoreIngredient={onRestoreIngredient}
        onReportContent={
          auth?.user && !edit
            ? async (reason, note) => {
                await vegifyData.reportContent("recipe", recipeId, reason, note)
              }
            : undefined
        }
      />
    )
  }
})

const profileRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/$username",
  loader: ({ context, params }) =>
    context.queryClient.ensureQueryData(profileQuery(params.username)),
  component: function Profile() {
    const auth = useContext(AuthContext)
    const { username } = useParams({ from: "/$username" })
    const { data: profile } = useSuspenseQuery(profileQuery(username))
    const queryClient = useQueryClient()
    const canModerate = !!auth?.user && auth.user.username !== username
    return (
      <ProfileView
        username={username}
        profile={profile}
        LinkComponent={LinkComponent}
        canMessage={canModerate}
        onReport={
          canModerate
            ? async (reason, note) => {
                await vegifyData.reportContent("user", username, reason, note)
              }
            : undefined
        }
        onToggleBlock={
          canModerate
            ? async () => {
                await vegifyData.blockUser(username)
                await queryClient.invalidateQueries()
              }
            : undefined
        }
      />
    )
  }
})

// ---- messages (1:1 DMs — online-only IPC proxies to the backend; auth required, like the web) ----
// ttipc emits f64 as `number | null` (like caloriesPer100g), so the queryFns normalize into the
// shared screens' strict VM types — the usual bindings→VM transform, à la toRecipeListItem.

const conversationsQuery = queryOptions({
  queryKey: ["conversations"],
  queryFn: async (): Promise<ConversationSummary[]> =>
    (await vegifyData.messageConversations()).map((c) => ({
      ...c,
      lastAt: c.lastAt ?? 0,
      unread: c.unread ?? 0
    }))
})

const threadQuery = (username: string) =>
  queryOptions({
    queryKey: ["thread", username],
    queryFn: async (): Promise<ThreadVM> => {
      const t = await vegifyData.messageThread(username)
      return {
        with: t.with,
        messages: t.messages.map((m) => ({
          ...m,
          createdAt: m.createdAt ?? 0
        }))
      }
    },
    // The WS push already schedules a sync (which invalidates every query); this interval is the
    // fallback while a thread sits open with the network quiet.
    refetchInterval: 15_000
  })

// The bell. IPC ships payload as a raw JSON string (specta has no stable JSON type) — parse here
// into the shared screen's VM.
const notificationsQuery = queryOptions({
  queryKey: ["notifications"],
  queryFn: async (): Promise<NotificationVM[]> =>
    (await vegifyData.notifications()).map((n) => ({
      id: n.id,
      kind: n.kind,
      payload: safeParse(n.payload),
      createdAt: n.createdAt ?? 0,
      read: n.read
    }))
})

function safeParse(raw: string): NotificationVM["payload"] {
  try {
    const v = JSON.parse(raw)
    return v && typeof v === "object" && !Array.isArray(v) ? v : null
  } catch {
    return null
  }
}

/** Fire a native OS toast for the newest unread bell entry — called off the WS `server-notification`
 *  event when the window isn't focused. Permission is requested lazily on first use. */
async function toastNewestNotification() {
  try {
    let granted = await isPermissionGranted()
    if (!granted) granted = (await requestPermission()) === "granted"
    if (!granted) return
    const rows = await vegifyData.notifications()
    const newest = rows.find((n) => !n.read)
    if (!newest) return
    const d = describeNotification({
      id: newest.id,
      kind: newest.kind,
      payload: safeParse(newest.payload),
      createdAt: newest.createdAt ?? 0,
      read: newest.read
    })
    sendNotification({ title: d.title, body: d.detail })
  } catch {
    // Toasts are garnish — never let them surface an error into the app.
  }
}

const notificationsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/notifications",
  component: function Notifications() {
    const auth = useContext(AuthContext)
    if (!auth?.user) return <SignInRequired action="see your notifications" />
    return <NotificationsInner />
  }
})

function NotificationsInner() {
  const queryClient = useQueryClient()
  const { data: notifications } = useSuspenseQuery(notificationsQuery)

  // Bell-standard: render with unread highlights, then mark everything read.
  useEffect(() => {
    void (async () => {
      await vegifyData.notificationsMarkRead()
      queryClient.invalidateQueries({ queryKey: ["notifications-unread"] })
      queryClient.invalidateQueries({
        queryKey: ["notifications"],
        refetchType: "none"
      })
    })()
  }, [queryClient])

  return (
    <NotificationsView
      notifications={notifications}
      LinkComponent={LinkComponent}
    />
  )
}

const messagesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/messages",
  // No loader: the fetch is auth-gated, so it runs inside the component where the gate can render.
  component: function Messages() {
    const auth = useContext(AuthContext)
    if (!auth?.user) return <SignInRequired action="read your messages" />
    return <MessagesInner />
  }
})

function MessagesInner() {
  const { data: conversations } = useSuspenseQuery(conversationsQuery)
  return (
    <MessagesView conversations={conversations} LinkComponent={LinkComponent} />
  )
}

const messageThreadRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/messages/$username",
  component: function Thread() {
    const auth = useContext(AuthContext)
    if (!auth?.user) return <SignInRequired action="read your messages" />
    return <ThreadInner />
  }
})

function ThreadInner() {
  const { username } = useParams({ from: "/messages/$username" })
  const queryClient = useQueryClient()
  const { data: thread } = useSuspenseQuery(threadQuery(username))
  const [sending, setSending] = useState(false)

  // Opening the thread consumed unread state server-side — drop the chrome badge immediately.
  useEffect(() => {
    queryClient.invalidateQueries({ queryKey: ["messages-unread"] })
  }, [queryClient, username])

  return (
    <ThreadView
      thread={thread}
      sending={sending}
      LinkComponent={LinkComponent}
      onSend={async (body) => {
        setSending(true)
        try {
          await vegifyData.sendMessage({ to: username, body })
          await queryClient.invalidateQueries({
            queryKey: ["thread", username]
          })
          queryClient.invalidateQueries({ queryKey: ["conversations"] })
        } finally {
          setSending(false)
        }
      }}
    />
  )
}

const recipeEditRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/recipes/$recipeId/edit",
  loader: ({ context, params }) =>
    context.queryClient.ensureQueryData(recipeEditQuery(params.recipeId)),
  component: function EditRecipe() {
    const auth = useContext(AuthContext)
    const { recipeId } = useParams({ from: "/recipes/$recipeId/edit" })
    const { data: d } = useSuspenseQuery(recipeEditQuery(recipeId))
    const navigate = useNavigate()
    const queryClient = useQueryClient()
    if (!auth?.user) return <SignInRequired action="edit recipes" />
    if (!d) return <NotFound what="recipe" />
    const defaults: RecipeFormDefaults = {
      id: d.id,
      visibility: d.visibility,
      name: d.name,
      subtitle: d.subtitle,
      directions: d.directions,
      servings: d.servings,
      items: d.items.map((it) => ({
        ingredientId: it.ingredientId,
        name: it.name,
        grams: num(it.grams),
        amount: num(it.amount),
        unit: it.unit,
        preferred: it.preferred,
        caloriesPer100g: it.caloriesPer100g,
        readings: it.readings.map((x) => ({
          name: x.name,
          amountPer100g: num(x.amountPer100g),
          unit: x.unit
        }))
      }))
    }
    return (
      <div className="mx-auto max-w-3xl p-6 lg:p-8">
        <RecipeForm
          defaults={defaults}
          onSearch={searchForForm}
          onSave={async (input) => {
            await saveRecipeFromForm(input)
            await queryClient.invalidateQueries({ queryKey: ["recipes"] })
            await queryClient.invalidateQueries({
              queryKey: ["recipe", recipeId]
            })
            await queryClient.invalidateQueries({
              queryKey: ["recipe-edit", recipeId]
            })
            navigate({ to: "/recipes/$recipeId", params: { recipeId } })
          }}
          onDelete={async () => {
            await vegifyData.deleteRecipe(recipeId)
            scheduleSync()
            queryClient.removeQueries({ queryKey: ["recipe", recipeId] })
            queryClient.removeQueries({ queryKey: ["recipe-edit", recipeId] })
            await queryClient.invalidateQueries({ queryKey: ["recipes"] })
            navigate({ to: "/recipes", search: { sort: "newest" } })
          }}
        />
      </div>
    )
  }
})

const ingredientsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/ingredients",
  validateSearch: (s: { sort?: string }): { sort: Sort } => ({
    sort: parseSort(s.sort)
  }),
  loaderDeps: ({ search }) => ({ sort: search.sort }),
  loader: ({ context, deps }) =>
    context.queryClient.ensureInfiniteQueryData(ingredientsQuery(deps.sort)),
  component: function IngredientsList() {
    const auth = useContext(AuthContext)
    const { sort } = ingredientsRoute.useSearch()
    const navigate = useNavigate()
    const { data, fetchNextPage, hasNextPage, isFetchingNextPage } =
      useSuspenseInfiniteQuery(ingredientsQuery(sort))
    return (
      <IngredientListView
        ingredients={data.pages.flat()}
        canCreate={!!auth?.user}
        LinkComponent={LinkComponent}
        sort={sort}
        onSortChange={(s) =>
          navigate({ to: "/ingredients", search: { sort: s } })
        }
        onLoadMore={fetchNextPage}
        hasMore={hasNextPage}
        isLoadingMore={isFetchingNextPage}
      />
    )
  }
})

const ingredientNewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/ingredients/new",
  component: function NewIngredient() {
    const auth = useContext(AuthContext)
    const navigate = useNavigate()
    const queryClient = useQueryClient()
    if (!auth?.user) return <SignInRequired action="add an ingredient" />
    return (
      <div className="mx-auto max-w-3xl p-6 lg:p-8">
        <IngredientForm
          onSave={async (input) => {
            await saveIngredientFromForm(input)
            await queryClient.invalidateQueries({ queryKey: ["ingredients"] }) // list gains the new item
            navigate({ to: "/ingredients", search: { sort: "newest" } })
          }}
        />
      </div>
    )
  }
})

const ingredientDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/ingredients/$ingredientId",
  loader: ({ context, params }) =>
    context.queryClient.ensureQueryData(
      ingredientDetailQuery(params.ingredientId)
    ),
  component: function IngredientDetail() {
    const auth = useContext(AuthContext)
    const { ingredientId } = useParams({ from: "/ingredients/$ingredientId" })
    const { data } = useSuspenseQuery(ingredientDetailQuery(ingredientId))
    const navigate = useNavigate()
    const queryClient = useQueryClient()
    // commit + history before the early return to keep hook order stable.
    const commit = async (next: IngredientEditData) => {
      const id = await saveIngredientFromForm({
        id: next.id,
        visibility: next.visibility,
        name: next.name,
        description: next.description,
        price: next.price,
        caloriesPer100g: next.caloriesPer100g,
        servingGrams: next.servingGrams,
        servingUnit: next.servingUnit,
        packageGrams: next.packageGrams,
        nutrients: next.nutrients.map((n) => ({
          name: n.name,
          amountPer100g: num(n.amountPer100g),
          unit: n.unit
        }))
      })
      await queryClient.invalidateQueries({
        queryKey: ["ingredient", String(id)]
      })
      await queryClient.invalidateQueries({ queryKey: ["ingredients"] })
      await queryClient.invalidateQueries({ queryKey: ["recipe"] })
    }
    const history = useEditHistory(commit)
    if (!data) return <NotFound what="ingredient" />

    const state = data.edit
    const patch = (
      from: IngredientEditData,
      p: Partial<IngredientEditData>
    ) => {
      history.record(from)
      return commit({ ...from, ...p })
    }

    const edit: IngredientEditAdapter | undefined = state
      ? {
          visibility: state.visibility,
          rename: (name) => patch(state, { name }),
          setDescription: (description) =>
            patch(state, { description: description || null }),
          setVisibility: (visibility) => patch(state, { visibility }),
          remove: async () => {
            await vegifyData.deleteIngredient(state.id)
            scheduleSync()
            queryClient.removeQueries({ queryKey: ["ingredient", state.id] })
            await queryClient.invalidateQueries({ queryKey: ["ingredients"] })
            navigate({ to: "/ingredients", search: { sort: "newest" } })
          },
          undo: () => void history.undo(state),
          redo: () => void history.redo(state),
          canUndo: history.canUndo,
          canRedo: history.canRedo
        }
      : undefined

    return (
      <IngredientDetailView
        ingredient={data.vm}
        LinkComponent={LinkComponent}
        edit={edit}
        onReportContent={
          auth?.user && !edit
            ? async (reason, note) => {
                await vegifyData.reportContent(
                  "ingredient",
                  ingredientId,
                  reason,
                  note
                )
              }
            : undefined
        }
      />
    )
  }
})

const ingredientEditRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/ingredients/$ingredientId/edit",
  loader: ({ context, params }) =>
    context.queryClient.ensureQueryData(
      ingredientEditQuery(params.ingredientId)
    ),
  component: function EditIngredient() {
    const auth = useContext(AuthContext)
    const { ingredientId } = useParams({
      from: "/ingredients/$ingredientId/edit"
    })
    const { data: d } = useSuspenseQuery(ingredientEditQuery(ingredientId))
    const navigate = useNavigate()
    const queryClient = useQueryClient()
    if (!auth?.user) return <SignInRequired action="edit ingredients" />
    if (!d) return <NotFound what="ingredient" />
    const scale = d.servingGrams ? d.servingGrams / 100 : 1
    const defaults: IngredientFormDefaults = {
      id: d.id,
      visibility: d.visibility,
      name: d.name,
      description: d.description,
      priceCents: d.price,
      servingGrams: d.servingGrams,
      servingUnit: d.servingUnit,
      packageGrams: d.packageGrams,
      caloriesPerServing:
        d.caloriesPer100g != null ? d.caloriesPer100g * scale : null,
      nutrients: d.nutrients.map((n) => ({
        name: n.name,
        amountPerServing: num(n.amountPer100g) * scale,
        unit: n.unit
      }))
    }
    return (
      <div className="mx-auto max-w-3xl p-6 lg:p-8">
        <IngredientForm
          defaults={defaults}
          onSave={async (input) => {
            await saveIngredientFromForm(input)
            await queryClient.invalidateQueries({ queryKey: ["ingredients"] })
            await queryClient.invalidateQueries({
              queryKey: ["ingredient", ingredientId]
            })
            await queryClient.invalidateQueries({
              queryKey: ["ingredient-edit", ingredientId]
            })
            navigate({
              to: "/ingredients/$ingredientId",
              params: { ingredientId }
            })
          }}
          onDelete={async () => {
            await vegifyData.deleteIngredient(ingredientId)
            scheduleSync()
            queryClient.removeQueries({
              queryKey: ["ingredient", ingredientId]
            })
            queryClient.removeQueries({
              queryKey: ["ingredient-edit", ingredientId]
            })
            await queryClient.invalidateQueries({ queryKey: ["ingredients"] })
            navigate({ to: "/ingredients", search: { sort: "newest" } })
          }}
        />
      </div>
    )
  }
})

// The nutrition profile (PRIVATE, authed-only) drives the diary's personalized targets. Read from the
// local-first cache (vegify_core::get_nutrition_profile); the structurally-identical bindings
// NutritionProfile passes straight into the shared form's NutritionProfileValues shape.
const nutritionProfileQuery = queryOptions({
  queryKey: ["nutrition-profile"],
  queryFn: (): Promise<NutritionProfile> => vegifyData.getNutritionProfile()
})

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: function Settings() {
    const auth = useContext(AuthContext)
    const navigate = useNavigate()
    const queryClient = useQueryClient()
    const authed = !!auth?.user
    // Authed-only: the profile query only fires (get_nutrition_profile require_uid-gates) once signed in.
    const { data: profile } = useQuery({
      ...nutritionProfileQuery,
      enabled: authed
    })
    return (
      <SettingsView
        // Render the profile form only once the profile has loaded, so the form's fields initialize from
        // the real values (the shared form seeds its state from the prop on mount).
        profile={profile}
        onSaveProfile={
          authed && profile
            ? async (values) => {
                await vegifyData.saveNutritionProfile(values)
                scheduleSync() // push the profile out; the server's other devices re-pull
                // Targets are computed from the local profile — refresh the day view and the form's read.
                await queryClient.invalidateQueries({
                  queryKey: ["nutrition-profile"]
                })
                await queryClient.invalidateQueries({ queryKey: ["diary"] })
              }
            : undefined
        }
        onDeleteAccount={
          auth?.user
            ? async (password) => {
                await vegifyData.deleteAccount(password)
                auth.onSignOut()
                queryClient.clear()
                navigate({ to: "/" })
              }
            : undefined
        }
      />
    )
  }
})

// /login mounts the shared auth views INSIDE the now-always-present router (the app is usable logged-out,
// so signing in is a destination, not a gate). On success: adopt the session and return home.
const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  component: function LoginPage() {
    const auth = useContext(AuthContext)
    const navigate = useNavigate()
    useEffect(() => {
      if (auth?.user) navigate({ to: "/" }) // already signed in — nothing to do here
    }, [auth?.user, navigate])
    return (
      <AuthGate
        onAuthed={(u) => {
          auth?.onAuthed(u)
          navigate({ to: "/" })
        }}
      />
    )
  }
})

// The PRIVATE food diary, read from the local-first cache (vegify_core::log_day) and mapped to the
// shared DayView's view-model — number|null coerced at the edge with num(), exactly like the recipe VMs.
const logDayQuery = (date: string) =>
  queryOptions({
    queryKey: ["diary", date],
    queryFn: async (): Promise<DayVM> => {
      const [day, recents] = await Promise.all([
        vegifyData.logDay(date),
        vegifyData.logRecents(20)
      ])
      return {
        date: day.date,
        entries: day.entries.map((e) => ({
          id: e.id,
          ingredientId: e.ingredientId,
          name: e.name,
          href: e.recipeId
            ? `/recipes/${e.recipeId}`
            : `/ingredients/${e.ingredientId}`,
          grams: num(e.amount.grams),
          calories: e.calories
        })),
        calories: day.calories,
        totals: day.totals.map((t) => ({
          name: t.name,
          amount: num(t.amount),
          unit: t.unit
        })),
        // Targets come from the local profile (synced from the server; edited in Settings). Generic-adult
        // until the viewer fills one in.
        targets: day.targets.map((t) => ({
          name: t.name,
          amount: num(t.amount),
          unit: t.unit,
          basis: t.basis,
          veganAdjusted: t.veganAdjusted,
          supplementCovered: t.supplementCovered,
          note: t.note ?? null
        })),
        recents: recents.map((r) => ({
          ingredientId: r.ingredientId,
          name: r.name,
          lastGrams: r.lastGrams
        })),
        supplements: {
          b12: day.supplements.b12 ?? false,
          vitD: day.supplements.vitD ?? false,
          algaeOil: day.supplements.algaeOil ?? false
        }
      }
    }
  })

const diaryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/diary",
  validateSearch: (s: { date?: string }): { date: string } => ({
    // Desktop is client-only, so todayLocal() is always the viewer's real local day (no SSR-tz issue).
    date: typeof s.date === "string" && s.date ? s.date : todayLocal()
  }),
  component: function Diary() {
    const auth = useContext(AuthContext)
    // Authed-only (private diary): the authed query lives in the inner component so it only mounts —
    // and only fires vegifyData.logDay, which require_uid-gates — once signed in.
    if (!auth?.user) return <SignInRequired action="see your food diary" />
    return <DiaryInner />
  }
})

function DiaryInner() {
  const { date } = diaryRoute.useSearch()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { data: day } = useQuery(logDayQuery(date))
  if (!day) {
    return <div className="p-8 text-center text-muted-foreground">Loading…</div>
  }
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["diary"] })
  const log: DayLogAdapter = {
    addEntry: async ({ ingredientId, grams, unit }) => {
      await vegifyData.saveLogEntry({
        id: null,
        ingredientId,
        date,
        slot: null,
        grams,
        unit: unit ?? null,
        loggedAt: null
      })
      scheduleSync()
      await refresh()
    },
    setEntryAmount: async (id, grams) => {
      const entry = day.entries.find((e) => e.id === id)
      if (!entry) return
      await vegifyData.saveLogEntry({
        id,
        ingredientId: entry.ingredientId,
        date,
        slot: null,
        grams,
        unit: null,
        loggedAt: null
      })
      scheduleSync()
      await refresh()
    },
    removeEntry: async (id) => {
      await vegifyData.deleteLogEntry(id)
      scheduleSync()
      await refresh()
    },
    search: searchForForm,
    copyYesterday: async () => {
      const src = await vegifyData.logDay(addDays(date, -1))
      for (const e of src.entries) {
        await vegifyData.saveLogEntry({
          id: null,
          ingredientId: e.ingredientId,
          date,
          slot: e.slot,
          grams: num(e.amount.grams),
          unit: e.amount.unit,
          loggedAt: null
        })
      }
      scheduleSync()
      await refresh()
    },
    setSupplements: async (next) => {
      await vegifyData.saveDaySupplements({ date, ...next })
      scheduleSync()
      await refresh()
    }
  }
  return (
    <DayView
      day={day}
      LinkComponent={LinkComponent}
      log={log}
      onNavigateDate={(d) => navigate({ to: "/diary", search: { date: d } })}
    />
  )
}

const routeTree = rootRoute.addChildren([
  homeRoute,
  recipesRoute,
  recipeNewRoute,
  recipeDetailRoute,
  recipeEditRoute,
  ingredientsRoute,
  ingredientNewRoute,
  ingredientDetailRoute,
  ingredientEditRoute,
  settingsRoute,
  loginRoute,
  messagesRoute,
  messageThreadRoute,
  notificationsRoute,
  profileRoute,
  diaryRoute
])

// The desktop loads the SPA from a fixed entry (tauri://localhost/index.html), so a hard webview
// reload — right-click → Reload — re-runs the bundle. Memory history kept the route only in memory, so
// a reload reset to the initial entry (the recipes home) instead of the page you were on. Hash history
// parks the active route in the URL fragment: it survives a reload AND is never sent to the asset
// server (so no sub-path 404 — the reason memory history was used). Seed a fresh launch (no fragment
// yet) with /recipes to preserve the prior landing screen.
if (!window.location.hash) window.location.hash = "#/recipes"

const router = createRouter({
  routeTree,
  history: createHashHistory(),
  defaultPreload: "intent",
  context: { queryClient }
})

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router
  }
}

// OS deep links: vegify://recipes/<id> (custom scheme) and https://vegify.app/recipes/<id>
// (universal links) both land here. A scheme URL parses its first segment as the URL *host*
// (vegify://recipes/x → host "recipes"), an https URL keeps the whole path in pathname — normalize
// both to a router path. getCurrent() covers a cold start (the URL is delivered before this bundle
// runs), onOpenUrl the already-running app; in 2.4.9 onOpenUrl does NOT replay the cold-start URL,
// so both are needed. Registered at module scope: history.push only moves the hash, which works
// before RouterProvider mounts (the router reads the location when it does). The OS only delivers
// deep links to a built .app bundle (universal links additionally need a signed build) — under
// `tauri dev` these never fire, and the catches keep a non-Tauri webview quiet.
function deepLinkToPath(url: string): string | null {
  try {
    const u = new URL(url)
    const path =
      u.protocol === "vegify:" ? `/${u.host}${u.pathname}` : u.pathname
    return `${path.replace(/\/+$/, "") || "/"}${u.search}`
  } catch {
    return null
  }
}
function openDeepLinks(urls: string[] | null) {
  const path = urls?.map(deepLinkToPath).find(Boolean)
  if (path) router.history.push(path)
}
getCurrentDeepLinks()
  .then(openDeepLinks)
  .catch(() => {})
void onOpenUrl(openDeepLinks).catch(() => {})

export function App() {
  // undefined = checking the keychain; null = signed out; AuthUser = signed in.
  const [user, setUser] = useState<AuthUser | null | undefined>(undefined)
  useEffect(() => {
    vegifyData
      .currentUser()
      .then((u) => setUser(u ?? null))
      .catch(() => setUser(null))
  }, [])

  if (user === undefined) return null // still checking the keychain
  return (
    <AuthContext.Provider
      value={{
        user,
        onSignOut: async () => {
          await vegifyData.signOut().catch(() => {})
          queryClient.clear() // drop the signed-out user's cached content; the next pull refills public content
          setUser(null)
        },
        onAuthed: setUser
      }}
    >
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </AuthContext.Provider>
  )
}

// Require-an-account gate. Toggles the shared Login/Signup screens with a local mode flag and
// authenticates via the on-device DAL → web auth route. Sits OUTSIDE the router (which only mounts
// once authed); a TanStack Router migration of the guard itself would make this a route beforeLoad.
function AuthGate({ onAuthed }: { onAuthed: (user: AuthUser) => void }) {
  const [mode, setMode] = useState<"login" | "signup" | "forgot">("login")
  // The shared auth views navigate via hrefs; with no router mounted yet, map each to a local mode.
  const authLink = useCallback(
    ({ href, children, className, ...rest }: AppShellLinkProps) => (
      <button
        type="button"
        className={className}
        onClick={() =>
          setMode(
            href === "/signup"
              ? "signup"
              : href === "/forgot"
                ? "forgot"
                : "login"
          )
        }
        {...rest}
      >
        {children}
      </button>
    ),
    []
  )
  const toError = (e: unknown) => ({
    error: String(
      (e as { message?: string })?.message ?? e ?? "Something went wrong."
    )
  })
  if (mode === "signup") {
    return (
      <SignupView
        LinkComponent={authLink}
        onSubmit={async ({ name, email, password }) => {
          try {
            onAuthed(await vegifyData.signUp({ name, email, password }))
          } catch (e) {
            return toError(e)
          }
        }}
      />
    )
  }
  if (mode === "forgot") {
    // The request is enumeration-safe (always resolves to the "check your email" confirmation). The
    // reset link in the email opens vegify.app/reset in the browser — desktop never holds the token.
    return (
      <ForgotPasswordView
        LinkComponent={authLink}
        onSubmit={async ({ email }) => {
          await vegifyData.requestPasswordReset({ email })
        }}
      />
    )
  }
  return (
    <LoginView
      LinkComponent={authLink}
      onSubmit={async ({ email, password }) => {
        try {
          onAuthed(await vegifyData.signIn({ email, password }))
        } catch (e) {
          return toError(e)
        }
      }}
    />
  )
}
