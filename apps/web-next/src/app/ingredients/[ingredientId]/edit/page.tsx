import { notFound, redirect } from "next/navigation";
import { db, deleteIngredient, saveIngredient } from "@vegify/db";
import { IngredientForm, type IngredientFormDefaults, type IngredientFormInput } from "@vegify/ui";

export const dynamic = "force-dynamic";

export default async function EditIngredientPage({
  params,
}: {
  params: Promise<{ ingredientId: string }>;
}) {
  const { ingredientId } = await params;
  const id = Number(ingredientId);
  const ingredient = await db.query.ingredients.findFirst({
    where: (i, { eq }) => eq(i.id, id),
    with: {
      servingSize: true,
      batchSize: true,
      nutrients: { with: { nutrient: true } },
    },
  });
  if (!ingredient) notFound();

  const servingGrams = ingredient.servingSize?.grams ?? null;
  const scale = servingGrams ? servingGrams / 100 : 1;
  const defaults: IngredientFormDefaults = {
    id: ingredient.id,
    name: ingredient.name,
    description: ingredient.description,
    priceCents: ingredient.price,
    servingGrams,
    packageGrams: ingredient.batchSize?.grams ?? null,
    caloriesPerServing:
      ingredient.caloriesPer100g != null ? ingredient.caloriesPer100g * scale : null,
    nutrients: ingredient.nutrients.map((n) => ({
      name: n.nutrient.name,
      amountPerServing: n.amountPer100g * scale,
      unit: n.unit,
    })),
  };

  async function save(input: IngredientFormInput) {
    "use server";
    await saveIngredient(input);
    redirect(`/ingredients/${id}`);
  }

  async function del() {
    "use server";
    await deleteIngredient(id);
    redirect("/recipes");
  }

  return <IngredientForm defaults={defaults} onSave={save} onDelete={del} />;
}
