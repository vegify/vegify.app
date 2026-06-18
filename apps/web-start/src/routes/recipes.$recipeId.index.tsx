import { Link, createFileRoute, notFound } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
  DetailHero,
  NutritionFacts,
  NutritionFactsFab,
  type NutritionFactsData,
} from '@vegify/ui'

const getRecipe = createServerFn({ method: 'GET' })
  .validator((recipeId: string) => recipeId)
  .handler(async ({ data }) => {
    const { db, getRecipeNutrition } = await import('@vegify/db')
    const id = data
    const recipe = await db.query.recipes.findFirst({
      where: (r, { eq }) => eq(r.id, id),
      with: {
        asIngredient: { with: { creator: true, servingSize: true, batchSize: true } },
        items: {
          orderBy: (iir, { asc }) => [asc(iir.order)],
          with: { ingredient: true, amount: true },
        },
      },
    })
    if (!recipe) throw notFound()
    const nutrition = await getRecipeNutrition(id)
    return { recipe, nutrition }
  })

export const Route = createFileRoute('/recipes/$recipeId/')({
  loader: ({ params }) => getRecipe({ data: params.recipeId }),
  component: RecipePage,
})

function RecipePage() {
  const { recipe, nutrition: agg } = Route.useLoaderData()
  const serving = recipe.asIngredient.servingSize
  const nutrition: NutritionFactsData = {
    heading: 'This Recipe',
    serving: serving
      ? { amount: serving.amount, unit: serving.unit, grams: serving.grams }
      : null,
    servingsPerBatch:
      recipe.asIngredient.batchSize && serving?.grams
        ? recipe.asIngredient.batchSize.grams / serving.grams
        : null,
    caloriesPerServing:
      agg.caloriesPer100g != null && serving?.grams
        ? (agg.caloriesPer100g * serving.grams) / 100
        : agg.caloriesPer100g,
    readings: agg.readings,
  }

  return (
    <div className="flex">
      <div className="min-w-0 flex-1">
        <div className="mx-auto max-w-3xl p-6 lg:p-8">
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem>
                <BreadcrumbLink>@{recipe.asIngredient.creator?.name ?? 'user'}</BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem>
                <BreadcrumbPage>{recipe.asIngredient.name}</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>

          <DetailHero
            label="Recipe Image"
            editHref={`/recipes/${recipe.id}/edit`}
            className="mt-4"
          />

          <h1 className="mt-10 text-center text-4xl font-bold text-primary-dark">
            {recipe.asIngredient.name}
          </h1>
          {recipe.subtitle ? (
            <p className="mt-1 text-center text-muted-foreground">{recipe.subtitle}</p>
          ) : null}

          <h2 className="mt-8 text-center text-xl font-bold">Ingredients</h2>
          <ul className="mx-auto mt-4 grid max-w-2xl list-disc grid-cols-1 gap-x-8 gap-y-1.5 pl-5 marker:text-primary sm:grid-cols-2 lg:grid-cols-3">
            {recipe.items.map((item) => (
              <li key={item.id}>
                {item.ingredient ? (
                  <Link
                    to="/ingredients/$ingredientId"
                    params={{ ingredientId: String(item.ingredient.id) }}
                    className="hover:text-primary hover:underline"
                  >
                    {item.amount?.amount} {item.amount?.unit} {item.ingredient.name}
                  </Link>
                ) : (
                  <span>(unknown)</span>
                )}
              </li>
            ))}
          </ul>

          <h2 className="mt-8 text-center text-xl font-bold">Directions</h2>
          <p className="mt-3 text-muted-foreground">
            {recipe.directions ?? 'No directions yet.'}
          </p>
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
