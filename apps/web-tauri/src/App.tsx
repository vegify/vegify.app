import { useCallback, useEffect, useState, type ReactNode } from 'react'
import {
  IngredientForm,
  NutritionFacts,
  RecipeForm,
  type IngredientFormInput,
  type NutritionFactsData,
  type RecipeFormInput,
} from '@vegify/ui'
import { vegifyData, type RecipeCard, type RecipeView } from './bindings'

// The SAME React shell + shared @vegify/ui forms the web app uses — but every read AND write goes
// through the on-device Rust DAL over typed IPC (vegifyData), not a server. Edits work offline:
// each save is captured as a SQLite changeset and shipped via the blob store (S3) on sync().
type View =
  | { mode: 'list' }
  | { mode: 'recipe'; id: string }
  | { mode: 'new-recipe' }
  | { mode: 'new-ingredient' }

export function App() {
  const [view, setView] = useState<View>({ mode: 'list' })
  const [recipes, setRecipes] = useState<RecipeCard[]>([])
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState<string | null>(null)

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

  if (error)
    return (
      <div className="p-8">
        <p className="text-xl text-destructive">Error: {error}</p>
        <button className="mt-4 text-primary underline" onClick={() => setError(null)}>
          dismiss
        </button>
      </div>
    )

  return (
    <div className="min-h-screen">
      <header className="flex items-center gap-3 border-b border-border px-6 py-3">
        <button
          className="text-lg font-bold text-primary-dark"
          onClick={() => setView({ mode: 'list' })}
        >
          Vegify
        </button>
        <span className="text-xs text-muted-foreground">local-first · on-device</span>
        <div className="ml-auto flex items-center gap-2">
          {status ? <span className="text-xs text-muted-foreground">{status}</span> : null}
          <HeaderButton onClick={() => setView({ mode: 'new-recipe' })}>+ Recipe</HeaderButton>
          <HeaderButton onClick={() => setView({ mode: 'new-ingredient' })}>+ Ingredient</HeaderButton>
          <HeaderButton onClick={() => run('Sync', vegifyData.sync())}>Sync</HeaderButton>
          <HeaderButton onClick={() => run('Compact', vegifyData.compact())}>Compact</HeaderButton>
        </div>
      </header>

      {view.mode === 'list' && (
        <RecipeList recipes={recipes} onOpen={(id) => setView({ mode: 'recipe', id })} />
      )}
      {view.mode === 'recipe' && (
        <RecipeDetail
          id={view.id}
          onBack={() => setView({ mode: 'list' })}
          onDeleted={() => {
            run('Deleted', Promise.resolve())
            setView({ mode: 'list' })
          }}
        />
      )}
      {view.mode === 'new-recipe' && (
        <div className="mx-auto max-w-3xl p-6">
          <RecipeForm
            onSearch={async (q) => {
              const results = await vegifyData.searchIngredients(q)
              return results.map((r) => ({
                id: r.id,
                name: r.name,
                servingGrams: r.servingGrams,
                caloriesPer100g: r.caloriesPer100g,
                readings: r.readings.map((x) => ({
                  name: x.name,
                  amountPer100g: x.amountPer100g ?? 0,
                  unit: x.unit,
                })),
              }))
            }}
            onSave={async (input: RecipeFormInput) => {
              await vegifyData.saveRecipe({
                id: input.id ?? null,
                name: input.name,
                subtitle: input.subtitle,
                directions: input.directions,
                servingGrams: input.servingGrams,
                batchGrams: input.batchGrams,
                items: input.items.map((it) => ({
                  ingredientId: it.ingredientId,
                  grams: it.grams,
                  unit: null,
                })),
              })
              refresh()
              setView({ mode: 'list' })
            }}
          />
        </div>
      )}
      {view.mode === 'new-ingredient' && (
        <div className="mx-auto max-w-3xl p-6">
          <IngredientForm
            onSave={async (input: IngredientFormInput) => {
              await vegifyData.saveIngredient({
                id: input.id ?? null,
                name: input.name,
                description: input.description,
                price: input.price,
                caloriesPer100g: input.caloriesPer100g,
                servingGrams: input.servingGrams,
                packageGrams: input.packageGrams,
                nutrients: input.nutrients,
              })
              refresh()
              setView({ mode: 'list' })
            }}
          />
        </div>
      )}
    </div>
  )
}

