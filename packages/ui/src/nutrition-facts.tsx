import { FileTextIcon } from "lucide-react";
import { cn } from "./cn";

/**
 * The FDA-style Nutrition Facts panel — the micronutrition core, transcribed from the
 * brand comp (Desktop "Nutrition Facts" rail / mobile modal). Readings are per-100g
 * (`ingredient_nutrient`); the panel scales them to the serving and computes %DV.
 */

export type NutritionReading = {
  name: string;
  amountPer100g: number;
  unit: string;
};

export type NutritionFactsData = {
  heading?: string; // "This Ingredient" | "This Recipe"
  servingsPerBatch?: number | null;
  serving?: {
    amount?: number | null;
    unit?: string | null;
    grams: number;
  } | null;
  caloriesPerServing?: number | null;
  readings: NutritionReading[];
};

// --- units → micrograms (common base for %DV ratios) ---
const TO_UG: Record<string, number> = { g: 1e6, mg: 1e3, µg: 1, mcg: 1, ug: 1 };
const toUg = (amt: number, unit: string) =>
  amt * (TO_UG[unit.toLowerCase()] ?? NaN);

// FDA Daily Values (adults / children ≥4y). null = no established DV.
const DV: Record<string, { dv: number; unit: string } | null> = {
  "total fat": { dv: 78, unit: "g" },
  saturated: { dv: 20, unit: "g" },
  monounsaturated: null,
  polyunsaturated: null,
  "omega-3s": null,
  "omega-6s": null,
  "total carbohydrates": { dv: 275, unit: "g" },
  "total protein": { dv: 50, unit: "g" },
  calcium: { dv: 1300, unit: "mg" },
  chloride: { dv: 2300, unit: "mg" },
  chromium: { dv: 35, unit: "µg" },
  copper: { dv: 0.9, unit: "mg" },
  iodine: { dv: 150, unit: "µg" },
  iron: { dv: 18, unit: "mg" },
  magnesium: { dv: 420, unit: "mg" },
  manganese: { dv: 2.3, unit: "mg" },
  phosphorus: { dv: 1250, unit: "mg" },
  potassium: { dv: 4700, unit: "mg" },
  selenium: { dv: 55, unit: "µg" },
  sodium: { dv: 2300, unit: "mg" },
  sulfur: null,
  "vitamin a": { dv: 900, unit: "µg" },
  "vitamin b6": { dv: 1.7, unit: "mg" },
  "vitamin b12": { dv: 2.4, unit: "µg" },
  "vitamin c": { dv: 90, unit: "mg" },
  "vitamin d": { dv: 20, unit: "µg" },
  "vitamin e": { dv: 15, unit: "mg" },
  "vitamin k": { dv: 120, unit: "µg" },
  thiamin: { dv: 1.2, unit: "mg" },
  riboflavin: { dv: 1.3, unit: "mg" },
  niacin: { dv: 16, unit: "mg" },
  folate: { dv: 400, unit: "µg" },
  "pantothenic acid": { dv: 5, unit: "mg" },
  biotin: { dv: 30, unit: "µg" },
  choline: { dv: 550, unit: "mg" },
};

// label name -> reading names that map to it
const ALIASES: Record<string, string[]> = {
  "total fat": ["fat"],
  saturated: ["saturated fat"],
  monounsaturated: ["monounsaturated fat"],
  polyunsaturated: ["polyunsaturated fat"],
  "omega-3s": ["omega-3", "omega 3", "omega-3 fatty acids"],
  "omega-6s": ["omega-6", "omega 6", "omega-6 fatty acids"],
  "total carbohydrates": [
    "carbohydrate",
    "carbohydrates",
    "carbs",
    "carb",
    "total carbohydrate",
  ],
  "total protein": ["protein"],
  thiamin: ["thiamine", "vitamin b1"],
  riboflavin: ["vitamin b2"],
  niacin: ["vitamin b3"],
  "pantothenic acid": ["vitamin b5", "pantotheniacid"],
  folate: ["folic acid", "vitamin b9"],
};

const norm = (s: string) => s.trim().toLowerCase();

type Macro = { key: string; label: string; indent: 0 | 1 | 2 };
const MACROS: Macro[] = [
  { key: "total fat", label: "Total Fat", indent: 0 },
  { key: "saturated", label: "Saturated", indent: 1 },
  { key: "monounsaturated", label: "Monounsaturated", indent: 1 },
  { key: "polyunsaturated", label: "Polyunsaturated", indent: 1 },
  { key: "omega-3s", label: "Omega-3s", indent: 2 },
  { key: "omega-6s", label: "Omega-6s", indent: 2 },
  { key: "total carbohydrates", label: "Total Carbohydrates", indent: 0 },
  { key: "total protein", label: "Total Protein", indent: 0 },
];
const MICRO_LEFT = [
  "calcium",
  "chloride",
  "chromium",
  "copper",
  "iodine",
  "iron",
  "magnesium",
  "manganese",
  "phosphorus",
  "potassium",
  "selenium",
  "sodium",
  "sulfur",
];
const MICRO_RIGHT = [
  "vitamin a",
  "vitamin b6",
  "vitamin b12",
  "vitamin c",
  "vitamin d",
  "vitamin e",
  "vitamin k",
  "thiamin",
  "riboflavin",
  "niacin",
  "folate",
  "pantothenic acid",
  "biotin",
  "choline",
];
const MICRO_LABEL: Record<string, string> = {
  "vitamin a": "Vitamin A",
  "vitamin b6": "Vitamin B6",
  "vitamin b12": "Vitamin B12",
  "vitamin c": "Vitamin C",
  "vitamin d": "Vitamin D",
  "vitamin e": "Vitamin E",
  "vitamin k": "Vitamin K",
  "pantothenic acid": "Pantothenic Acid",
};
const micLabel = (k: string) =>
  MICRO_LABEL[k] ?? k.charAt(0).toUpperCase() + k.slice(1);

