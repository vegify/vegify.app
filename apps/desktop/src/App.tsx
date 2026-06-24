import { createContext, useCallback, useContext, useEffect, useState } from 'react'
import {
  Link,
  Outlet,
  RouterProvider,
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  useLoaderData,
  useNavigate,
  useParams,
  useRouter,
  useRouterState,
} from '@tanstack/react-router'
import {
  AppShell,
  HomeView,
  IngredientDetailView,
  IngredientForm,
  IngredientListView,
  LoginView,
  RecipeDetailView,
  RecipeForm,
  RecipeListView,
  SearchResultsView,
  SignupView,
  type AppShellLinkProps,
  type IngredientDetailVM,
  type IngredientFormDefaults,
  type IngredientFormInput,
  type IngredientListItem,
  type NutritionFactsData,
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
const saveRecipeFromForm = (input: RecipeFormInput) =>
  vegifyData.saveRecipe({
    id: input.id ?? null,
    visibility: input.visibility,
    name: input.name,
    subtitle: input.subtitle,
    directions: input.directions,
    servingGrams: input.servingGrams,
    batchGrams: input.batchGrams,
    items: input.items.map((it) => ({ ingredientId: it.ingredientId, grams: it.grams, unit: null })),
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
  return { id: data.id, name: data.name, description: data.description, nutrition }
}

// --- nav port: a TanStack Router <Link> (over memory history). The SAME adapter web uses. ---
function LinkComponent({ href, ...props }: AppShellLinkProps) {
  return <Link to={href} {...props} />
}

// The signed-in user + sign-out, provided by the App gate below and consumed by the chrome.
const AuthContext = createContext<{ user: AuthUser; onSignOut: () => void } | null>(null)

function NotFound({ what }: { what: string }) {
  return <div className="p-8 text-muted-foreground">No {what} found.</div>
}

function SyncButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex-1 rounded-lg bg-white/10 px-3 py-2 text-sm font-medium text-white hover:bg-white/20"
    >
      {label}
    </button>
  )
}

// Chrome search overlay — queries the DAL and renders the SHARED SearchResultsView (same as web).
function SearchOverlay({ query }: { query: string }) {
  const [res, setRes] = useState<{ recipes: RecipeListItem[]; ingredients: IngredientListItem[] }>({
    recipes: [],
    ingredients: [],
  })
  useEffect(() => {
    let active = true
    Promise.all([vegifyData.listRecipes(), vegifyData.searchIngredients(query)])
      .then(([recipes, ings]) => {
        if (!active) return
        const q = query.toLowerCase()
        setRes({
          recipes: recipes.filter((r) => r.name.toLowerCase().includes(q)).map(toRecipeListItem),
          ingredients: ings.map((i) => ({ id: i.id, name: i.name, caloriesPer100g: i.caloriesPer100g })),
        })
      })
      .catch(() => active && setRes({ recipes: [], ingredients: [] }))
    return () => {
      active = false
    }
  }, [query])
  return (
    <SearchResultsView query={query} recipes={res.recipes} ingredients={res.ingredients} LinkComponent={LinkComponent} />
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
  const [search, setSearch] = useState('')
  const [status, setStatus] = useState<string | null>(null)

  const runSync = (label: string, p: Promise<unknown>) => {
    setStatus(`${label}…`)
    p.then(async () => {
      setStatus(`${label} ✓`)
      await router.invalidate()
    }).catch((e) => setStatus(`${label} failed: ${String((e as { message?: string })?.message ?? e)}`))
  }

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

  const footer = (
    <div className="space-y-2 border-t border-white/15 pt-4">
      <div className="flex gap-2 px-3">
        <SyncButton label="Sync" onClick={() => runSync('Sync', vegifyData.sync())} />
        <SyncButton label="Compact" onClick={() => runSync('Compact', vegifyData.compact())} />
      </div>
      {status ? <p className="px-3 text-xs text-white/70">{status}</p> : null}
      <p className="px-3 text-xs text-white/45">local-first · on-device</p>
    </div>
  )

  return (
    <AppShell
      currentPath={pathname}
      LinkComponent={LinkComponent}
      footer={footer}
      ingredientsNav
      searchValue={search}
      onSearchChange={setSearch}
      user={auth ? { name: auth.user.name, email: auth.user.email } : undefined}
      onSignOut={auth?.onSignOut}
    >
      {search.trim() ? <SearchOverlay query={search.trim()} /> : <Outlet />}
    </AppShell>
  )
}

// ---- routes (mirror web's route files; loaders call the on-device DAL) ----
const rootRoute = createRootRoute({ component: RootChrome })

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: () => <HomeView LinkComponent={LinkComponent} />,
})

const recipesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/recipes',
  loader: () => vegifyData.listRecipes(),
  component: function RecipesList() {
    const recipes = useLoaderData({ from: '/recipes' })
    return <RecipeListView recipes={recipes.map(toRecipeListItem)} LinkComponent={LinkComponent} />
  },
})

const recipeNewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/recipes/new',
  component: function NewRecipe() {
    const navigate = useNavigate()
    const router = useRouter()
    return (
      <div className="mx-auto max-w-3xl p-6 lg:p-8">
        <RecipeForm
          onSearch={searchForForm}
          onSave={async (input) => {
            await saveRecipeFromForm(input)
            await router.invalidate()
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
  loader: async ({ params }) => {
    const r = await vegifyData.recipe(params.recipeId)
    return r ? recipeViewToVM(params.recipeId, r) : null
  },
  component: function RecipeDetail() {
    const vm = useLoaderData({ from: '/recipes/$recipeId' })
    if (!vm) return <NotFound what="recipe" />
    return <RecipeDetailView recipe={vm} LinkComponent={LinkComponent} />
  },
})

const recipeEditRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/recipes/$recipeId/edit',
  loader: ({ params }) => vegifyData.recipeForEdit(params.recipeId),
  component: function EditRecipe() {
    const d = useLoaderData({ from: '/recipes/$recipeId/edit' })
    const { recipeId } = useParams({ from: '/recipes/$recipeId/edit' })
    const navigate = useNavigate()
    const router = useRouter()
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
            await router.invalidate()
            navigate({ to: '/recipes/$recipeId', params: { recipeId } })
          }}
          onDelete={async () => {
            await vegifyData.deleteRecipe(recipeId)
            await router.invalidate()
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
  loader: () => vegifyData.listIngredients(),
  component: function IngredientsList() {
    const items = useLoaderData({ from: '/ingredients' })
    return (
      <IngredientListView
        ingredients={items.map((i) => ({ id: i.id, name: i.name, caloriesPer100g: i.caloriesPer100g }))}
        LinkComponent={LinkComponent}
      />
    )
  },
})

const ingredientNewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/ingredients/new',
  component: function NewIngredient() {
    const navigate = useNavigate()
    const router = useRouter()
    return (
      <div className="mx-auto max-w-3xl p-6 lg:p-8">
        <IngredientForm
          onSave={async (input) => {
            await saveIngredientFromForm(input)
            await router.invalidate()
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
  loader: async ({ params }) => {
    const d = await vegifyData.ingredient(params.ingredientId)
    return d ? ingredientEditToVM(d) : null
  },
  component: function IngredientDetail() {
    const vm = useLoaderData({ from: '/ingredients/$ingredientId' })
    if (!vm) return <NotFound what="ingredient" />
    return <IngredientDetailView ingredient={vm} LinkComponent={LinkComponent} />
  },
})

const ingredientEditRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/ingredients/$ingredientId/edit',
  loader: ({ params }) => vegifyData.ingredientForEdit(params.ingredientId),
  component: function EditIngredient() {
    const d = useLoaderData({ from: '/ingredients/$ingredientId/edit' })
    const { ingredientId } = useParams({ from: '/ingredients/$ingredientId/edit' })
    const navigate = useNavigate()
    const router = useRouter()
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
            await router.invalidate()
            navigate({ to: '/ingredients/$ingredientId', params: { ingredientId } })
          }}
          onDelete={async () => {
            await vegifyData.deleteIngredient(ingredientId)
            await router.invalidate()
            navigate({ to: '/ingredients' })
          }}
        />
      </div>
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
])

const router = createRouter({
  routeTree,
  history: createMemoryHistory({ initialEntries: ['/recipes'] }),
  defaultPreload: 'intent',
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

  if (user === undefined) return null
  if (!user) return <AuthGate onAuthed={setUser} />
  return (
    <AuthContext.Provider
      value={{
        user,
        onSignOut: async () => {
          await vegifyData.signOut().catch(() => {})
          setUser(null)
        },
      }}
    >
      <RouterProvider router={router} />
    </AuthContext.Provider>
  )
}

// Require-an-account gate. Toggles the shared Login/Signup screens with a local mode flag and
// authenticates via the on-device DAL → web auth route. Sits OUTSIDE the router (which only mounts
// once authed); a TanStack Router migration of the guard itself would make this a route beforeLoad.
function AuthGate({ onAuthed }: { onAuthed: (user: AuthUser) => void }) {
  const [mode, setMode] = useState<'login' | 'signup'>('login')
  const authLink = useCallback(
    ({ href, children, className, ...rest }: AppShellLinkProps) => (
      <button
        type="button"
        className={className}
        onClick={() => setMode(href === '/signup' ? 'signup' : 'login')}
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
  return mode === 'login' ? (
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
  ) : (
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
