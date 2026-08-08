"use client"

import { useEffect, useRef, useState } from "react"
import { BarcodeIcon, CameraIcon, TriangleAlertIcon } from "lucide-react"

import { buttonClasses } from "./button"
import { cn } from "./cn"
import type { NutritionReading } from "./nutrition-facts"
import type { IngredientSearchItem } from "./recipe-form"

/**
 * BRANDED FOODS — the client half of P2.1/P2.2. Purely presentational, like every screen module here:
 * it takes a `BrandedAdapter` (a bag of async callbacks) and each shell wires that to its own
 * transport (web → server-fns → /api/branded/*; desktop/iOS → IPC, when that lands).
 *
 * Two invariants this module exists to enforce, both licensing/trust obligations rather than taste:
 *
 * 1. **Diet flags are ADVISORY and ONE-DIRECTIONAL.** `animal_derived_flags` reports what it FOUND in
 *    the label's ingredient statement; it never returns "vegan". So the UI renders flags as warnings
 *    ("Contains: milk, rennet") and renders an EMPTY flag list as an explicit non-answer — never as a
 *    green check, a "vegan" badge, or any wording a reader could take for a certification. Inverting
 *    "we matched nothing" into "this is vegan" is the one thing this component must never do.
 * 2. **Attribution is rendered, not stored and forgotten.** Every result carries the `attribution`
 *    string the server computed for its source; OFF rows are ODbL and the credit + license mention is
 *    a licence condition. The per-row line here plus the /sources page is the whole obligation.
 */

/** Which animal-product family a flagged term belongs to (mirrors vegify-core's `DietCategory`). */
export type BrandedDietCategory = "dairy" | "egg" | "meat" | "fish" | "insect"

/** One advisory animal-derived term found in a label's ingredient statement. Never a verdict. */
export type BrandedDietFlagVM = {
  term: string
  /** The span exactly as the label writes it, so we quote rather than paraphrase. */
  matched: string
  category: BrandedDietCategory
}

/** One branded-food lookup hit as the UI renders it (structurally the wire `BrandedFood`; kept local
 *  so @vegify/ui stays decoupled from @vegify/api-types, same as `DayVM`). */
export type BrandedFoodVM = {
  source: "usda-branded" | "off"
  externalId: string
  gtin: string | null
  name: string
  brand: string | null
  servingGrams: number | null
  servingUnit: string | null
  ingredientsText: string | null
  caloriesPer100g: number | null
  nutrients: { name: string; amountPer100g: number | null; unit: string }[]
  /** Set once this food has been promoted into the communal catalog. */
  ingredientId: string | null
  dietFlags: BrandedDietFlagVM[]
  attribution: string
}

/** The branded lookup port — present ⇒ the shell can offer branded search / barcode entry. */
export type BrandedAdapter = {
  /** Ranked branded-food search (cache-first; tops up from USDA FDC only when the cache under-fills). */
  search: (q: string) => Promise<BrandedFoodVM[]>
  /** Resolve a GTIN/UPC/EAN. Resolves to null for a barcode no source knows — that is manual entry,
   *  never an error. */
  barcode: (gtin: string) => Promise<BrandedFoodVM | null>
  /** Join a looked-up food into the communal catalog (authed; idempotent). Resolves to the catalog
   *  ingredient id — from that moment the food is an ordinary catalog entry on every client. */
  promote: (food: BrandedFoodVM) => Promise<string>
  /** Native camera scan port. Present ⇒ the shell has a real scanner wired (iOS, once P2.3's native
   *  half lands). Absent ⇒ the web photo-scan path is used when the platform supports it, else manual
   *  entry. Resolves to a barcode string, or null when the user cancelled. */
  scanNative?: () => Promise<string | null>
}

/** How a source is named to the reader. Deliberately the human names, not the wire discriminants. */
export const BRANDED_SOURCE_LABEL: Record<BrandedFoodVM["source"], string> = {
  "usda-branded": "USDA Branded",
  off: "Open Food Facts"
}

/** Plural-aware category wording for the "Contains" line. */
const CATEGORY_LABEL: Record<BrandedDietCategory, string> = {
  dairy: "dairy",
  egg: "egg",
  meat: "meat",
  fish: "fish",
  insect: "insect-derived"
}

/**
 * The catalog name a promoted row takes: brand-prefixed unless the product name already carries the
 * brand. MIRRORS `BrandedFood::catalog_name` in crates/vegify-core/src/branded.rs — the server is
 * authoritative (it is what actually gets stored); this is the optimistic label the composer shows on
 * the row it just added, so the two must agree. Change one, change the other.
 */
