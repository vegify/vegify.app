import { Link, createFileRoute, notFound } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@vegify/ui'

const getRecipe = createServerFn({ method: 'GET' })
  .validator((recipeId: string) => recipeId)
  .handler(async ({ data }) => {
    const { db } = await import('@vegify/db')
    const recipe = await db.query.recipes.findFirst({
      where: (r, { eq }) => eq(r.id, Number(data)),
      with: {
        asIngredient: true,
        items: {
          orderBy: (iir, { asc }) => [asc(iir.order)],
          with: { ingredient: true, amount: true },
        },
      },
    })
    if (!recipe) throw notFound()
    return recipe
  })

export const Route = createFileRoute('/recipes/$recipeId')({
  loader: ({ params }) => getRecipe({ data: params.recipeId }),
  component: RecipePage,
})

function RecipePage() {
  const recipe = Route.useLoaderData()
  return (
    <div className="mx-auto max-w-3xl p-8">
      <Link to="/recipes" className="text-sm text-primary hover:underline">
        ← Recipes
      </Link>
      <h1 className="mt-2 text-4xl font-bold text-primary-dark">
        {recipe.asIngredient.name}
      </h1>
      <p className="mb-8 text-gray-500">{recipe.subtitle}</p>
      <Card>
        <CardHeader>
          <CardTitle>Ingredients</CardTitle>
        </CardHeader>
        <CardContent>
          <ul className="flex flex-col gap-2">
            {recipe.items.map((item) => (
              <li
                key={item.id}
                className="flex justify-between border-b border-gray-100 pb-2 text-sm"
              >
                {item.ingredient ? (
                  <Link
                    to="/ingredients/$ingredientId"
                    params={{ ingredientId: String(item.ingredient.id) }}
                    className="hover:text-primary hover:underline"
                  >
                    {item.ingredient.name}
                  </Link>
                ) : (
                  <span>(unknown)</span>
                )}
                <span className="text-gray-500">
                  {item.amount.amount} {item.amount.unit} · {item.amount.grams} g
                </span>
              </li>
            ))}
          </ul>
          {recipe.asIngredient.description ? (
            <CardDescription className="mt-4">
              {recipe.asIngredient.description}
            </CardDescription>
          ) : null}
        </CardContent>
      </Card>
    </div>
  )
}
