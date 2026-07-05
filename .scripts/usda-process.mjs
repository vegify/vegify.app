#!/usr/bin/env node
// USDA FoodData Central → vegify's communal-catalog seed (PLANTS ONLY — the product decision).
//
// Reads the raw FDC downloads (gitignored, .data/import/usda/ — Foundation 2025-04 + SR Legacy
// 2018-04, both public domain) and emits services/api/data/usda-plants.json.gz, the compact
// artifact the server's boot-time ingest embeds. Run it locally after downloading fresh FDC
// releases; the artifact is committed (public-domain data, ~hundreds of KB gzipped).
//
// Selection: the six unambiguous plant categories (oils/beverages/sweets are mixed bags — curate
// later if wanted). Nutrients: the 42-field dictionary from the completefoods capture
// (nutrient-meta.json's usdaId tagnames) mapped to classic NDB nutrient numbers — the SAME bridge
// the CompleteFoods import uses, so the two datasets speak one nutrient vocabulary. Units are
// FDC-native (Vitamin E in mg, not CF's IU — importers convert, the catalog stays honest).
// Amounts in both datasets are per 100 g, which is exactly vegify's ingredient_nutrient model.
// Dedupe: Foundation (newer, analytically measured) wins over SR Legacy on a normalized-name tie.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { gzipSync } from "node:zlib";

const SRC = ".data/import/usda";
const OUT = "services/api/data/usda-plants.json.gz";

const PLANT_CATEGORIES = new Set([
  "Vegetables and Vegetable Products",
  "Fruits and Fruit Juices",
  "Legumes and Legume Products",
  "Nut and Seed Products",
  "Cereal Grains and Pasta",
  "Spices and Herbs",
]);

// NDB nutrient number → catalog (name, unit). Names are the completefoods dictionary's display
// titles (the CF import maps 1:1); numbers are the classic NDB ids FDC still carries on every
// nutrient row. Folate prefers DFE (435) with food folate (417) as fallback — see below.
const NUTRIENTS = new Map([
  [208, ["Calories", "kcal"]],
  [205, ["Carbohydrates", "g"]],
  [203, ["Protein", "g"]],
  [204, ["Total Fat", "g"]],
  [606, ["Saturated Fat", "g"]],
  [645, ["Monounsaturated Fat", "g"]],
  [646, ["Polyunsaturated Fat", "g"]],
  [851, ["Omega-3 Fatty Acids", "g"]], // 18:3 n-3 (ALA) — the plant omega-3
  [675, ["Omega-6 Fatty Acids", "g"]], // 18:2 n-6
  [291, ["Total Fiber", "g"]],
  [601, ["Cholesterol", "mg"]],
  [301, ["Calcium", "mg"]],
  [312, ["Copper", "mg"]],
  [303, ["Iron", "mg"]],
  [304, ["Magnesium", "mg"]],
  [315, ["Manganese", "mg"]],
  [305, ["Phosphorus", "mg"]],
  [306, ["Potassium", "mg"]],
  [317, ["Selenium", "ug"]],
  [307, ["Sodium", "mg"]],
  [309, ["Zinc", "mg"]],
  [318, ["Vitamin A", "IU"]],
  [415, ["Vitamin B6", "mg"]],
  [418, ["Vitamin B12", "ug"]],
  [401, ["Vitamin C", "mg"]],
  [324, ["Vitamin D", "IU"]],
  [323, ["Vitamin E", "mg"]], // FDC-native mg alpha-tocopherol (CF displays IU; its import converts)
  [430, ["Vitamin K", "ug"]],
  [404, ["Thiamin", "mg"]],
  [405, ["Riboflavin", "mg"]],
  [406, ["Niacin", "mg"]],
  [435, ["Folate", "ug"]], // DFE, preferred
  [417, ["Folate", "ug"]], // food folate, fallback when 435 absent
  [410, ["Pantothenic Acid", "mg"]],
  [421, ["Choline", "mg"]],
]);
const FOLATE_DFE = 435;
const FOLATE_FOOD = 417;

function extract(food) {
  const category = (food.foodCategory ?? {}).description ?? "";
  if (!PLANT_CATEGORIES.has(category)) return null;
  const byNumber = new Map();
  for (const fn of food.foodNutrients ?? []) {
    const number = Number(fn.nutrient?.number);
    if (!NUTRIENTS.has(number)) continue;
    const amount = fn.amount;
    if (amount == null || Number.isNaN(amount)) continue;
    byNumber.set(number, amount);
  }
  // Folate: one row, DFE preferred.
  if (byNumber.has(FOLATE_DFE)) byNumber.delete(FOLATE_FOOD);
  const nutrients = [...byNumber.entries()].map(([number, amount]) => {
    const [name, unit] = NUTRIENTS.get(number);
    return { name, unit, amountPer100g: amount };
  });
  if (nutrients.length === 0) return null;
  return { fdcId: food.fdcId, name: food.description.trim(), category, nutrients };
}

const foundation = JSON.parse(
  readFileSync(`${SRC}/FoodData_Central_foundation_food_json_2025-04-24.json`, "utf8"),
).FoundationFoods;
const legacy = JSON.parse(
  readFileSync(`${SRC}/FoodData_Central_sr_legacy_food_json_2018-04.json`, "utf8"),
).SRLegacyFoods;

const out = new Map(); // normalized name → entry; Foundation first so it wins ties
let fromFoundation = 0;
for (const f of foundation) {
  const e = extract(f);
  if (e) {
    out.set(e.name.toLowerCase(), { ...e, dataset: "foundation" });
    fromFoundation++;
  }
}
let fromLegacy = 0;
let shadowed = 0;
for (const f of legacy) {
  const e = extract(f);
  if (!e) continue;
  const key = e.name.toLowerCase();
  if (out.has(key)) {
    shadowed++;
    continue;
  }
  out.set(key, { ...e, dataset: "sr-legacy" });
  fromLegacy++;
}

const entries = [...out.values()].sort((a, b) => a.name.localeCompare(b.name));
const json = JSON.stringify(entries);
mkdirSync("services/api/data", { recursive: true });
writeFileSync(OUT, gzipSync(Buffer.from(json), { level: 9 }));

const nutrientCounts = entries.map((e) => e.nutrients.length);
const avg = (nutrientCounts.reduce((a, b) => a + b, 0) / entries.length).toFixed(1);
const perCat = {};
for (const e of entries) perCat[e.category] = (perCat[e.category] ?? 0) + 1;
console.log(`wrote ${OUT}`);
console.log(`foods: ${entries.length} (foundation ${fromFoundation}, sr-legacy ${fromLegacy}, shadowed ${shadowed})`);
console.log(`nutrients/food avg ${avg}, min ${Math.min(...nutrientCounts)}, max ${Math.max(...nutrientCounts)}`);
console.log(`raw ${(json.length / 1e6).toFixed(1)} MB → gz ${(gzipSync(Buffer.from(json), { level: 9 }).length / 1e3).toFixed(0)} KB`);
for (const [c, n] of Object.entries(perCat)) console.log(`  ${c}: ${n}`);
