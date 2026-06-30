import { createContext, useCallback, useContext, useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import {
  Link,
  Outlet,
  RouterProvider,
  createHashHistory,
  createRootRouteWithContext,
  createRoute,
  createRouter,
  useNavigate,
  useParams,
  useRouter,
  useRouterState,
} from '@tanstack/react-router'
import {
  QueryClient,
  QueryClientProvider,
  queryOptions,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from '@tanstack/react-query'
import {
  AppShell,
  EmailVerificationBanner,
  HomeView,
  IngredientDetailView,
  ForgotPasswordView,
  IngredientForm,
  IngredientListView,
  LoginView,
  ProfileView,
  RecipeDetailView,
  RecipeForm,
  RecipeListView,
  SearchResultsView,
  SettingsView,
  SignupView,
  useChromeSearch,
  type AppShellLinkProps,
  type IngredientDetailVM,
  type IngredientFormDefaults,
  type IngredientFormInput,
  type IngredientListItem,
  type NutritionFactsData,
  type ProfileVM,
  type RecipeDetailVM,
  type RecipeFormDefaults,
  type RecipeFormInput,
  type RecipeListItem,
} from '@vegify/ui'
import {
  vegifyData,
  type AuthUser,
  type IngredientEditData,
  type RecipeCard,
  type RecipeView,
} from './bindings'

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
const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 30_000, retry: 1 } } })

