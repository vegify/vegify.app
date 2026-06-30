"use client";

import { useEffect, useState } from "react";
import {
  CarrotIcon,
  ImageIcon,
  PlusIcon,
  SaveIcon,
  SearchIcon,
  Trash2Icon,
} from "lucide-react";
import { Input } from "./input";
import { NutritionFacts, type NutritionFactsData, type NutritionReading } from "./nutrition-facts";
import { VisibilityField, type Visibility } from "./visibility-field";

export type IngredientSearchItem = {
  id: string;
  name: string;
  servingGrams: number | null;
  caloriesPer100g: number | null;
  readings: NutritionReading[];
};

/** Storage shape passed to onSave. */
export type RecipeFormInput = {
  id?: string;
  visibility: Visibility;
  name: string;
  subtitle: string | null;
  directions: string | null;
  servingGrams: number | null;
  batchGrams: number | null;
  items: { ingredientId: string; grams: number }[];
};

export type RecipeFormDefaults = {
  id?: string;
  visibility?: Visibility;
  name?: string;
  subtitle?: string | null;
  directions?: string | null;
  servings?: number | null;
  items?: Array<{
    ingredientId: string;
    name: string;
    grams: number;
    caloriesPer100g: number | null;
    readings: NutritionReading[];
  }>;
};

type Row = {
  ingredientId: string;
  name: string;
  grams: string;
  caloriesPer100g: number | null;
  readings: NutritionReading[];
};

const clean = (n: number) => String(Math.round(n * 1e6) / 1e6);

