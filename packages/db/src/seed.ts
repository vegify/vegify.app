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
import { hashPassword } from "./auth";

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

  // Dev login: dev@example.com / "dev-password" (local seed only — not a real credential).
  const [john] = await db
    .insert(users)
    .values({
      name: "Dev User",
      email: "dev@example.com",
      passwordHash: await hashPassword("dev-password"),
    })
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
    price?: number; // cents
    caloriesPer100g?: number;
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
          price: opts.price,
          caloriesPer100g: opts.caloriesPer100g,
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
    price: 599,
    caloriesPer100g: 364,
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
    price: 199,
    caloriesPer100g: 132,
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
    // Flour carries protein + iron so the Biga/Dough recipes show aggregated micros.
    { ingredientId: flour.id, nutrientId: protein.id, amountPer100g: 10, unit: "g" },
    { ingredientId: flour.id, nutrientId: iron.id, amountPer100g: 1.2, unit: "mg" },
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
      directions:
        "Stir the yeast into the water, then mix in the flour until shaggy. Cover and ferment at room temperature for 12–16 hours, until bubbly and risen.",
      prepMinutes: 10,
      totalTime: 970,
    })
    .returning();
  const bigaItems: [string, string, number, number][] = [
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
      directions:
        "Dissolve the biga and salt into the water, then work in the flour and knead to a smooth dough. Cold-ferment 24 hours, divide into balls, and proof before stretching.",
      prepMinutes: 30,
      totalTime: 1440,
    })
    .returning();
  const doughItems: [string, string, number, number][] = [
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

  // A complex DIY meal-replacement recipe (Soylent-style) to stress nutrition aggregation:
  // 20 ingredients × a full micronutrient panel. This is where the recursive-CTE (1 query)
  // vs ORM N+1 (~40 queries) difference and raw framework speed actually show.
  const extraNutrients = await db
    .insert(nutrients)
    .values(
      [
        "Total Fat",
        "Total Carbohydrates",
        "Calcium",
        "Magnesium",
        "Potassium",
        "Sodium",
        "Zinc",
        "Vitamin C",
        "Vitamin D",
        "Vitamin A",
      ].map((name) => ({ name })),
    )
    .returning();
  const panel = [protein, iron, b12, ...extraNutrients];
  const unitFor = (name: string) =>
    ["Vitamin B12", "Vitamin D", "Vitamin A"].includes(name)
      ? "µg"
      : ["Total Fat", "Total Carbohydrates", "Protein"].includes(name)
        ? "g"
        : "mg";

  const shakeItems: [string, string, number, number][] = [];
  for (let k = 0; k < 20; k++) {
    const ing = await ingredient({
      name: `Shake Ingredient ${k + 1}`,
      serving: ["g", 10, 10],
      caloriesPer100g: 180 + k * 12,
    });
    const rows = panel
      .map((nut, j) => ({
        ingredientId: ing.id,
        nutrientId: nut.id,
        amountPer100g: ((k * 7 + j * 5) % 60) + 1, // deterministic, varied
        unit: unitFor(nut.name),
      }))
      .filter((_, j) => (k + j) % 4 !== 0); // each ingredient carries most (not all) nutrients
    await db.insert(ingredientNutrient).values(rows);
    shakeItems.push([ing.id, "g", 20 + k, 20 + k]);
  }
  const shakeIngredient = await ingredient({
    name: "Complete Shake (20-ingredient)",
    description:
      "DIY meal-replacement shake — 20 ingredients with a full micronutrient panel. Exercises nutrition aggregation at scale.",
    serving: ["scoop", 1, 100],
    batch: ["batch", 1, shakeItems.reduce((s, [, , , g]) => s + g, 0)],
  });
  const [shake] = await db
    .insert(recipes)
    .values({
      asIngredientId: shakeIngredient.id,
      subtitle: "20 ingredients · full micronutrient panel",
      directions: "Combine all ingredients, blend with water, and shake well.",
    })
    .returning();
  order = 0;
  for (const [ingredientId, unit, qty, grams] of shakeItems) {
    const a = await amount(unit, qty, grams);
    await db.insert(ingredientInRecipe).values({
      order: order++,
      recipeId: shake.id,
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
