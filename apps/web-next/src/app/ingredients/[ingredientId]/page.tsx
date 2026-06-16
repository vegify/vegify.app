import { notFound } from "next/navigation";
import { db } from "@vegify/db";
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
} from "@vegify/ui";

export const dynamic = "force-dynamic";

export default async function IngredientPage({
  params,
}: {
  params: Promise<{ ingredientId: string }>;
}) {
  const { ingredientId } = await params;
  const ingredient = await db.query.ingredients.findFirst({
    where: (i, { eq }) => eq(i.id, Number(ingredientId)),
    with: {
      creator: true,
      servingSize: true,
      batchSize: true,
      nutrients: { with: { nutrient: true } },
    },
  });
  if (!ingredient) notFound();

  const nutrition: NutritionFactsData = {
    heading: "This Ingredient",
    caloriesPerServing:
      ingredient.caloriesPer100g != null
        ? ingredient.caloriesPer100g *
          (ingredient.servingSize?.grams ? ingredient.servingSize.grams / 100 : 1)
        : null,
    serving: ingredient.servingSize
      ? {
          amount: ingredient.servingSize.amount,
          unit: ingredient.servingSize.unit,
          grams: ingredient.servingSize.grams,
        }
      : null,
    servingsPerBatch:
      ingredient.batchSize && ingredient.servingSize?.grams
        ? ingredient.batchSize.grams / ingredient.servingSize.grams
        : null,
    readings: ingredient.nutrients.map((n) => ({
      name: n.nutrient.name,
      amountPer100g: n.amountPer100g,
      unit: n.unit,
    })),
  };

  return (
    <div className="flex">
      <div className="min-w-0 flex-1">
        <div className="mx-auto max-w-2xl p-6 lg:p-8">
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem>
                <BreadcrumbLink>@{ingredient.creator?.name ?? "user"}</BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem>
                <BreadcrumbPage>{ingredient.name}</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>

          <DetailHero
            label="Ingredient Image"
            editHref={`/ingredients/${ingredient.id}/edit`}
            className="mt-4"
          />

          <h1 className="mt-10 text-center text-4xl font-bold text-primary-dark">
            {ingredient.name}
          </h1>
          <h2 className="mt-6 text-center text-xl font-bold">Information</h2>
          <p className="mt-3 text-muted-foreground">
            {ingredient.description ?? "No description yet."}
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
  );
}
