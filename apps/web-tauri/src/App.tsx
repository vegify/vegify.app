import { useCallback, useEffect, useState } from 'react'
import {
  AppShell,
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
  buttonClasses,
  DetailHero,
  IngredientForm,
  NutritionFacts,
  NutritionFactsFab,
  RecipeForm,
  type AppShellLinkProps,
  type IngredientFormDefaults,
  type IngredientFormInput,
  type NutritionFactsData,
  type RecipeFormDefaults,
  type RecipeFormInput,
} from '@vegify/ui'
import {
  vegifyData,
  type IngredientCard,
  type IngredientEditData,
  type IngredientSearchResult,
  type RecipeCard,
  type RecipeView,
} from './bindings'

// The SAME branded shell (AppShell, DetailHero, NutritionFacts, the shared forms) the web app
// renders — but every read AND write goes through the on-device Rust DAL over typed IPC
// (vegifyData), not a server. Edits work offline: each save is captured as a SQLite changeset
// and shipped via the blob store (S3) on sync(). web-start drives this same JSX through its
// router; the desktop drives it through the `view` state machine below + a router-less adapter.

// specta types every f64 wire field as `number | null` (JSON can't carry NaN/Inf), so coerce reads.
const num = (n: number | null) => n ?? 0

// Shared by create + edit: the RecipeForm ingredient picker, backed by the DAL search.
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

// Shared by create (input.id absent) + edit (input.id set → updates that recipe).
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

// Shared by create + edit: IngredientForm gives per-serving values already converted to per-100g.
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

