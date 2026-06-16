import { redirect } from "next/navigation";
import { saveRecipe, searchIngredients } from "@vegify/db";
import { RecipeForm, type IngredientSearchItem, type RecipeFormInput } from "@vegify/ui";

export default function NewRecipePage() {
  async function search(query: string): Promise<IngredientSearchItem[]> {
    "use server";
    return searchIngredients(query);
  }
  async function save(input: RecipeFormInput) {
    "use server";
    const id = await saveRecipe(input);
    redirect(`/recipes/${id}`);
  }

  return <RecipeForm onSearch={search} onSave={save} />;
}
