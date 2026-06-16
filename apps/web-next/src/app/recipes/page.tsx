import Link from "next/link";
import { db } from "@vegify/db";
import { Card, CardDescription, CardHeader, CardTitle } from "@vegify/ui";

export const dynamic = "force-dynamic";

export default async function RecipesPage() {
  const recipes = await db.query.recipes.findMany({
    with: { asIngredient: true, items: true },
  });

  return (
    <div className="mx-auto max-w-3xl p-8">
      <h1 className="mb-1 text-4xl font-bold text-primary-dark">Recipes</h1>
      <p className="mb-8 text-gray-500">{recipes.length} recipes</p>
      <div className="flex flex-col gap-4">
        {recipes.map((r) => (
          <Link key={r.id} href={`/recipes/${r.id}`}>
            <Card className="transition hover:ring-primary/40">
              <CardHeader>
                <CardTitle>{r.asIngredient.name}</CardTitle>
                <CardDescription>
                  {r.subtitle} · {r.items.length} ingredients
                  {r.totalTime
                    ? ` · ${Math.round(r.totalTime / 60)}h total`
                    : ""}
                </CardDescription>
              </CardHeader>
            </Card>
          </Link>
        ))}
      </div>
    </div>
  );
}
