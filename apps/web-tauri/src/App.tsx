import { useCallback, useEffect, useState } from 'react'
import {
  AppShell,
  HomeView,
  IngredientDetailView,
  IngredientForm,
  IngredientListView,
  RecipeDetailView,
  RecipeForm,
  RecipeListView,
  SearchResultsView,
  type AppShellLinkProps,
  type IngredientDetailVM,
  type IngredientFormDefaults,
  type IngredientFormInput,
  type IngredientListItem,
  type NavLink,
  type NutritionFactsData,
  type RecipeDetailVM,
  type RecipeFormDefaults,
  type RecipeFormInput,
  type RecipeListItem,
} from '@vegify/ui'
import { vegifyData, type IngredientEditData, type RecipeCard, type RecipeView } from './bindings'

// SAME shared screens (@vegify/ui) the web app renders — but every read AND write goes through the
// on-device Rust DAL over typed IPC (vegifyData), not a server. This file is the desktop ADAPTER:
// it maps IPC results to the screens' view-models, and maps the screens' href-based navigation onto
// an in-process `view` state machine (no router). web-start is the same screens with a server +
// router adapter; the screens themselves live once, in @vegify/ui.

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

type View =
  | { mode: 'home' }
  | { mode: 'list' }
  | { mode: 'recipe'; id: string }
  | { mode: 'edit-recipe'; id: string }
  | { mode: 'new-recipe' }
  | { mode: 'ingredients' }
  | { mode: 'ingredient'; id: string }
  | { mode: 'edit-ingredient'; id: string }
  | { mode: 'new-ingredient' }

// Synthetic pathname so AppShell highlights the right nav item (it matches by path prefix).
function pathForView(view: View): string {
  switch (view.mode) {
    case 'home':
      return '/'
    case 'list':
    case 'recipe':
    case 'edit-recipe':
      return '/recipes'
    case 'new-recipe':
      return '/recipes/new'
    case 'ingredients':
    case 'ingredient':
    case 'edit-ingredient':
      return '/ingredients'
    case 'new-ingredient':
      return '/ingredients/new'
  }
}

// The desktop's router: maps an href (as the shared screens emit) to a view. This is the inverse of
// pathForView and the reason the screens can be router-agnostic — web-start uses a real router here.
function viewForHref(href: string): View | null {
  if (href === '/') return { mode: 'home' }
  if (href === '/recipes') return { mode: 'list' }
  if (href === '/recipes/new') return { mode: 'new-recipe' }
  if (href === '/ingredients') return { mode: 'ingredients' }
  if (href === '/ingredients/new') return { mode: 'new-ingredient' }
  let m: RegExpMatchArray | null
  if ((m = href.match(/^\/recipes\/([^/]+)\/edit$/))) return { mode: 'edit-recipe', id: m[1] }
  if ((m = href.match(/^\/recipes\/([^/]+)$/))) return { mode: 'recipe', id: m[1] }
  if ((m = href.match(/^\/ingredients\/([^/]+)\/edit$/))) return { mode: 'edit-ingredient', id: m[1] }
  if ((m = href.match(/^\/ingredients\/([^/]+)$/))) return { mode: 'ingredient', id: m[1] }
  return null
}

// Browser-style view history so ⌘[ / ⌘] go back / forward. `setView` pushes a new entry
// (truncating any forward history), so every existing setView(...) call just works.
function useHistory(initial: View) {
  const [state, setState] = useState<{ stack: View[]; index: number }>({ stack: [initial], index: 0 })
  const setView = useCallback((next: View) => {
    setState((s) => {
      const stack = s.stack.slice(0, s.index + 1)
      stack.push(next)
      return { stack, index: stack.length - 1 }
    })
  }, [])
  const back = useCallback(() => setState((s) => (s.index > 0 ? { ...s, index: s.index - 1 } : s)), [])
  const forward = useCallback(
    () => setState((s) => (s.index < s.stack.length - 1 ? { ...s, index: s.index + 1 } : s)),
    [],
  )
  return { view: state.stack[state.index], setView, back, forward }
}

