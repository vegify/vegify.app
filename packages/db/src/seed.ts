import { db, client } from "./index";
import {
  amounts,
  ingredientInRecipe,
  ingredientNutrient,
  ingredients,
  nutrients,
  recipes,
  users,
} from "./schema";

// Seed content ported in spirit from vegify-laravel's seeders (Caputo 00 Flour, Biga)
// plus a nested-recipe example to exercise the recipe-as-ingredient pattern.
// Nutrient figures are placeholders, not sourced data.

async function main() {
  // wipe (dev only — order respects FKs)
  await db.delete(ingredientInRecipe);
  await db.delete(recipes);
  await db.delete(ingredientNutrient);
  await db.delete(ingredients);
  await db.delete(amounts);
  await db.delete(nutrients);
  await db.delete(users);

  const [john] = await db
    .insert(users)
    .values({ name: "John", email: "dev@example.com" })
    .returning();

  const amount = async (unit: string, qty: number, grams: number) =>
    (
      await db
        .insert(amounts)
        .values({ unit, amount: qty, grams })
        .returning()
    )[0];

  const ingredient = async (opts: {
    name: string;
    description?: string;
    serving: [string, number, number];
    batch?: [string, number, number];
  }) => {
    const serving = await amount(...opts.serving);
    const batch = opts.batch ? await amount(...opts.batch) : null;
    return (
      await db
        .insert(ingredients)
        .values({
          userId: john.id,
          name: opts.name,
          description: opts.description,
          isVegan: true,
          servingSizeId: serving.id,
          batchSizeId: batch?.id,
        })
        .returning()
    )[0];
  };

  const flour = await ingredient({
    name: "Caputo 00 Flour",
    description:
      "Professional flour; this 100% wheat flour is a culinary essential, perfect for long fermentation baking.",
    serving: ["cup", 0.25, 30],
    batch: ["servings", 166, 500],
  });
  const water = await ingredient({
    name: "Water",
    serving: ["cup", 1, 237],
  });
  const yeast = await ingredient({
    name: "Active Dry Yeast",
    serving: ["tsp", 1, 3],
  });
  const salt = await ingredient({
    name: "Fine Sea Salt",
    serving: ["tsp", 1, 6],
  });
  const blackBeans = await ingredient({
    name: "Black Beans",
    description: "Cooked black beans.",
    serving: ["cup", 0.5, 130],
  });

  const [iron, b12, protein] = await db
    .insert(nutrients)
    .values([
      { name: "Iron", description: "Fe" },
      { name: "Vitamin B12", description: "Cobalamin" },
      { name: "Protein" },
    ])
    .returning();

  await db.insert(ingredientNutrient).values([
    { ingredientId: blackBeans.id, nutrientId: iron.id, amountPer100g: 2.1, unit: "mg" },
    { ingredientId: blackBeans.id, nutrientId: protein.id, amountPer100g: 8.9, unit: "g" },
    { ingredientId: blackBeans.id, nutrientId: b12.id, amountPer100g: 0, unit: "µg" },
  ]);

  // Biga — recipe that is itself an ingredient
  const bigaIngredient = await ingredient({
    name: "Biga",
    description:
      "Biga is a type of pre-fermentation used in Italian baking. Many popular Italian breads, including ciabatta, are made using a biga. It adds complexity to the bread's flavor and an open texture.",
    serving: ["biga", 1, 415],
    batch: ["biga", 1, 415],
  });
  const [biga] = await db
    .insert(recipes)
    .values({
      asIngredientId: bigaIngredient.id,
      subtitle: "Italian pre-ferment",
      prepMinutes: 10,
      totalTime: 970,
    })
    .returning();
  const bigaItems: [number, string, number, number][] = [
    [flour.id, "g", 250, 250],
    [water.id, "g", 162.5, 162.5],
    [yeast.id, "g", 2.5, 2.5],
  ];
  let order = 0;
  for (const [ingredientId, unit, qty, grams] of bigaItems) {
    const a = await amount(unit, qty, grams);
    await db.insert(ingredientInRecipe).values({
      order: order++,
      recipeId: biga.id,
      ingredientId,
      amountId: a.id,
    });
  }

  // Pizza dough uses the Biga *as an ingredient* (nested recipe)
  const doughIngredient = await ingredient({
    name: "Neapolitan Pizza Dough",
    description: "Long-fermentation pizza dough built on a biga.",
    serving: ["dough ball", 1, 260],
    batch: ["dough balls", 3, 780],
  });
  const [dough] = await db
    .insert(recipes)
    .values({
      asIngredientId: doughIngredient.id,
      subtitle: "Built on the biga",
      prepMinutes: 30,
      totalTime: 1440,
    })
    .returning();
  const doughItems: [number, string, number, number][] = [
    [bigaIngredient.id, "biga", 1, 415],
    [flour.id, "g", 250, 250],
    [water.id, "g", 100, 100],
    [salt.id, "g", 15, 15],
  ];
  order = 0;
  for (const [ingredientId, unit, qty, grams] of doughItems) {
    const a = await amount(unit, qty, grams);
    await db.insert(ingredientInRecipe).values({
      order: order++,
      recipeId: dough.id,
      ingredientId,
      amountId: a.id,
    });
  }

  const counts = {
    users: 1,
    ingredients: (await db.select().from(ingredients)).length,
    recipes: (await db.select().from(recipes)).length,
  };
  console.log("seeded:", counts);
  client.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