export function App() {
  const [view, setView] = useState<View>({ mode: 'list' })
  const [recipes, setRecipes] = useState<RecipeCard[]>([])
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const [dark, setDark] = useState(() => document.documentElement.classList.contains('dark'))
  const toggleTheme = () => {
    setDark((d) => {
      const next = !d
      document.documentElement.classList.toggle('dark', next)
      return next
    })
  }

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

  // AppShell injects this for the sidebar/tab-bar nav. The web shells pass a router Link; the
  // desktop maps the shared nav hrefs onto the `view` state machine — no router needed.
  const navigate = useCallback((href: string) => {
    setSearch('')
    if (href === '/') setView({ mode: 'home' })
    else if (href === '/recipes') setView({ mode: 'list' })
    else if (href === '/ingredients') setView({ mode: 'ingredients' })
    else if (href === '/ingredients/new') setView({ mode: 'new-ingredient' })
  }, [])
  const LinkComponent = useCallback(
    ({ href, children, className, ...rest }: AppShellLinkProps) => (
      <button type="button" className={className} onClick={() => navigate(href)} {...rest}>
        {children}
      </button>
    ),
    [navigate],
  )

  // Desktop-only local-first controls + the ingredient browser entry point (web has neither),
  // tucked into the sidebar footer so the shared nav stays identical to the web shells.
  const footer = (
    <div className="space-y-2 border-t border-white/15 pt-4">
      <div className="flex gap-2 px-3">
        <SyncButton label="Sync" onClick={() => run('Sync', vegifyData.sync())} />
        <SyncButton label="Compact" onClick={() => run('Compact', vegifyData.compact())} />
      </div>
      <div className="px-3">
        <button
          type="button"
          onClick={toggleTheme}
          className="w-full rounded-lg bg-white/10 px-3 py-2 text-sm font-medium text-white hover:bg-white/20"
        >
          {dark ? '☀ Light mode' : '☾ Dark mode'}
        </button>
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
        <SearchResults
          search={search}
          recipes={recipes}
          onOpenRecipe={(id) => {
            setSearch('')
            setView({ mode: 'recipe', id })
          }}
          onOpenIngredient={(id) => {
            setSearch('')
            setView({ mode: 'ingredient', id })
          }}
        />
      ) : (
        <>
      {view.mode === 'home' && <Home onBrowse={() => setView({ mode: 'list' })} />}

      {view.mode === 'list' && (
        <RecipeList
          recipes={recipes}
          search={search}
          onOpen={(id) => setView({ mode: 'recipe', id })}
          onNew={() => setView({ mode: 'new-recipe' })}
        />
      )}

      {view.mode === 'recipe' && (
        <RecipeDetail
          id={view.id}
          onEdit={() => setView({ mode: 'edit-recipe', id: view.id })}
          onOpenIngredient={(ingId) => setView({ mode: 'ingredient', id: ingId })}
          onOpenRecipe={(rid) => setView({ mode: 'recipe', id: rid })}
        />
      )}

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

      {view.mode === 'ingredients' && (
        <IngredientList
          search={search}
          onOpen={(id) => setView({ mode: 'ingredient', id })}
          onNew={() => setView({ mode: 'new-ingredient' })}
        />
      )}

      {view.mode === 'ingredient' && (
        <IngredientDetail
          id={view.id}
          onEdit={() => setView({ mode: 'edit-ingredient', id: view.id })}
        />
      )}

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

function SearchResults({
  search,
  recipes,
  onOpenRecipe,
  onOpenIngredient,
}: {
  search: string
  recipes: RecipeCard[]
  onOpenRecipe: (id: string) => void
  onOpenIngredient: (id: string) => void
}) {
  const q = search.trim().toLowerCase()
  const recipeHits = recipes.filter((r) => r.name.toLowerCase().includes(q))
  const [ingredientHits, setIngredientHits] = useState<IngredientSearchResult[]>([])
  useEffect(() => {
    vegifyData
      .searchIngredients(search.trim())
      .then(setIngredientHits)
      .catch(() => setIngredientHits([]))
  }, [search])
  const total = recipeHits.length + ingredientHits.length
  return (
    <div className="mx-auto max-w-3xl p-8">
      <h1 className="mb-1 text-4xl font-serif font-bold text-primary-dark">Search</h1>
      <p className="mb-8 text-gray-500">
        {total} {total === 1 ? 'result' : 'results'} for “{search.trim()}”
      </p>
      {total === 0 ? (
        <p className="text-muted-foreground">No recipes or ingredients match.</p>
      ) : (
        <div className="space-y-8">
          {recipeHits.length > 0 && (
            <section>
              <h2 className="mb-3 font-serif text-xl font-bold">Recipes</h2>
              <div className="flex flex-col gap-3">
                {recipeHits.map((r) => (
                  <ResultRow
                    key={r.id}
                    name={r.name}
                    sub={r.subtitle ?? 'Recipe'}
                    onOpen={() => onOpenRecipe(r.id)}
                  />
                ))}
              </div>
            </section>
          )}
          {ingredientHits.length > 0 && (
            <section>
              <h2 className="mb-3 font-serif text-xl font-bold">Ingredients</h2>
              <div className="flex flex-col gap-3">
                {ingredientHits.map((i) => (
                  <ResultRow
                    key={i.id}
                    name={i.name}
                    sub={
                      i.caloriesPer100g != null
                        ? `${Math.round(i.caloriesPer100g)} cal/100g`
                        : 'Ingredient'
                    }
                    onOpen={() => onOpenIngredient(i.id)}
                  />
                ))}
              </div>
            </section>
          )}
        </div>
      )}
    </div>
  )
}

function ResultRow({ name, sub, onOpen }: { name: string; sub: string; onOpen: () => void }) {
  return (
    <button onClick={onOpen} className="block w-full text-left">
      <div className="flex items-center gap-4 rounded-xl bg-card p-3 ring-1 ring-foreground/10 transition duration-150 hover:-translate-y-0.5 hover:shadow-lg hover:ring-orange/70">
        <div className="size-12 shrink-0 rounded-lg bg-muted" />
        <div className="min-w-0">
          <h3 className="truncate font-serif text-xl font-semibold">{name}</h3>
          <p className="truncate text-sm text-muted-foreground">{sub}</p>
        </div>
      </div>
    </button>
  )
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

function Home({ onBrowse }: { onBrowse: () => void }) {
  return (
    <div className="mx-auto flex min-h-[70vh] max-w-3xl flex-col items-center justify-center gap-6 p-8 text-center">
      <h1 className="text-5xl font-serif font-bold text-primary-dark">Vegify</h1>
      <p className="w-full max-w-md text-lg text-gray-500">
        Micronutrition tracking for plant-based cooking — local-first desktop
      </p>
      <button onClick={onBrowse} className={buttonClasses({ size: 'lg' })}>
        Browse recipes
      </button>
    </div>
  )
}

function RecipeList({
  recipes,
  search,
  onOpen,
  onNew,
}: {
  recipes: RecipeCard[]
  search: string
  onOpen: (id: string) => void
  onNew: () => void
}) {
  const q = search.trim().toLowerCase()
  const shown = q ? recipes.filter((r) => r.name.toLowerCase().includes(q)) : recipes
  return (
    <div className="mx-auto max-w-3xl p-8">
      <div className="mb-8 flex items-end justify-between gap-4">
        <div>
          <h1 className="mb-1 text-4xl font-serif font-bold text-primary-dark">Recipes</h1>
          <p className="text-gray-500">{shown.length} recipes</p>
        </div>
        <button onClick={onNew} className={buttonClasses({ size: 'sm' })}>
          + New recipe
        </button>
      </div>
      {shown.length === 0 ? (
        <p className="text-muted-foreground">
          {search ? 'No recipes match your search.' : 'No recipes yet — add one.'}
        </p>
      ) : (
        <div className="flex flex-col gap-4">
          {shown.map((r) => (
            <button key={r.id} onClick={() => onOpen(r.id)} className="block w-full text-left">
              <div className="flex items-center gap-4 rounded-xl bg-card p-3 ring-1 ring-foreground/10 transition duration-150 hover:-translate-y-0.5 hover:shadow-lg hover:ring-orange/70">
                <div className="size-16 shrink-0 rounded-lg bg-muted" />
                <div className="min-w-0">
                  <h3 className="truncate font-serif text-2xl font-semibold">{r.name}</h3>
                  <p className="truncate text-sm text-muted-foreground">{r.subtitle ?? 'Recipe'}</p>
                </div>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

function RecipeDetail({
  id,
  onEdit,
  onOpenIngredient,
  onOpenRecipe,
}: {
  id: string
  onEdit: () => void
  onOpenIngredient: (id: string) => void
  onOpenRecipe: (id: string) => void
}) {
  const [recipe, setRecipe] = useState<RecipeView | null>(null)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    vegifyData
      .recipe(id)
      .then(setRecipe)
      .catch((e) => setErr(String(e?.message ?? e)))
  }, [id])

  if (err) return <div className="p-8 text-destructive">Error: {err}</div>
  if (!recipe) return <div className="p-8 text-muted-foreground">Loading…</div>

  const serving = recipe.serving
  const nutrition: NutritionFactsData = {
    heading: 'This Recipe',
    serving: serving
      ? { amount: num(serving.amount), unit: serving.unit ?? 'g', grams: num(serving.grams) }
      : null,
    servingsPerBatch: recipe.batchGrams && serving?.grams ? recipe.batchGrams / serving.grams : null,
    caloriesPerServing:
      recipe.nutrition.caloriesPer100g != null && serving?.grams
        ? (recipe.nutrition.caloriesPer100g * serving.grams) / 100
        : recipe.nutrition.caloriesPer100g,
    readings: recipe.nutrition.readings.map((r) => ({
      name: r.name,
      amountPer100g: num(r.amountPer100g),
      unit: r.unit,
    })),
  }

  return (
    <div className="flex">
      <div className="min-w-0 flex-1">
        <div className="mx-auto max-w-3xl p-6 lg:p-8">
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem>
                <BreadcrumbLink>@{recipe.creator ?? 'user'}</BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem>
                <BreadcrumbPage>{recipe.name}</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>

          <DetailHero label="Recipe Image" onEdit={onEdit} className="mt-4" />

          <h1 className="mt-10 text-center text-4xl font-serif font-bold text-primary-dark">{recipe.name}</h1>
          {recipe.subtitle ? (
            <p className="mt-1 text-center text-muted-foreground">{recipe.subtitle}</p>
          ) : null}

          <h2 className="mt-8 text-center font-serif text-xl font-bold">Ingredients</h2>
          <ul className="mx-auto mt-4 grid max-w-2xl list-disc grid-cols-1 gap-x-8 gap-y-1.5 pl-5 marker:text-primary sm:grid-cols-2 lg:grid-cols-3">
            {recipe.items.map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => (item.recipeId ? onOpenRecipe(item.recipeId) : onOpenIngredient(item.id))}
                  className="text-left hover:text-primary hover:underline"
                >
                  {item.amount.amount ?? ''} {item.amount.unit ?? ''} {item.name}
                </button>
              </li>
            ))}
          </ul>

          <h2 className="mt-8 text-center font-serif text-xl font-bold">Directions</h2>
          <p className="mt-3 text-muted-foreground">{recipe.directions ?? 'No directions yet.'}</p>
        </div>
      </div>

      <aside className="hidden w-80 shrink-0 border-l border-border p-6 lg:block">
        <div className="lg:sticky lg:top-6">
          <NutritionFacts data={nutrition} />
        </div>
      </aside>

      <NutritionFactsFab data={nutrition} />
    </div>
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

function IngredientList({
  search,
  onOpen,
  onNew,
}: {
  search: string
  onOpen: (id: string) => void
  onNew: () => void
}) {
  const [items, setItems] = useState<IngredientCard[]>([])
  const [err, setErr] = useState<string | null>(null)
  useEffect(() => {
    vegifyData
      .listIngredients()
      .then(setItems)
      .catch((e) => setErr(String(e?.message ?? e)))
  }, [])
  const q = search.trim().toLowerCase()
  const shown = q ? items.filter((i) => i.name.toLowerCase().includes(q)) : items
  if (err) return <div className="p-8 text-destructive">Error: {err}</div>
  return (
    <div className="mx-auto max-w-3xl p-8">
      <div className="mb-8 flex items-end justify-between gap-4">
        <div>
          <h1 className="mb-1 text-4xl font-serif font-bold text-primary-dark">Ingredients</h1>
          <p className="text-gray-500">{shown.length} ingredients</p>
        </div>
        <button onClick={onNew} className={buttonClasses({ size: 'sm' })}>
          + New ingredient
        </button>
      </div>
      {shown.length === 0 ? (
        <p className="text-muted-foreground">
          {search ? 'No ingredients match your search.' : 'No ingredients yet — add one.'}
        </p>
      ) : (
        <div className="flex flex-col gap-4">
          {shown.map((i) => (
            <button key={i.id} onClick={() => onOpen(i.id)} className="block w-full text-left">
              <div className="flex items-center gap-4 rounded-xl bg-card p-3 ring-1 ring-foreground/10 transition duration-150 hover:-translate-y-0.5 hover:shadow-lg hover:ring-orange/70">
                <div className="size-16 shrink-0 rounded-lg bg-muted" />
                <div className="min-w-0">
                  <h3 className="truncate font-serif text-2xl font-semibold">{i.name}</h3>
                  {i.caloriesPer100g != null ? (
                    <p className="text-sm text-muted-foreground">{Math.round(i.caloriesPer100g)} cal/100g</p>
                  ) : null}
                </div>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

function IngredientDetail({ id, onEdit }: { id: string; onEdit: () => void }) {
  const [data, setData] = useState<IngredientEditData | null>(null)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    vegifyData
      .ingredientForEdit(id)
      .then((d) => {
        if (!d) {
          setErr('ingredient not found')
          return
        }
        setData(d)
      })
      .catch((e) => setErr(String(e?.message ?? e)))
  }, [id])

  if (err) return <div className="p-8 text-destructive">Error: {err}</div>
  if (!data) return <div className="p-8 text-muted-foreground">Loading…</div>

  // ingredientForEdit returns per-100g + servingGrams (no human amount/unit); show grams as the unit.
  const grams = data.servingGrams
  const nutrition: NutritionFactsData = {
    heading: 'This Ingredient',
    caloriesPerServing:
      data.caloriesPer100g != null ? data.caloriesPer100g * (grams ? grams / 100 : 1) : null,
    serving: grams != null ? { amount: grams, unit: 'g', grams } : null,
    servingsPerBatch: data.packageGrams && grams ? data.packageGrams / grams : null,
    readings: data.nutrients.map((n) => ({
      name: n.name,
      amountPer100g: num(n.amountPer100g),
      unit: n.unit,
    })),
  }

  return (
    <div className="flex">
      <div className="min-w-0 flex-1">
        <div className="mx-auto max-w-2xl p-6 lg:p-8">
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem>
                <BreadcrumbLink>@user</BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem>
                <BreadcrumbPage>{data.name}</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>

          <DetailHero label="Ingredient Image" onEdit={onEdit} className="mt-4" />

          <h1 className="mt-10 text-center text-4xl font-serif font-bold text-primary-dark">{data.name}</h1>
          <h2 className="mt-6 text-center font-serif text-xl font-bold">Information</h2>
          <p className="mt-3 text-muted-foreground">{data.description ?? 'No description yet.'}</p>
        </div>
      </div>

      <aside className="hidden w-80 shrink-0 border-l border-border p-6 lg:block">
        <div className="lg:sticky lg:top-6">
          <NutritionFacts data={nutrition} />
        </div>
      </aside>

      <NutritionFactsFab data={nutrition} />
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