export function RecipeForm({
  defaults,
  onSearch,
  onSave,
  onDelete,
  createIngredientHref = "/ingredients/new",
}: {
  defaults?: RecipeFormDefaults;
  onSearch: (query: string) => Promise<IngredientSearchItem[]>;
  onSave: (input: RecipeFormInput) => Promise<void>;
  onDelete?: () => Promise<void>;
  createIngredientHref?: string;
}) {
  const [name, setName] = useState(defaults?.name ?? "");
  const [subtitle, setSubtitle] = useState(defaults?.subtitle ?? "");
  const [directions, setDirections] = useState(defaults?.directions ?? "");
  const [visibility, setVisibility] = useState<Visibility>(defaults?.visibility ?? "public");
  const [servings, setServings] = useState(
    defaults?.servings != null ? clean(defaults.servings) : "1",
  );
  const [rows, setRows] = useState<Row[]>(
    defaults?.items?.map((i) => ({
      ingredientId: i.ingredientId,
      name: i.name,
      grams: clean(i.grams),
      caloriesPer100g: i.caloriesPer100g,
      readings: i.readings,
    })) ?? [],
  );
  const [saving, setSaving] = useState(false);

  // ingredient search
  const [picking, setPicking] = useState(false);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<IngredientSearchItem[]>([]);
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    if (!picking) return;
    let alive = true;
    setSearching(true);
    const t = setTimeout(async () => {
      try {
        const r = await onSearch(query);
        if (alive) setResults(r);
      } finally {
        if (alive) setSearching(false);
      }
    }, 250);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [query, picking, onSearch]);

  const totalGrams = rows.reduce((s, r) => s + (Number(r.grams) || 0), 0);
  const servingsN = Math.max(1, Number(servings) || 1);
  const servingGrams = totalGrams > 0 ? totalGrams / servingsN : 0;

  // live aggregate (grams-weighted) → recipe per-100g, same shape as getRecipeNutrition
  const calTotal = rows.reduce(
    (s, r) => s + (r.caloriesPer100g ?? 0) * (Number(r.grams) || 0) / 100,
    0,
  );
  const calKnown = rows.some((r) => r.caloriesPer100g != null);
  const nutMap = new Map<string, { amt: number; unit: string }>();
  for (const r of rows) {
    const grams = Number(r.grams) || 0;
    for (const reading of r.readings) {
      const prev = nutMap.get(reading.name) ?? { amt: 0, unit: reading.unit };
      nutMap.set(reading.name, {
        amt: prev.amt + (reading.amountPer100g * grams) / 100,
        unit: prev.unit,
      });
    }
  }
  const nutrition: NutritionFactsData = {
    heading: "This Recipe",
    serving: { amount: 1, unit: "serving", grams: servingGrams },
    servingsPerBatch: servingsN,
    caloriesPerServing:
      calKnown && totalGrams ? (calTotal / totalGrams) * servingGrams : null,
    readings: [...nutMap].map(([n, { amt, unit }]) => ({
      name: n,
      amountPer100g: totalGrams ? (amt / totalGrams) * 100 : 0,
      unit,
    })),
  };

  function addIngredient(item: IngredientSearchItem) {
    setRows((rs) => [
      ...rs,
      {
        ingredientId: item.id,
        name: item.name,
        grams: clean(item.servingGrams ?? 100),
        caloriesPer100g: item.caloriesPer100g,
        readings: item.readings,
      },
    ]);
    setQuery("");
    setResults([]);
    setPicking(false);
  }

  function buildInput(): RecipeFormInput {
    return {
      id: defaults?.id,
      visibility,
      name: name.trim(),
      subtitle: subtitle.trim() || null,
      directions: directions.trim() || null,
      servingGrams: totalGrams > 0 ? totalGrams / servingsN : null,
      batchGrams: totalGrams > 0 ? totalGrams : null,
      items: rows
        .filter((r) => r.ingredientId && Number(r.grams) > 0)
        .map((r) => ({ ingredientId: r.ingredientId, grams: Number(r.grams) })),
    };
  }

  async function handleSave() {
    if (!name.trim() || saving) return;
    setSaving(true);
    try {
      await onSave(buildInput());
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!onDelete || saving) return;
    setSaving(true);
    try {
      await onDelete();
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex">
      <div className="min-w-0 flex-1">
        <div className="mx-auto max-w-2xl p-6 pb-28 lg:p-8">
          <div className="relative">
            <div className="flex aspect-video w-full items-center justify-center rounded-xl bg-muted">
              <div className="flex flex-col items-center gap-2 text-muted-foreground">
                <ImageIcon className="size-10" />
                <span className="text-sm">Recipe Image</span>
              </div>
            </div>
            <button
              type="button"
              aria-label="Add image"
              className="absolute right-4 -bottom-5 flex size-11 items-center justify-center rounded-full bg-card text-muted-foreground ring-1 ring-foreground/10"
            >
              <PlusIcon className="size-5" />
            </button>
          </div>

          <div className="mt-10 flex items-center justify-center gap-3">
            <h1 className="text-center text-3xl font-bold text-primary-dark">
              Create / Edit Recipe
            </h1>
            {onDelete && (
              <button
                type="button"
                aria-label="Delete recipe"
                onClick={handleDelete}
                className="flex size-8 items-center justify-center rounded-full bg-destructive text-white"
              >
                <Trash2Icon className="size-4" />
              </button>
            )}
          </div>

          <div className="mt-6 space-y-3">
            <Input
              aria-label="Recipe Name"
              placeholder="Recipe Name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="h-11"
            />
            <Input
              aria-label="Subtitle"
              placeholder="Subtitle"
              value={subtitle ?? ""}
              onChange={(e) => setSubtitle(e.target.value)}
              className="h-11"
            />
            <label className="flex items-center gap-3 text-sm text-muted-foreground">
              <span className="shrink-0">Servings per batch</span>
              <Input
                aria-label="Servings per batch"
                type="number"
                value={servings}
                onChange={(e) => setServings(e.target.value)}
                className="h-11 w-24"
              />
            </label>
          </div>

          <div className="mt-4">
            <VisibilityField value={visibility} onChange={setVisibility} />
          </div>

          <h2 className="mt-8 mb-3 text-center text-xl font-bold">Ingredients</h2>
          <div className="space-y-2">
            {rows.map((r, i) => (
              <div key={i} className="flex items-center gap-2">
                <Input
                  aria-label={`Amount for ${r.name}`}
                  type="number"
                  value={r.grams}
                  onChange={(e) =>
                    setRows((rs) => rs.map((x, j) => (j === i ? { ...x, grams: e.target.value } : x)))
                  }
                  className="h-11 w-20"
                />
                <span className="text-sm text-muted-foreground">g</span>
                <span className="flex-1 truncate font-medium">{r.name}</span>
                <button
                  type="button"
                  aria-label={`Remove ${r.name}`}
                  onClick={() => setRows((rs) => rs.filter((_, j) => j !== i))}
                  className="flex size-8 shrink-0 items-center justify-center rounded-full text-destructive hover:bg-muted"
                >
                  <Trash2Icon className="size-4" />
                </button>
              </div>
            ))}
            {rows.length === 0 && (
              <p className="text-center text-sm text-muted-foreground">No ingredients yet.</p>
            )}
          </div>

          {picking ? (
            <div className="mt-3 rounded-xl border border-border p-3">
              <div className="relative">
                <SearchIcon className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  aria-label="Search ingredients"
                  autoFocus
                  placeholder="Search ingredients…"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  className="h-11 pl-9"
                />
              </div>
              <ul className="mt-2 max-h-56 overflow-y-auto">
                {searching && <li className="px-2 py-1.5 text-sm text-muted-foreground">Searching…</li>}
                {!searching &&
                  results.map((item) => (
                    <li key={item.id}>
                      <button
                        type="button"
                        onClick={() => addIngredient(item)}
                        className="flex w-full items-center justify-between gap-2 rounded-lg px-2 py-1.5 text-left text-sm hover:bg-accent"
                      >
                        <span className="font-medium">{item.name}</span>
                        <span className="text-muted-foreground">
                          {item.caloriesPer100g != null ? `${Math.round(item.caloriesPer100g)} cal/100g` : ""}
                        </span>
                      </button>
                    </li>
                  ))}
                {!searching && results.length === 0 && (
                  <li className="px-2 py-1.5 text-sm text-muted-foreground">
                    No matches.{" "}
                    <a href={createIngredientHref} className="text-primary hover:underline">
                      Create a new ingredient
                    </a>
                  </li>
                )}
              </ul>
            </div>
          ) : (
            <button
              type="button"
              onClick={() => setPicking(true)}
              className="mt-4 flex items-center gap-2 rounded-lg bg-green-dark px-4 py-2.5 text-sm font-semibold text-white transition hover:brightness-110"
            >
              <CarrotIcon className="size-4" /> Add Ingredient
            </button>
          )}

          <h2 className="mt-8 mb-3 text-center text-xl font-bold">Directions</h2>
          <textarea
            aria-label="Directions"
            placeholder="Directions…"
            value={directions ?? ""}
            onChange={(e) => setDirections(e.target.value)}
            className="min-h-32 w-full rounded-lg border border-input bg-transparent px-2.5 py-2 text-base outline-none placeholder:text-muted-foreground focus-visible:border-ring"
          />
        </div>
      </div>

      <aside className="hidden w-80 shrink-0 border-l border-border p-6 lg:block">
        <div className="lg:sticky lg:top-6">
          <NutritionFacts data={nutrition} />
        </div>
      </aside>

      <button
        type="button"
        aria-label="Save"
        disabled={saving || !name.trim()}
        onClick={handleSave}
        className="fixed right-4 bottom-20 z-30 flex size-14 items-center justify-center rounded-full bg-orange text-white shadow-lg transition hover:brightness-95 disabled:opacity-50 lg:bottom-8"
      >
        <SaveIcon className="size-6" />
      </button>
    </div>
  );
}