const fmt = (n: number) => {
  if (!Number.isFinite(n)) return "0";
  const r = Math.round(n * 10) / 10;
  return (Number.isInteger(r) ? r.toFixed(0) : r.toFixed(1)).replace(
    /\.0$/,
    "",
  );
};

export function NutritionFacts({
  data,
  className,
}: {
  data: NutritionFactsData;
  className?: string;
}) {
  const scale = data.serving?.grams ? data.serving.grams / 100 : 1;
  const lookup = new Map<string, NutritionReading>();
  for (const r of data.readings) {
    lookup.set(norm(r.name), r);
    for (const [canon, aliases] of Object.entries(ALIASES)) {
      if (aliases.includes(norm(r.name))) lookup.set(canon, r);
    }
  }

  const valueFor = (key: string) => {
    const r = lookup.get(key);
    const perServing = r ? r.amountPer100g * scale : 0;
    const unit = r?.unit ?? DV[key]?.unit ?? "g";
    const dv = DV[key];
    let pct: number | null = null;
    if (dv && r) {
      const base = toUg(perServing, r.unit);
      const dvBase = toUg(dv.dv, dv.unit);
      if (Number.isFinite(base) && dvBase)
        pct = Math.round((base / dvBase) * 100);
    } else if (dv && !r) {
      pct = 0;
    }
    return { amount: perServing, unit, pct };
  };

  const cal = data.caloriesPerServing ?? 0;
  const serving = data.serving;

  return (
    <div className={cn("text-sm text-foreground", className)}>
      <div className="flex items-center justify-between border-b-4 border-foreground pb-1">
        <h2 className="text-2xl font-extrabold tracking-tight">
          Nutrition Facts
        </h2>
        <FileTextIcon className="size-6" aria-hidden />
      </div>

      <p className="mt-2 text-lg font-semibold">
        {data.heading ?? "This Ingredient"}
      </p>
      {data.servingsPerBatch != null && (
        <p>{fmt(data.servingsPerBatch)} servings per batch</p>
      )}
      <p className="font-semibold">
        Serving size {serving?.amount != null ? fmt(serving.amount) : ""}{" "}
        {serving?.unit ?? (serving?.grams ? `${fmt(serving.grams)} g` : "")}
      </p>

      <div className="mt-1 border-t-8 border-foreground" />
      <p className="pt-1 text-xs font-semibold">Amount per serving</p>
      <div className="flex items-end justify-between border-b-4 border-foreground pb-1">
        <span className="text-3xl font-extrabold">Calories</span>
        <span className="text-3xl font-extrabold">{fmt(cal)}</span>
      </div>
      <p className="text-xs text-muted-foreground">{macroPct(valueFor, cal)}</p>

      <p className="mt-1 border-b border-foreground pb-0.5 text-right text-xs font-bold">
        % Daily Value*
      </p>

      <dl>
        {MACROS.map((m) => {
          const { amount, unit, pct } = valueFor(m.key);
          return (
            <LeaderRow
              key={m.key}
              indent={m.indent}
              label={m.label}
              value={`${fmt(amount)}${unit}`}
              pct={pct}
              bold={m.indent === 0}
            />
          );
        })}
      </dl>

      <div className="mt-1 grid grid-cols-2 gap-x-6 gap-y-0.5 border-t-4 border-foreground pt-1 text-xs">
        {(
          [
            ["left", MICRO_LEFT],
            ["right", MICRO_RIGHT],
          ] as const
        ).map(([side, col]) => (
          <div key={side} className="space-y-0.5">
            {col.map((k) => {
              const { amount, unit, pct } = valueFor(k);
              return (
                <div key={k} className="flex justify-between gap-1">
                  <span>
                    <span className="font-bold">{micLabel(k)}</span>{" "}
                    {fmt(amount)}
                    {unit}
                  </span>
                  <span>{pct == null ? "" : `${pct}%`}</span>
                </div>
              );
            })}
          </div>
        ))}
      </div>

      <p className="mt-2 border-t border-foreground pt-1 text-[10px] leading-tight text-muted-foreground">
        * Percent Daily Values are based on a 2,000 calorie diet.
      </p>
    </div>
  );
}

function LeaderRow({
  label,
  value,
  pct,
  indent,
  bold,
}: {
  label: string;
  value: string;
  pct: number | null;
  indent: 0 | 1 | 2;
  bold?: boolean;
}) {
  return (
    <div
      className={cn(
        "flex items-baseline border-b border-foreground/15 py-0.5",
        bold ? "font-bold" : "font-normal",
      )}
      style={{ paddingLeft: indent * 14 }}
    >
      <span>
        {label} {value}
      </span>
      <span className="mx-1 flex-1 self-center border-b border-dotted border-foreground/30" />
      <span className="font-bold">{pct == null ? "—" : `${pct}%`}</span>
    </div>
  );
}

// "00% fat 00% protein 00% carb" — share of calories from each macro
function macroPct(
  valueFor: (k: string) => { amount: number; unit: string; pct: number | null },
  cal: number,
) {
  if (!cal) return "0% fat 0% protein 0% carb";
  const g = (k: string) => valueFor(k).amount; // grams per serving
  const fat = Math.round(((g("total fat") * 9) / cal) * 100);
  const protein = Math.round(((g("total protein") * 4) / cal) * 100);
  const carb = Math.round(((g("total carbohydrates") * 4) / cal) * 100);
  return `${fat}% fat ${protein}% protein ${carb}% carb`;
}