// --- form helpers (shared by create + edit) ---
const searchForForm = async (q: string) => {
  const results = await vegifyData.searchIngredients(q)
  return results.map((r) => ({
    id: r.id,
    name: r.name,
    servingGrams: r.servingGrams,
    caloriesPer100g: r.caloriesPer100g,
    readings: r.readings.map((x) => ({ name: x.name, amountPer100g: num(x.amountPer100g), unit: x.unit })),
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
  vegifyData.saveRecipe({
    id: input.id ?? null,
    asIngredientId: null, // form never sets it; the sync pull supplies it when mirroring server rows
    visibility: input.visibility,
    name: input.name,
    subtitle: input.subtitle,
    directions: input.directions,
    servingGrams: input.servingGrams,
    batchGrams: input.batchGrams,
    items: input.items.map((it) => ({ ingredientId: it.ingredientId, grams: it.grams, unit: null })),
  }).then((id) => {
    scheduleSync()
    return id
  })
const saveIngredientFromForm = (input: IngredientFormInput) =>
  vegifyData.saveIngredient({
    id: input.id ?? null,
    visibility: input.visibility,
    name: input.name,
    description: input.description,
    price: input.price,
    caloriesPer100g: input.caloriesPer100g,
    servingGrams: input.servingGrams,
    packageGrams: input.packageGrams,
    nutrients: input.nutrients,
  }).then((id) => {
    scheduleSync()
    return id
  })

// --- IPC results → shared view-models (the desktop's data adapter) ---
const toRecipeListItem = (r: RecipeCard): RecipeListItem => ({ id: r.id, name: r.name, subtitle: r.subtitle })

function recipeViewToNutrition(recipe: RecipeView): NutritionFactsData {
  const serving = recipe.serving
  return {
    heading: 'This Recipe',
    serving: serving ? { amount: num(serving.amount), unit: serving.unit ?? 'g', grams: num(serving.grams) } : null,
    servingsPerBatch: recipe.batchGrams && serving?.grams ? recipe.batchGrams / serving.grams : null,
    caloriesPerServing:
      recipe.nutrition.caloriesPer100g != null && serving?.grams
        ? (recipe.nutrition.caloriesPer100g * serving.grams) / 100
        : recipe.nutrition.caloriesPer100g,
    readings: recipe.nutrition.readings.map((r) => ({ name: r.name, amountPer100g: num(r.amountPer100g), unit: r.unit })),
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
      label: `${item.amount.amount ?? ''} ${item.amount.unit ?? ''} ${item.name}`.trim(),
      href: item.recipeId ? `/recipes/${item.recipeId}` : `/ingredients/${item.id}`,
    })),
    nutrition: recipeViewToNutrition(recipe),
  }
}
function ingredientEditToVM(data: IngredientEditData): IngredientDetailVM {
  const grams = data.servingGrams
  const nutrition: NutritionFactsData = {
    heading: 'This Ingredient',
    caloriesPerServing: data.caloriesPer100g != null ? data.caloriesPer100g * (grams ? grams / 100 : 1) : null,
    serving: grams != null ? { amount: grams, unit: 'g', grams } : null,
    servingsPerBatch: data.packageGrams && grams ? data.packageGrams / grams : null,
    readings: data.nutrients.map((n) => ({ name: n.name, amountPer100g: num(n.amountPer100g), unit: n.unit })),
  }
  return { id: data.id, name: data.name, description: data.description, canEdit: data.canEdit, nutrition }
}

// --- query definitions (the DAL reads, as queryOptions; loaders prefetch them, components read them).
// Keys mirror web's exactly so the two shells stay legible side by side. The queryFn does the IPC call
// + the view-model transform, so the cache holds ready-to-render data (same as web's server fns).
const recipesQuery = queryOptions({
  queryKey: ['recipes'],
  queryFn: async () => (await vegifyData.listRecipes({})).map(toRecipeListItem),
})
const recipeDetailQuery = (id: string) =>
  queryOptions({
    queryKey: ['recipe', id],
    queryFn: async () => {
      const r = await vegifyData.recipe(id)
      return r ? recipeViewToVM(id, r) : null
    },
  })
const recipeEditQuery = (id: string) =>
  queryOptions({ queryKey: ['recipe-edit', id], queryFn: () => vegifyData.recipeForEdit(id) })
const profileQuery = (username: string) =>
  queryOptions({
    queryKey: ['profile', username],
    queryFn: async (): Promise<ProfileVM | null> => {
      const p = await vegifyData.getProfile(username)
      return p ? { username: p.username, name: p.name, recipes: p.recipes.map(toRecipeListItem) } : null
    },
  })
const ingredientsQuery = queryOptions({
  queryKey: ['ingredients'],
  queryFn: async (): Promise<IngredientListItem[]> =>
    (await vegifyData.listIngredients({})).map((i) => ({ id: i.id, name: i.name, caloriesPer100g: i.caloriesPer100g })),
})
const ingredientDetailQuery = (id: string) =>
  queryOptions({
    queryKey: ['ingredient', id],
    queryFn: async () => {
      const d = await vegifyData.ingredient(id)
      return d ? ingredientEditToVM(d) : null
    },
  })
const ingredientEditQuery = (id: string) =>
  queryOptions({ queryKey: ['ingredient-edit', id], queryFn: () => vegifyData.ingredientForEdit(id) })

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
        className="inline-flex items-center rounded-lg bg-green-dark px-4 py-2 text-sm font-semibold text-white transition hover:opacity-90"
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
    queryKey: ['search', query],
    queryFn: async () => {
      const [recipes, ings] = await Promise.all([vegifyData.listRecipes({}), vegifyData.searchIngredients(query)])
      const q = query.toLowerCase()
      return {
        recipes: recipes.filter((r) => r.name.toLowerCase().includes(q)).map(toRecipeListItem),
        ingredients: ings.map((i) => ({ id: i.id, name: i.name, caloriesPer100g: i.caloriesPer100g })),
      }
    },
    placeholderData: (prev) => prev,
  })
  const res = data ?? { recipes: [] as RecipeListItem[], ingredients: [] as IngredientListItem[] }
  return (
    <SearchResultsView query={query} recipes={res.recipes} ingredients={res.ingredients} LinkComponent={LinkComponent} />
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

  useEffect(() => {
    const focusSearch = () => document.querySelector<HTMLInputElement>('input[type="search"]')?.focus()
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey
      const el = e.target as HTMLElement | null
      const typing = el?.tagName === 'INPUT' || el?.tagName === 'TEXTAREA' || el?.isContentEditable === true
      if (meta && e.key === '1') {
        e.preventDefault()
        setSearch('')
        navigate({ to: '/' })
      } else if (meta && e.key === '2') {
        e.preventDefault()
        setSearch('')
        navigate({ to: '/recipes' })
      } else if (meta && e.key === '3') {
        e.preventDefault()
        setSearch('')
        navigate({ to: '/ingredients' })
      } else if (meta && e.key === ',') {
        e.preventDefault()
        setSearch('')
        navigate({ to: '/settings' })
      } else if (meta && e.key === '[') {
        e.preventDefault()
        setSearch('')
        router.history.back()
      } else if (meta && e.key === ']') {
        e.preventDefault()
        setSearch('')
        router.history.forward()
      } else if ((meta && e.key.toLowerCase() === 'k') || (e.key === '/' && !typing)) {
        e.preventDefault()
        focusSearch()
      } else if (e.key === 'Escape') {
        setSearch('')
        ;(document.activeElement as HTMLElement | null)?.blur?.()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [navigate, router])

  // Bootstrap + keep sync flowing: an initial pull on mount (fresh sign-in or a restored session), a
  // realtime WebSocket push (the primary trigger — the Rust client emits `server-content-changed`), a
  // 5-min safety-net poll, and a reconnect trigger. Writes schedule their own (debounced) sync. All of it
  // runs through the quiet single-flight scheduler — no manual Sync button.
  useEffect(() => {
    scheduleSync(0)
    // WS push is the primary trigger now, so the interval drops to a 5-min net for missed pushes / WS gaps.
    const interval = setInterval(() => scheduleSync(0), 5 * 60_000)
    const onOnline = () => scheduleSync(0)
    window.addEventListener('online', onOnline)
    // Realtime: pull the instant the server says content changed (another device — or our own push echo).
    const unlisten = listen('server-content-changed', () => scheduleSync(0))
    return () => {
      clearInterval(interval)
      window.removeEventListener('online', onOnline)
      unlisten.then((f) => f())
    }
  }, [])

  return (
    <AppShell
      currentPath={pathname}
      LinkComponent={LinkComponent}
      ingredientsNav
      searchValue={search}
      onSearchChange={setSearch}
      user={auth?.user ? { name: auth.user.name, email: auth.user.email, username: auth.user.username } : undefined}
      onSignOut={auth?.onSignOut}
    >
      {auth?.user && !auth.user.emailVerified ? <EmailVerificationNotice email={auth.user.email} /> : null}
      {query ? <SearchOverlay query={query} /> : <Outlet />}
    </AppShell>
  )
}

// ---- routes (mirror web's route files; loaders prefetch the on-device DAL into the QueryClient) ----
const rootRoute = createRootRouteWithContext<{ queryClient: QueryClient }>()({ component: RootChrome })

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: () => <HomeView LinkComponent={LinkComponent} />,
})

const recipesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/recipes',
  loader: ({ context }) => context.queryClient.ensureQueryData(recipesQuery),
  component: function RecipesList() {
    const auth = useContext(AuthContext)
    const { data } = useSuspenseQuery(recipesQuery)
    return <RecipeListView recipes={data} canCreate={!!auth?.user} LinkComponent={LinkComponent} />
  },
})

const recipeNewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/recipes/new',
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
            await queryClient.invalidateQueries({ queryKey: ['recipes'] }) // the list gains the new recipe
            navigate({ to: '/recipes' })
          }}
        />
      </div>
    )
  },
})

const recipeDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/recipes/$recipeId',
  loader: ({ context, params }) => context.queryClient.ensureQueryData(recipeDetailQuery(params.recipeId)),
  component: function RecipeDetail() {
    const { recipeId } = useParams({ from: '/recipes/$recipeId' })
    const { data: vm } = useSuspenseQuery(recipeDetailQuery(recipeId))
    if (!vm) return <NotFound what="recipe" />
    return <RecipeDetailView recipe={vm} LinkComponent={LinkComponent} />
  },
})

const profileRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/$username',
  loader: ({ context, params }) => context.queryClient.ensureQueryData(profileQuery(params.username)),
  component: function Profile() {
    const { username } = useParams({ from: '/$username' })
    const { data: profile } = useSuspenseQuery(profileQuery(username))
    return <ProfileView username={username} profile={profile} LinkComponent={LinkComponent} />
  },
})

const recipeEditRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/recipes/$recipeId/edit',
  loader: ({ context, params }) => context.queryClient.ensureQueryData(recipeEditQuery(params.recipeId)),
  component: function EditRecipe() {
    const auth = useContext(AuthContext)
    const { recipeId } = useParams({ from: '/recipes/$recipeId/edit' })
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
        caloriesPer100g: it.caloriesPer100g,
        readings: it.readings.map((x) => ({ name: x.name, amountPer100g: num(x.amountPer100g), unit: x.unit })),
      })),
    }
    return (
      <div className="mx-auto max-w-3xl p-6 lg:p-8">
        <RecipeForm
          defaults={defaults}
          onSearch={searchForForm}
          onSave={async (input) => {
            await saveRecipeFromForm(input)
            await queryClient.invalidateQueries({ queryKey: ['recipes'] })
            await queryClient.invalidateQueries({ queryKey: ['recipe', recipeId] })
            await queryClient.invalidateQueries({ queryKey: ['recipe-edit', recipeId] })
            navigate({ to: '/recipes/$recipeId', params: { recipeId } })
          }}
          onDelete={async () => {
            await vegifyData.deleteRecipe(recipeId)
            scheduleSync()
            queryClient.removeQueries({ queryKey: ['recipe', recipeId] })
            queryClient.removeQueries({ queryKey: ['recipe-edit', recipeId] })
            await queryClient.invalidateQueries({ queryKey: ['recipes'] })
            navigate({ to: '/recipes' })
          }}
        />
      </div>
    )
  },
})

const ingredientsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/ingredients',
  loader: ({ context }) => context.queryClient.ensureQueryData(ingredientsQuery),
  component: function IngredientsList() {
    const auth = useContext(AuthContext)
    const { data } = useSuspenseQuery(ingredientsQuery)
    return <IngredientListView ingredients={data} canCreate={!!auth?.user} LinkComponent={LinkComponent} />
  },
})

const ingredientNewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/ingredients/new',
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
            await queryClient.invalidateQueries({ queryKey: ['ingredients'] }) // list gains the new item
            navigate({ to: '/ingredients' })
          }}
        />
      </div>
    )
  },
})

const ingredientDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/ingredients/$ingredientId',
  loader: ({ context, params }) => context.queryClient.ensureQueryData(ingredientDetailQuery(params.ingredientId)),
  component: function IngredientDetail() {
    const { ingredientId } = useParams({ from: '/ingredients/$ingredientId' })
    const { data: vm } = useSuspenseQuery(ingredientDetailQuery(ingredientId))
    if (!vm) return <NotFound what="ingredient" />
    return <IngredientDetailView ingredient={vm} LinkComponent={LinkComponent} />
  },
})

const ingredientEditRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/ingredients/$ingredientId/edit',
  loader: ({ context, params }) => context.queryClient.ensureQueryData(ingredientEditQuery(params.ingredientId)),
  component: function EditIngredient() {
    const auth = useContext(AuthContext)
    const { ingredientId } = useParams({ from: '/ingredients/$ingredientId/edit' })
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
      packageGrams: d.packageGrams,
      caloriesPerServing: d.caloriesPer100g != null ? d.caloriesPer100g * scale : null,
      nutrients: d.nutrients.map((n) => ({ name: n.name, amountPerServing: num(n.amountPer100g) * scale, unit: n.unit })),
    }
    return (
      <div className="mx-auto max-w-3xl p-6 lg:p-8">
        <IngredientForm
          defaults={defaults}
          onSave={async (input) => {
            await saveIngredientFromForm(input)
            await queryClient.invalidateQueries({ queryKey: ['ingredients'] })
            await queryClient.invalidateQueries({ queryKey: ['ingredient', ingredientId] })
            await queryClient.invalidateQueries({ queryKey: ['ingredient-edit', ingredientId] })
            navigate({ to: '/ingredients/$ingredientId', params: { ingredientId } })
          }}
          onDelete={async () => {
            await vegifyData.deleteIngredient(ingredientId)
            scheduleSync()
            queryClient.removeQueries({ queryKey: ['ingredient', ingredientId] })
            queryClient.removeQueries({ queryKey: ['ingredient-edit', ingredientId] })
            await queryClient.invalidateQueries({ queryKey: ['ingredients'] })
            navigate({ to: '/ingredients' })
          }}
        />
      </div>
    )
  },
})

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings',
  component: () => <SettingsView />,
})

// /login mounts the shared auth views INSIDE the now-always-present router (the app is usable logged-out,
// so signing in is a destination, not a gate). On success: adopt the session and return home.
const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/login',
  component: function LoginPage() {
    const auth = useContext(AuthContext)
    const navigate = useNavigate()
    useEffect(() => {
      if (auth?.user) navigate({ to: '/' }) // already signed in — nothing to do here
    }, [auth?.user, navigate])
    return (
      <AuthGate
        onAuthed={(u) => {
          auth?.onAuthed(u)
          navigate({ to: '/' })
        }}
      />
    )
  },
})

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
  profileRoute,
])

// The desktop loads the SPA from a fixed entry (tauri://localhost/index.html), so a hard webview
// reload — right-click → Reload — re-runs the bundle. Memory history kept the route only in memory, so
// a reload reset to the initial entry (the recipes home) instead of the page you were on. Hash history
// parks the active route in the URL fragment: it survives a reload AND is never sent to the asset
// server (so no sub-path 404 — the reason memory history was used). Seed a fresh launch (no fragment
// yet) with /recipes to preserve the prior landing screen.
if (!window.location.hash) window.location.hash = '#/recipes'

const router = createRouter({
  routeTree,
  history: createHashHistory(),
  defaultPreload: 'intent',
  context: { queryClient },
})

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

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
        onAuthed: setUser,
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
  const [mode, setMode] = useState<'login' | 'signup' | 'forgot'>('login')
  // The shared auth views navigate via hrefs; with no router mounted yet, map each to a local mode.
  const authLink = useCallback(
    ({ href, children, className, ...rest }: AppShellLinkProps) => (
      <button
        type="button"
        className={className}
        onClick={() => setMode(href === '/signup' ? 'signup' : href === '/forgot' ? 'forgot' : 'login')}
        {...rest}
      >
        {children}
      </button>
    ),
    [],
  )
  const toError = (e: unknown) => ({
    error: String((e as { message?: string })?.message ?? e ?? 'Something went wrong.'),
  })
  if (mode === 'signup') {
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
  if (mode === 'forgot') {
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