export function brandedCatalogName(food: {
  name: string
  brand: string | null
}): string {
  const brand = food.brand?.trim()
  if (!brand) return food.name
  return food.name.toLowerCase().includes(brand.toLowerCase())
    ? food.name
    : `${brand} ${food.name}`
}

/**
 * A promoted branded food, as the recipe composer's row model. `id` is the catalog ingredient id the
 * promote call just returned, so from here the row is indistinguishable from a catalog pick.
 */
export function brandedAsSearchItem(
  food: BrandedFoodVM,
  ingredientId: string
): IngredientSearchItem {
  const readings: NutritionReading[] = food.nutrients.map((n) => ({
    name: n.name,
    amountPer100g: n.amountPer100g ?? 0,
    unit: n.unit
  }))
  return {
    id: ingredientId,
    name: brandedCatalogName(food),
    servingGrams: food.servingGrams,
    servingUnit: food.servingUnit,
    caloriesPer100g: food.caloriesPer100g,
    readings
  }
}

/** Strip a scanned/typed barcode to digits. UPC/EAN/GTIN are numeric; scanners and labels sprinkle
 *  spaces and hyphens, and a pasted code often arrives with both. */
export function normalizeBarcode(raw: string): string {
  return raw.replace(/\D/g, "")
}

/** GTIN-8 / UPC-A(12) / EAN-13 / GTIN-14 are the shapes the lookup can serve. Checked client-side only
 *  to avoid burning an outbound-quota request on an obvious typo — the server still validates. */
export function isPlausibleBarcode(raw: string): boolean {
  const d = normalizeBarcode(raw)
  return d.length === 8 || d.length === 12 || d.length === 13 || d.length === 14
}

/** The distinct sources present in a result set, in render order — one attribution line each, so a
 *  mixed list credits both without repeating a line per row. */
export function attributionLines(foods: BrandedFoodVM[]): string[] {
  const seen = new Set<string>()
  const lines: string[] = []
  for (const f of foods) {
    if (seen.has(f.attribution)) continue
    seen.add(f.attribution)
    lines.push(f.attribution)
  }
  return lines
}

// --- barcode scanning ---------------------------------------------------------------------------

// The Barcode Detection API, where the platform ships it. Typed structurally rather than pulled from
// a DOM lib: it is not in the baseline TS DOM types, and a structural type keeps this honest about
// the only two members we touch.
type BarcodeDetectorLike = {
  detect: (source: ImageBitmapSource) => Promise<{ rawValue: string }[]>
}
type BarcodeDetectorCtor = new (opts?: {
  formats?: string[]
}) => BarcodeDetectorLike

const BARCODE_FORMATS = ["ean_13", "ean_8", "upc_a", "upc_e", "code_128"]

function barcodeDetectorCtor(): BarcodeDetectorCtor | null {
  if (typeof globalThis === "undefined") return null
  const ctor = (globalThis as { BarcodeDetector?: BarcodeDetectorCtor })
    .BarcodeDetector
  return typeof ctor === "function" ? ctor : null
}

/** True when this platform can decode a barcode out of a photo the user takes. Feature-detected, never
 *  UA-sniffed — Chrome/Android has it, most desktop Safari/Firefox builds do not, and both are fine
 *  because manual entry is the universal path underneath. */
export function isPhotoScanSupported(): boolean {
  return barcodeDetectorCtor() !== null
}

/**
 * Decode a barcode from a still image the user just captured or picked. Deliberately image-based
 * rather than a live `getUserMedia` video loop: the roadmap's P2.3 rules out getUserMedia scanning on
 * web/desktop, and a captured frame needs no camera-permission plumbing, no stream teardown, and no
 * always-on preview. Returns null when nothing decodes — the caller falls back to the manual field.
 */
export async function scanBarcodeFromImage(file: Blob): Promise<string | null> {
  const Ctor = barcodeDetectorCtor()
  if (!Ctor) return null
  try {
    const detector = new Ctor({ formats: BARCODE_FORMATS })
    const bitmap = await createImageBitmap(file)
    try {
      const found = await detector.detect(bitmap)
      const first = found.find((b) => normalizeBarcode(b.rawValue).length >= 8)
      return first ? normalizeBarcode(first.rawValue) : null
    } finally {
      bitmap.close?.()
    }
  } catch {
    // A detector that throws (unsupported format list, undecodable image) is a miss, not a failure —
    // the user types the digits instead.
    return null
  }
}

// --- components ---------------------------------------------------------------------------------

/** The "this came from somewhere else" marker. Branded results are third-party records, not curated
 *  catalog entries, and the reader is told so before they pick one. */
export function BrandedSourceBadge({
  source,
  className
}: {
  source: BrandedFoodVM["source"]
  className?: string
}) {
  return (
    <span
      className={cn(
        "shrink-0 rounded-full bg-muted px-1.5 py-0.5 font-medium text-[0.65rem] text-muted-foreground uppercase tracking-wide",
        className
      )}
    >
      {BRANDED_SOURCE_LABEL[source]}
    </span>
  )
}

