"use client"

import { useState } from "react"
import { ImageIcon, PlusIcon, SaveIcon, Trash2Icon, XIcon } from "lucide-react"

import { Input } from "./input"
import { NutritionFacts, type NutritionFactsData } from "./nutrition-facts"
import { type Visibility, VisibilityField } from "./visibility-field"

/** What `onSave` receives — the storage shape (per-100g), already converted. */
export type IngredientFormInput = {
  id?: string
  visibility: Visibility
  name: string
  description: string | null
  price: number | null // cents
  caloriesPer100g: number | null
  servingGrams: number | null
  packageGrams: number | null
  nutrients: { name: string; amountPer100g: number; unit: string }[]
}

/** Initial values for edit mode (per-serving, as the user entered them). */
export type IngredientFormDefaults = {
  id?: string
  visibility?: Visibility
  name?: string
  description?: string | null
  priceCents?: number | null
  servingGrams?: number | null
  packageGrams?: number | null
  caloriesPerServing?: number | null
  nutrients?: { name: string; amountPerServing: number; unit: string }[]
}

type Row = { key: string; name: string; amount: string; unit: string }
// Stable per-row identity so removing a middle row cannot bleed input state
// into its neighbour (index keys break exactly there).
const rowKey = () => crypto.randomUUID()
const emptyRow = (): Row => ({ key: rowKey(), name: "", amount: "", unit: "" })
const numOrNull = (s: string) => (s.trim() === "" ? null : Number(s))
// kill float noise from per-100g <-> per-serving conversions before showing in inputs
const clean = (n: number) => String(Math.round(n * 1e6) / 1e6)