export function App() {
  const { view, setView, back, forward } = useHistory({ mode: 'list' })
  const [recipes, setRecipes] = useState<RecipeCard[]>([])
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState<string | null>(null)
  const [search, setSearch] = useState('')

  const refresh = useCallback(() => {
    vegifyData
      .listRecipes()
      .then(setRecipes)
      .catch((e) => setError(String(e?.message ?? e)))
  }, [])
  useEffect(() => {
    refresh()
  }, [refresh])

  const run = (label: string, p: Promise<unknown>) => {
    setStatus(`${label}…`)
    p.then(() => {
      setStatus(`${label} ✓`)
      refresh()
    }).catch((e) => setError(String(e?.message ?? e)))
  }

  // The screens (and AppShell) navigate by href; the desktop maps that href to its view state.
  const navigate = useCallback(
    (href: string) => {
      setSearch('')
      const next = viewForHref(href)
      if (next) setView(next)
    },
    [setView],
  )
  const LinkComponent = useCallback(
    ({ href, children, className, ...rest }: AppShellLinkProps) => (
      <button type="button" className={className} onClick={() => navigate(href)} {...rest}>
        {children}
      </button>
    ),
    [navigate],
  )

  // Navigation keyboard shortcuts: ⌘1/2/3 jump to Home/Explore/Ingredients, ⌘K or "/" focuses
  // the search, Esc clears it, ⌘[ / ⌘] go back / forward.
  useEffect(() => {
    const focusSearch = () => document.querySelector<HTMLInputElement>('input[type="search"]')?.focus()
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey
      const el = e.target as HTMLElement | null
      const typing = el?.tagName === 'INPUT' || el?.tagName === 'TEXTAREA' || el?.isContentEditable === true
      if (meta && e.key === '1') {
        e.preventDefault()
        navigate('/')
      } else if (meta && e.key === '2') {
        e.preventDefault()
        navigate('/recipes')
      } else if (meta && e.key === '3') {
        e.preventDefault()
        navigate('/ingredients')
      } else if (meta && e.key === '[') {
        e.preventDefault()
        setSearch('')
        back()
      } else if (meta && e.key === ']') {
        e.preventDefault()
        setSearch('')
        forward()
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
  }, [navigate, back, forward])

  // Desktop-only local-first controls (Sync/Compact + theme) in the sidebar footer.
  const footer = (
    <div className="space-y-2 border-t border-white/15 pt-4">
      <div className="flex gap-2 px-3">
        <SyncButton label="Sync" onClick={() => run('Sync', vegifyData.sync())} />
        <SyncButton label="Compact" onClick={() => run('Compact', vegifyData.compact())} />
      </div>
      {status ? <p className="px-3 text-xs text-white/70">{status}</p> : null}
      <p className="px-3 text-xs text-white/45">local-first · on-device</p>
    </div>
  )

  return (
    <AppShell
      currentPath={pathForView(view)}
      LinkComponent={LinkComponent}
      footer={footer}
      ingredientsNav
      searchValue={search}
      onSearchChange={setSearch}
    >
      {error ? (
        <div className="m-6 flex items-center justify-between gap-4 rounded-lg border border-destructive/30 bg-destructive/5 px-4 py-3">
          <p className="text-sm text-destructive">Error: {error}</p>
          <button className="text-sm text-primary underline" onClick={() => setError(null)}>
            dismiss
          </button>
        </div>
      ) : null}

      {search.trim() ? (
        <SearchRoute search={search.trim()} recipes={recipes} LinkComponent={LinkComponent} />
      ) : (
        <>
          {view.mode === 'home' && <HomeView LinkComponent={LinkComponent} />}

          {view.mode === 'list' && (
            <RecipeListView recipes={recipes.map(toRecipeListItem)} LinkComponent={LinkComponent} />
          )}

          {view.mode === 'recipe' && <RecipeDetailRoute id={view.id} LinkComponent={LinkComponent} />}

          {view.mode === 'edit-recipe' && (
            <EditRecipe
              id={view.id}
              onSaved={() => setView({ mode: 'recipe', id: view.id })}
              onDeleted={() => {
                refresh()
                setView({ mode: 'list' })
              }}
            />
          )}

          {view.mode === 'new-recipe' && (
            <div className="mx-auto max-w-3xl p-6 lg:p-8">
              <RecipeForm
                onSearch={searchForForm}
                onSave={async (input: RecipeFormInput) => {
                  await saveRecipeFromForm(input)
                  refresh()
                  setView({ mode: 'list' })
                }}
              />
            </div>
          )}

          {view.mode === 'ingredients' && <IngredientListRoute LinkComponent={LinkComponent} />}

          {view.mode === 'ingredient' && <IngredientDetailRoute id={view.id} LinkComponent={LinkComponent} />}

          {view.mode === 'edit-ingredient' && (
            <EditIngredient id={view.id} onDone={() => setView({ mode: 'ingredients' })} />
          )}

          {view.mode === 'new-ingredient' && (
            <div className="mx-auto max-w-3xl p-6 lg:p-8">
              <IngredientForm
                onSave={async (input: IngredientFormInput) => {
                  await saveIngredientFromForm(input)
                  setView({ mode: 'ingredients' })
                }}
              />
            </div>
          )}
        </>
      )}
    </AppShell>
  )
}

// Search overlay: filters the already-loaded recipes + searches ingredients via the DAL, then
// renders the SHARED SearchResultsView (same component web-start would use).
function SearchRoute({
  search,
  recipes,
  LinkComponent,
}: {
  search: string
  recipes: RecipeCard[]
  LinkComponent: NavLink
}) {
  const q = search.toLowerCase()
  const recipeHits = recipes.filter((r) => r.name.toLowerCase().includes(q)).map(toRecipeListItem)
  const [ingredientHits, setIngredientHits] = useState<IngredientListItem[]>([])
  useEffect(() => {
    vegifyData
      .searchIngredients(search)
      .then((res) =>
        setIngredientHits(res.map((i) => ({ id: i.id, name: i.name, caloriesPer100g: i.caloriesPer100g }))),
      )
      .catch(() => setIngredientHits([]))
  }, [search])
  return (
    <SearchResultsView query={search} recipes={recipeHits} ingredients={ingredientHits} LinkComponent={LinkComponent} />
  )
}

type LinkProp = { LinkComponent: NavLink }

function RecipeDetailRoute({ id, LinkComponent }: { id: string } & LinkProp) {
  const [vm, setVm] = useState<RecipeDetailVM | null>(null)
  const [err, setErr] = useState<string | null>(null)
  useEffect(() => {
    vegifyData
      .recipe(id)
      .then((r) => (r ? setVm(recipeViewToVM(id, r)) : setErr('recipe not found')))
      .catch((e) => setErr(String(e?.message ?? e)))
  }, [id])
  if (err) return <div className="p-8 text-destructive">Error: {err}</div>
  if (!vm) return <div className="p-8 text-muted-foreground">Loading…</div>
  return <RecipeDetailView recipe={vm} LinkComponent={LinkComponent} />
}

function IngredientDetailRoute({ id, LinkComponent }: { id: string } & LinkProp) {
  const [vm, setVm] = useState<IngredientDetailVM | null>(null)
  const [err, setErr] = useState<string | null>(null)
  useEffect(() => {
    vegifyData
      .ingredientForEdit(id)
      .then((d) => (d ? setVm(ingredientEditToVM(d)) : setErr('ingredient not found')))
      .catch((e) => setErr(String(e?.message ?? e)))
  }, [id])
  if (err) return <div className="p-8 text-destructive">Error: {err}</div>
  if (!vm) return <div className="p-8 text-muted-foreground">Loading…</div>
  return <IngredientDetailView ingredient={vm} LinkComponent={LinkComponent} />
}

function IngredientListRoute({ LinkComponent }: LinkProp) {
  const [items, setItems] = useState<IngredientListItem[]>([])
  const [err, setErr] = useState<string | null>(null)
  useEffect(() => {
    vegifyData
      .listIngredients()
      .then((res) =>
        setItems(res.map((i) => ({ id: i.id, name: i.name, caloriesPer100g: i.caloriesPer100g }))),
      )
      .catch((e) => setErr(String(e?.message ?? e)))
  }, [])
  if (err) return <div className="p-8 text-destructive">Error: {err}</div>
  return <IngredientListView ingredients={items} LinkComponent={LinkComponent} />
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

function EditRecipe({
  id,
  onSaved,
  onDeleted,
}: {
  id: string
  onSaved: () => void
  onDeleted: () => void
}) {
  const [defaults, setDefaults] = useState<RecipeFormDefaults | null>(null)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    vegifyData
      .recipeForEdit(id)
      .then((d) => {
        if (!d) {
          setErr('recipe not found')
          return
        }
        setDefaults({
          id: d.id,
          name: d.name,
          subtitle: d.subtitle,
          directions: d.directions,
          servings: d.servings,
          items: d.items.map((it) => ({
            ingredientId: it.ingredientId,
            name: it.name,
            grams: num(it.grams),
            caloriesPer100g: it.caloriesPer100g,
            readings: it.readings.map((x) => ({
              name: x.name,
              amountPer100g: num(x.amountPer100g),
              unit: x.unit,
            })),
          })),
        })
      })
      .catch((e) => setErr(String(e?.message ?? e)))
  }, [id])

  if (err) return <div className="p-8 text-destructive">Error: {err}</div>
  if (!defaults) return <div className="p-8 text-muted-foreground">Loading…</div>

  return (
    <div className="mx-auto max-w-3xl p-6 lg:p-8">
      <RecipeForm
        defaults={defaults}
        onSearch={searchForForm}
        onSave={async (input: RecipeFormInput) => {
          await saveRecipeFromForm(input)
          onSaved()
        }}
        onDelete={async () => {
          await vegifyData.deleteRecipe(id)
          onDeleted()
        }}
      />
    </div>
  )
}