/**
 * The diet-flag readout. WARNINGS ONLY, in both branches:
 *  - flags found → "Contains: milk, rennet", quoting the label's own words;
 *  - no flags   → an explicit non-answer, because an empty result means "we matched nothing", which
 *    is NOT the same claim as "this is vegan" and must never be rendered as one.
 */
export function DietFlagNotice({
  flags,
  hasIngredientsText,
  className
}: {
  flags: BrandedDietFlagVM[]
  /** False ⇒ the source published no ingredient statement, so there was nothing to check at all. */
  hasIngredientsText: boolean
  className?: string
}) {
  if (flags.length > 0) {
    // De-duplicate by canonical term but show the label's own spelling.
    const seen = new Set<string>()
    const shown: BrandedDietFlagVM[] = []
    for (const f of flags) {
      if (seen.has(f.term)) continue
      seen.add(f.term)
      shown.push(f)
    }
    const categories = [
      ...new Set(shown.map((f) => CATEGORY_LABEL[f.category]))
    ]
    return (
      <p
        className={cn(
          "flex items-start gap-1.5 text-[0.7rem] text-destructive leading-snug",
          className
        )}
      >
        <TriangleAlertIcon className="mt-px size-3 shrink-0" aria-hidden />
        <span>
          <strong className="font-medium">Contains:</strong>{" "}
          {shown.map((f) => f.matched.toLowerCase()).join(", ")}
          <span className="text-muted-foreground">
            {" "}
            ({categories.join(", ")})
          </span>
        </span>
      </p>
    )
  }
  return (
    <p
      className={cn(
        "text-[0.7rem] text-muted-foreground leading-snug",
        className
      )}
    >
      {hasIngredientsText
        ? "No animal-derived ingredients matched this label — that is not a vegan certification. Check the packaging."
        : "This source published no ingredient list, so nothing could be checked. Check the packaging."}
    </p>
  )
}

/** One branded result as a pickable row. Picking promotes it into the catalog and hands the caller the
 *  resulting ingredient id — from then on it behaves like any other catalog food. */
export function BrandedFoodRow({
  food,
  onPick,
  busy
}: {
  food: BrandedFoodVM
  onPick: (food: BrandedFoodVM) => void
  busy?: boolean
}) {
  return (
    <button
      type="button"
      disabled={busy}
      onClick={() => onPick(food)}
      className="w-full rounded-sm px-2 py-1.5 text-left text-sm transition hover:bg-accent disabled:opacity-60"
    >
      <span className="flex items-baseline gap-2">
        <span className="min-w-0 flex-1 truncate font-medium">
          {brandedCatalogName(food)}
        </span>
        <BrandedSourceBadge source={food.source} />
        {food.caloriesPer100g != null ? (
          <span className="shrink-0 text-muted-foreground text-xs tabular-nums">
            {Math.round(food.caloriesPer100g)} cal/100g
          </span>
        ) : null}
      </span>
      <DietFlagNotice
        flags={food.dietFlags}
        hasIngredientsText={!!food.ingredientsText}
        className="mt-0.5"
      />
    </button>
  )
}

/** The branded group inside an existing search list: a header that says plainly these are external
 *  records, the rows, and the source attribution beneath them. */
export function BrandedResults({
  foods,
  onPick,
  busy,
  searching
}: {
  foods: BrandedFoodVM[]
  onPick: (food: BrandedFoodVM) => void
  busy?: boolean
  searching?: boolean
}) {
  if (!searching && foods.length === 0) return null
  return (
    <>
      <li className="mt-1 border-border border-t px-2 pt-1.5 pb-0.5 font-medium text-muted-foreground text-xs">
        Branded &amp; packaged foods
      </li>
      {searching && foods.length === 0 ? (
        <li className="px-2 py-1.5 text-muted-foreground text-sm">
          Looking up branded foods…
        </li>
      ) : null}
      {foods.map((f) => (
        <li key={`${f.source}:${f.externalId}`}>
          <BrandedFoodRow food={f} onPick={onPick} busy={busy} />
        </li>
      ))}
      {attributionLines(foods).map((line) => (
        <li
          key={line}
          className="px-2 pt-1 pb-0.5 text-[0.65rem] text-muted-foreground/70 leading-snug"
        >
          {line}
        </li>
      ))}
      {foods.length > 0 ? (
        <li className="px-2 pb-1">
          <a
            href="/sources"
            className="text-[0.65rem] text-muted-foreground/70 underline underline-offset-2 hover:text-foreground"
          >
            Data sources &amp; licenses
          </a>
        </li>
      ) : null}
    </>
  )
}