export function IngredientForm({
  defaults,
  onSave,
  onDelete
}: {
  defaults?: IngredientFormDefaults
  onSave: (input: IngredientFormInput) => Promise<void>
  onDelete?: () => Promise<void>
}) {
  const [name, setName] = useState(defaults?.name ?? "")
  const [description, setDescription] = useState(defaults?.description ?? "")
  const [visibility, setVisibility] = useState<Visibility>(
    defaults?.visibility ?? "public"
  )
  const [price, setPrice] = useState(
    defaults?.priceCents != null ? clean(defaults.priceCents / 100) : ""
  )
  const [packageWeight, setPackageWeight] = useState(
    defaults?.packageGrams != null ? String(defaults.packageGrams) : ""
  )
  const [servingWeight, setServingWeight] = useState(
    defaults?.servingGrams != null ? String(defaults.servingGrams) : ""
  )
  const [calories, setCalories] = useState(
    defaults?.caloriesPerServing != null
      ? clean(defaults.caloriesPerServing)
      : ""
  )
  const [rows, setRows] = useState<Row[]>(
    defaults?.nutrients?.length
      ? defaults.nutrients.map((n) => ({
          key: rowKey(),
          name: n.name,
          amount: clean(n.amountPerServing),
          unit: n.unit
        }))
      : [emptyRow()]
  )
  const [saving, setSaving] = useState(false)

  const servingGrams = numOrNull(servingWeight)
  const per100 = (perServing: number) =>
    servingGrams ? (perServing * 100) / servingGrams : perServing

  const nutrition: NutritionFactsData = {
    heading: "This Ingredient",
    serving: { amount: 1, unit: "serving", grams: servingGrams ?? 0 },
    caloriesPerServing: numOrNull(calories),
    readings: rows
      .filter((r) => r.name.trim())
      .map((r) => ({
        name: r.name,
        amountPer100g: per100(Number(r.amount) || 0),
        unit: r.unit || "g"
      }))
  }

  function buildInput(): IngredientFormInput {
    return {
      id: defaults?.id,
      visibility,
      name: name.trim(),
      description: description.trim() || null,
      price: price.trim() ? Math.round(Number(price) * 100) : null,
      caloriesPer100g:
        servingGrams && calories.trim()
          ? (Number(calories) * 100) / servingGrams
          : null,
      servingGrams,
      packageGrams: numOrNull(packageWeight),
      nutrients: rows
        .filter((r) => r.name.trim())
        .map((r) => ({
          name: r.name.trim(),
          amountPer100g: per100(Number(r.amount) || 0),
          unit: r.unit.trim() || "g"
        }))
    }
  }

  async function handleSave() {
    if (!name.trim() || saving) return
    setSaving(true)
    try {
      await onSave(buildInput())
    } finally {
      setSaving(false)
    }
  }

  async function handleDelete() {
    if (!onDelete || saving) return
    setSaving(true)
    try {
      await onDelete()
    } finally {
      setSaving(false)
    }
  }

  const setRow = (i: number, patch: Partial<Row>) =>
    setRows((rs) => rs.map((r, j) => (j === i ? { ...r, ...patch } : r)))

  return (
    <div className="flex">
      <div className="min-w-0 flex-1">
        <div className="mx-auto max-w-2xl p-6 pb-28 lg:p-8">
          <div className="relative">
            <div className="flex aspect-video w-full items-center justify-center rounded-xl bg-muted">
              <div className="flex flex-col items-center gap-2 text-muted-foreground">
                <ImageIcon className="size-10" />
                <span className="text-sm">Ingredient Image</span>
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
            <h1 className="text-center font-bold text-3xl text-primary-dark">
              Create / Edit Ingredient
            </h1>
            {onDelete && (
              <button
                type="button"
                aria-label="Delete ingredient"
                onClick={handleDelete}
                className="flex size-8 items-center justify-center rounded-full bg-destructive text-white"
              >
                <Trash2Icon className="size-4" />
              </button>
            )}
          </div>

          <div className="mt-6 space-y-3">
            <Input
              aria-label="Food Name"
              placeholder="Food Name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="h-11"
            />
            <textarea
              aria-label="Description"
              placeholder="Description"
              value={description ?? ""}
              onChange={(e) => setDescription(e.target.value)}
              className="min-h-20 w-full rounded-lg border border-input bg-transparent px-2.5 py-2 text-base outline-none placeholder:text-muted-foreground focus-visible:border-ring"
            />
            <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
              <Input
                aria-label="Price (dollars)"
                placeholder="Price"
                type="number"
                step="0.01"
                value={price}
                onChange={(e) => setPrice(e.target.value)}
                className="h-11"
              />
              <Input
                aria-label="Package Weight (grams)"
                placeholder="Package Weight"
                type="number"
                value={packageWeight}
                onChange={(e) => setPackageWeight(e.target.value)}
                className="h-11"
              />
              <Input
                aria-label="Weight per Serving (grams)"
                placeholder="Weight per Serving"
                type="number"
                value={servingWeight}
                onChange={(e) => setServingWeight(e.target.value)}
                className="h-11"
              />
            </div>
            <Input
              aria-label="Calories Per Serving"
              placeholder="Calories Per Serving"
              type="number"
              value={calories}
              onChange={(e) => setCalories(e.target.value)}
              className="h-11"
            />
          </div>

          <div className="mt-4">
            <VisibilityField value={visibility} onChange={setVisibility} />
          </div>

          <h2 className="mt-8 mb-3 text-center font-bold text-lg">Nutrients</h2>
          <p className="mb-3 text-center text-muted-foreground text-xs">
            Amounts are per serving{servingGrams ? ` (${servingGrams} g)` : ""}.
          </p>
          <div className="space-y-2">
            {rows.map((r, i) => (
              <div key={r.key} className="flex items-center gap-2">
                <Input
                  aria-label="Nutrient"
                  placeholder="Nutrient"
                  value={r.name}
                  onChange={(e) => setRow(i, { name: e.target.value })}
                  className="h-11 flex-1"
                />
                <Input
                  aria-label="Amount"
                  placeholder="0"
                  type="number"
                  value={r.amount}
                  onChange={(e) => setRow(i, { amount: e.target.value })}
                  className="h-11 w-20"
                />
                <Input
                  aria-label="Unit"
                  placeholder="unit or %"
                  value={r.unit}
                  onChange={(e) => setRow(i, { unit: e.target.value })}
                  className="h-11 w-24"
                />
                <button
                  type="button"
                  aria-label="Remove nutrient"
                  onClick={() => setRows((rs) => rs.filter((_, j) => j !== i))}
                  className="flex size-8 shrink-0 items-center justify-center rounded-full text-muted-foreground hover:bg-muted"
                >
                  <XIcon className="size-4" />
                </button>
              </div>
            ))}
            <button
              type="button"
              onClick={() => setRows((rs) => [...rs, emptyRow()])}
              className="flex items-center gap-1 font-medium text-primary text-sm hover:underline"
            >
              <PlusIcon className="size-4" /> Add nutrient
            </button>
          </div>
        </div>
      </div>

      <aside className="hidden w-80 shrink-0 border-border border-l p-6 lg:block">
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
  )
}
