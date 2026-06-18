import { useEffect, useState } from 'react'
import { NutritionFacts, type NutritionFactsData } from '@vegify/ui'
import { vegifyData, type RecipeView } from './bindings'

// The SAME React shell the web app uses — but data comes from the on-device Rust DAL over
// typed IPC (vegifyData), not a server. Reads work offline; the recursive CTE runs locally.
export function App() {
  const [recipe, setRecipe] = useState<RecipeView | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    vegifyData
      .recipe(17)
      .then(setRecipe)
      .catch((e) => setError(String(e?.message ?? e)))
  }, [])

  if (error) return <div className="p-8 text-xl text-destructive">Error: {error}</div>
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
          <h1 className="mt-4 text-center text-4xl font-bold text-primary-dark">{recipe.name}</h1>
          {recipe.subtitle ? (
            <p className="mt-1 text-center text-muted-foreground">{recipe.subtitle}</p>
          ) : null}

          <h2 className="mt-8 text-center text-xl font-bold">Ingredients</h2>
          <ul className="mx-auto mt-4 grid max-w-2xl list-disc grid-cols-1 gap-x-8 gap-y-1.5 pl-5 marker:text-primary sm:grid-cols-2 lg:grid-cols-3">
            {recipe.items.map((item) => (
              <li key={item.id}>
                {item.amount.amount} {item.amount.unit} {item.name}
              </li>
            ))}
          </ul>

          <h2 className="mt-8 text-center text-xl font-bold">Directions</h2>
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
