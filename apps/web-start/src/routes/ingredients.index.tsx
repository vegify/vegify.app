import { Link, createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { buttonClasses } from '@vegify/ui'

// Standalone ingredients = ingredients that are NOT a recipe's as-ingredient (those are recipes,
// shown under /recipes). Mirrors the desktop's listIngredients view.
const getIngredients = createServerFn({ method: 'GET' }).handler(async () => {
  const { db } = await import('@vegify/db')
  const [all, recipes] = await Promise.all([
    db.query.ingredients.findMany({ orderBy: (i, { asc }) => asc(i.name) }),
    db.query.recipes.findMany({ columns: { asIngredientId: true } }),
  ])
  const recipeIngredientIds = new Set(recipes.map((r) => r.asIngredientId))
  return all.filter((i) => !recipeIngredientIds.has(i.id))
})

export const Route = createFileRoute('/ingredients/')({
  loader: () => getIngredients(),
  component: IngredientsPage,
})

function IngredientsPage() {
  const ingredients = Route.useLoaderData()
  return (
    <div className="mx-auto max-w-3xl p-8">
      <div className="mb-8 flex items-end justify-between gap-4">
        <div>
          <h1 className="mb-1 font-serif text-4xl font-bold text-primary-dark">Ingredients</h1>
          <p className="text-gray-500">{ingredients.length} ingredients</p>
        </div>
        <Link to="/ingredients/new" className={buttonClasses({ size: 'sm' })}>
          + New ingredient
        </Link>
      </div>
      {ingredients.length === 0 ? (
        <p className="text-muted-foreground">No ingredients yet — add one.</p>
      ) : (
        <div className="flex flex-col gap-4">
          {ingredients.map((i) => (
            <Link
              key={i.id}
              to="/ingredients/$ingredientId"
              params={{ ingredientId: String(i.id) }}
              className="block"
            >
              <div className="flex items-center gap-4 rounded-xl bg-card p-3 ring-1 ring-foreground/10 transition duration-150 hover:-translate-y-0.5 hover:shadow-lg hover:ring-orange/70">
                <div className="size-16 shrink-0 rounded-lg bg-muted" />
                <div className="min-w-0">
                  <h3 className="truncate font-serif text-2xl font-semibold">{i.name}</h3>
                  {i.caloriesPer100g != null ? (
                    <p className="text-sm text-muted-foreground">
                      {Math.round(i.caloriesPer100g)} cal/100g
                    </p>
                  ) : null}
                </div>
              </div>
            </Link>
          ))}
        </div>
      )}
    </div>
  )
}