/**
 * BARCODE ENTRY — universal manual UPC field, with a camera path layered on only where it is safe.
 *
 * Platform ladder, in order of preference:
 *  1. `adapter.scanNative` — a real native scanner (iOS, once P2.3's native half is deploy-verified).
 *  2. Photo scan via the Barcode Detection API, feature-detected: the user takes/picks ONE photo and
 *     it is decoded locally. No getUserMedia, no live preview (P2.3 rules those out on web/desktop).
 *  3. Typing the digits. Always present, on every platform, as the floor.
 *
 * A barcode nothing resolves is not an error state: it says so and leaves the user in search.
 */
export function BarcodeEntry({
  branded,
  onResolved,
  onCancel
}: {
  branded: BrandedAdapter
  /** A resolved food, ready to pick. */
  onResolved: (food: BrandedFoodVM) => void
  onCancel?: () => void
}) {
  const [code, setCode] = useState("")
  const [status, setStatus] = useState<"idle" | "looking" | "miss" | "error">(
    "idle"
  )
  const [photoScan, setPhotoScan] = useState(false)
  const fileRef = useRef<HTMLInputElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  // Feature detection runs on the client only — `isPhotoScanSupported()` reads a global the SSR pass
  // does not have, so deciding it during render would hydrate-mismatch (server says no, client yes).
  useEffect(() => setPhotoScan(isPhotoScanSupported()), [])
  useEffect(() => inputRef.current?.focus(), [])

  const lookup = async (raw: string) => {
    const gtin = normalizeBarcode(raw)
    if (!isPlausibleBarcode(gtin)) return
    setStatus("looking")
    try {
      const food = await branded.barcode(gtin)
      if (food) {
        setStatus("idle")
        onResolved(food)
      } else {
        setStatus("miss")
      }
    } catch {
      setStatus("error")
    }
  }

  const scanPhoto = async (file: File | undefined) => {
    if (!file) return
    setStatus("looking")
    const found = await scanBarcodeFromImage(file)
    if (!found) {
      setStatus("miss")
      return
    }
    setCode(found)
    await lookup(found)
  }

  const scanNative = async () => {
    if (!branded.scanNative) return
    setStatus("looking")
    try {
      const found = await branded.scanNative()
      if (!found) {
        setStatus("idle")
        return
      }
      setCode(found)
      await lookup(found)
    } catch {
      setStatus("error")
    }
  }

  return (
    <div className="rounded-md border border-border p-2">
      <div className="flex items-center gap-2">
        <BarcodeIcon className="size-4 shrink-0 text-muted-foreground" />
        <input
          ref={inputRef}
          value={code}
          inputMode="numeric"
          autoComplete="off"
          placeholder="Barcode number"
          aria-label="Barcode number"
          onChange={(e) => {
            setCode(e.target.value)
            setStatus("idle")
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") void lookup(code)
            if (e.key === "Escape" && onCancel) onCancel()
          }}
          className="min-w-0 flex-1 rounded-md border border-border bg-background px-2 py-1.5 text-sm outline-none focus:border-primary"
        />
        <button
          type="button"
          disabled={!isPlausibleBarcode(code) || status === "looking"}
          onClick={() => void lookup(code)}
          className={buttonClasses({ variant: "outline", size: "sm" })}
        >
          {status === "looking" ? "Looking…" : "Look up"}
        </button>
      </div>

      {branded.scanNative ? (
        <button
          type="button"
          onClick={() => void scanNative()}
          className="mt-2 flex items-center gap-1.5 text-primary text-xs hover:underline"
        >
          <CameraIcon className="size-3.5" /> Scan with camera
        </button>
      ) : photoScan ? (
        <>
          <button
            type="button"
            onClick={() => fileRef.current?.click()}
            className="mt-2 flex items-center gap-1.5 text-primary text-xs hover:underline"
          >
            <CameraIcon className="size-3.5" /> Scan from a photo
          </button>
          <input
            ref={fileRef}
            type="file"
            accept="image/*"
            capture="environment"
            className="hidden"
            aria-label="Photo of a barcode"
            onChange={(e) => {
              void scanPhoto(e.target.files?.[0])
              e.target.value = ""
            }}
          />
        </>
      ) : null}

      {status === "miss" ? (
        <p className="mt-1.5 text-muted-foreground text-xs leading-snug">
          No match for that barcode. Search by name instead, or add it as your
          own ingredient.
        </p>
      ) : null}
      {status === "error" ? (
        <p className="mt-1.5 text-muted-foreground text-xs leading-snug">
          Barcode lookup is unavailable right now. Try searching by name.
        </p>
      ) : null}
    </div>
  )
}