function HeaderButton({ children, onClick }: { children: ReactNode; onClick: () => void }) {
  return (
    <button
      className="rounded-md border border-border px-3 py-1.5 text-sm font-medium hover:bg-muted"
      onClick={onClick}
    >
      {children}
    </button>
  )
}

function RecipeList({
  recipes,
  onOpen,
}: {
  recipes: RecipeCard[]
  onOpen: (id: string) => void
}) {
  return (
    <div className="mx-auto max-w-2xl p-6">
      <h1 className="mb-4 text-2xl font-bold text-primary-dark">Recipes</h1>
      {recipes.length === 0 ? (
        <p className="text-muted-foreground">No recipes yet — add one.</p>
      ) : (
        <ul className="divide-y divide-border">
          {recipes.map((r) => (
            <li key={r.id}>
              <button
                className="w-full py-3 text-left hover:text-primary"
                onClick={() => onOpen(r.id)}
              >
                <span className="font-medium">{r.name}</span>
                {r.subtitle ? (
                  <span className="ml-2 text-sm text-muted-foreground">{r.subtitle}</span>
                ) : null}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

function RecipeDetail({
  id,
  onBack,
  onDeleted,
}: {
  id: string
  onBack: () => void
  onDeleted: () => void
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
      ? { amount: serving.amount ?? 0, unit: serving.unit ?? 'g', grams: serving.grams ?? 0 }
      : null,
    servingsPerBatch:
      recipe.batchGrams && serving?.grams ? recipe.batchGrams / serving.grams : null,
    caloriesPerServing:
      recipe.nutrition.caloriesPer100g != null && serving?.grams
        ? (recipe.nutrition.caloriesPer100g * serving.grams) / 100
        : recipe.nutrition.caloriesPer100g,
    readings: recipe.nutrition.readings.map((r) => ({
      name: r.name,
      amountPer100g: r.amountPer100g ?? 0,
      unit: r.unit,
    })),
  }

  return (
    <div className="flex">
      <div className="min-w-0 flex-1">
        <div className="mx-auto max-w-3xl p-6 lg:p-8">
          <button className="text-sm text-primary hover:underline" onClick={onBack}>
            ← Recipes
          </button>
          <div className="mt-2 flex items-center justify-between">
            <h1 className="text-4xl font-bold text-primary-dark">{recipe.name}</h1>
            <button
              className="text-sm text-destructive hover:underline"
              onClick={() => vegifyData.deleteRecipe(recipe.id).then(onDeleted)}
            >
              Delete
            </button>
          </div>
          {recipe.subtitle ? (
            <p className="mt-1 text-muted-foreground">{recipe.subtitle}</p>
          ) : null}

          <h2 className="mt-8 text-xl font-bold">Ingredients</h2>
          <ul className="mt-4 list-disc pl-5 marker:text-primary">
            {recipe.items.map((item) => (
              <li key={item.id}>
                {item.amount.amount} {item.amount.unit} {item.name}
              </li>
            ))}
          </ul>

          <h2 className="mt-8 text-xl font-bold">Directions</h2>
          <p className="mt-3 text-muted-foreground">{recipe.directions ?? 'No directions yet.'}</p>
        </div>
      </div>

      <aside className="w-80 shrink-0 border-l border-border p-6">
        <div className="sticky top-6">
          <NutritionFacts data={nutrition} />
        </div>
      </aside>
    </div>
  )
}