function EditIngredient({ id, onDone }: { id: string; onDone: () => void }) {
  const [defaults, setDefaults] = useState<IngredientFormDefaults | null>(null)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    vegifyData
      .ingredientForEdit(id)
      .then((d) => {
        if (!d) {
          setErr('ingredient not found')
          return
        }
        // The form shows per-serving values; the DAL stores per-100g — scale across.
        const scale = d.servingGrams ? d.servingGrams / 100 : 1
        setDefaults({
          id: d.id,
          name: d.name,
          description: d.description,
          priceCents: d.price,
          servingGrams: d.servingGrams,
          packageGrams: d.packageGrams,
          caloriesPerServing: d.caloriesPer100g != null ? d.caloriesPer100g * scale : null,
          nutrients: d.nutrients.map((n) => ({
            name: n.name,
            amountPerServing: num(n.amountPer100g) * scale,
            unit: n.unit,
          })),
        })
      })
      .catch((e) => setErr(String(e?.message ?? e)))
  }, [id])

  if (err) return <div className="p-8 text-destructive">Error: {err}</div>
  if (!defaults) return <div className="p-8 text-muted-foreground">Loading…</div>

  return (
    <div className="mx-auto max-w-3xl p-6 lg:p-8">
      <IngredientForm
        defaults={defaults}
        onSave={async (input: IngredientFormInput) => {
          await saveIngredientFromForm(input)
          onDone()
        }}
        onDelete={async () => {
          await vegifyData.deleteIngredient(id)
          onDone()
        }}
      />
    </div>
  )
}
