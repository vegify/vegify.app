import Link from "next/link";
import { notFound } from "next/navigation";
import { db } from "@vegify/db";
import { Card, CardDescription, CardTitle } from "@vegify/ui";

export const dynamic = "force-dynamic";

export default async function RecipePage({
  params,
}: {
  params: Promise<{ recipeId: string }>;
}) {
  const { recipeId } = await params;
  const recipe = await db.query.recipes.findFirst({
    where: (r, { eq }) => eq(r.id, Number(recipeId)),
    with: {
      asIngredient: true,
      items: {
        orderBy: (iir, { asc }) => [asc(iir.order)],
        with: { ingredient: true, amount: true },
      },
    },
  });
  if (!recipe) notFound();

  return (
    <main className="mx-auto max-w-3xl p-8">
      <Link href="/recipes" className="text-sm text-primary hover:underline">
        ← Recipes
      </Link>
      <h1 className="mt-2 text-4xl font-bold text-primary-dark">
        {recipe.asIngredient.name}
      </h1>
      <p className="mb-8 text-gray-500">{recipe.subtitle}</p>
      <Card>
        <CardTitle className="mb-3">Ingredients</CardTitle>
        <ul className="flex flex-col gap-2">
          {recipe.items.map((item) => (
            <li key={item.id} className="flex justify-between border-b border-gray-100 pb-2 text-sm">
              <span>{item.ingredient?.name ?? "(unknown)"}</span>
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
      </Card>
    </main>
  );
}
