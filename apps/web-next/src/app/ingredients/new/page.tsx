import { redirect } from "next/navigation";
import { saveIngredient } from "@vegify/db";
import { IngredientForm, type IngredientFormInput } from "@vegify/ui";

export default function NewIngredientPage() {
  async function save(input: IngredientFormInput) {
    "use server";
    const id = await saveIngredient(input);
    redirect(`/ingredients/${id}`);
  }

  return <IngredientForm onSave={save} />;
}
