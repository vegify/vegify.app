import { notFound, redirect } from "next/navigation";
import {
  db,
  deleteRecipe,
  getIngredientNutrition,
  saveRecipe,
  searchIngredients,
} from "@vegify/db";
import {
  RecipeForm,
  type IngredientSearchItem,
  type RecipeFormDefaults,
  type RecipeFormInput,
} from "@vegify/ui";

export const dynamic = "force-dynamic";

export default async function EditRecipePage({
  params,
}: {
  params: Promise<{ recipeId: string }>;
}) {
  const { recipeId } = await params;
  const id = Number(recipeId);
  const recipe = await db.query.recipes.findFirst({
    where: (r, { eq }) => eq(r.id, id),
    with: {
      asIngredient: { with: { servingSize: true, batchSize: true } },
      items: {
        orderBy: (iir, { asc }) => [asc(iir.order)],
        with: { ingredient: true, amount: true },
      },
    },
  });
  if (!recipe) notFound();

  const items: NonNullable<RecipeFormDefaults["items"]> = [];
  for (const it of recipe.items) {
    if (!it.ingredient) continue;
    const n = await getIngredientNutrition(it.ingredient.id);
    items.push({
      ingredientId: it.ingredient.id,
      name: it.ingredient.name,
      grams: it.amount?.grams ?? 0,
      caloriesPer100g: n.caloriesPer100g,
      readings: n.readings,
    });
  }
  const sg = recipe.asIngredient.servingSize?.grams ?? null;
  const bg = recipe.asIngredient.batchSize?.grams ?? null;
  const defaults: RecipeFormDefaults = {
    id: recipe.id,
    name: recipe.asIngredient.name,
    subtitle: recipe.subtitle,
    directions: recipe.directions,
    servings: sg && bg ? bg / sg : 1,
    items,
  };

  async function search(query: string): Promise<IngredientSearchItem[]> {
    "use server";
    return searchIngredients(query);
  }
  async function save(input: RecipeFormInput) {
    "use server";
    await saveRecipe(input);
    redirect(`/recipes/${id}`);
  }
  async function del() {
    "use server";
    await deleteRecipe(id);
    redirect("/recipes");
  }

  return <RecipeForm defaults={defaults} onSearch={search} onSave={save} onDelete={del} />;
}
