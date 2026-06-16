import { Link, createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { buttonClasses, Card, CardDescription, CardHeader, CardTitle } from '@vegify/ui'

const getRecipes = createServerFn({ method: 'GET' }).handler(async () => {
  const { db } = await import('@vegify/db')
  return db.query.recipes.findMany({
    with: { asIngredient: true, items: true },
  })
})

export const Route = createFileRoute('/recipes/')({
  loader: () => getRecipes(),
  component: RecipesPage,
})

function RecipesPage() {
  const recipes = Route.useLoaderData()
  return (
    <div className="mx-auto max-w-3xl p-8">
      <div className="mb-8 flex items-end justify-between gap-4">
        <div>
          <h1 className="mb-1 text-4xl font-bold text-primary-dark">Recipes</h1>
          <p className="text-gray-500">{recipes.length} recipes</p>
        </div>
        <Link to="/recipes/new" className={buttonClasses({ size: 'sm' })}>
          + New recipe
        </Link>
      </div>
      <div className="flex flex-col gap-4">
        {recipes.map((r) => (
          <Link
            key={r.id}
            to="/recipes/$recipeId"
            params={{ recipeId: String(r.id) }}
          >
            <Card className="transition hover:ring-primary/40">
              <CardHeader>
                <CardTitle>{r.asIngredient.name}</CardTitle>
                <CardDescription>
                  {r.subtitle} · {r.items.length} ingredients
                  {r.totalTime
                    ? ` · ${Math.round(r.totalTime / 60)}h total`
                    : ''}
                </CardDescription>
              </CardHeader>
            </Card>
          </Link>
        ))}
      </div>
    </div>
  )
}
