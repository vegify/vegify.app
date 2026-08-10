// The branded-food client helpers (@vegify/ui/branded). Two of these encode obligations rather than
// preferences, which is why they are pinned here rather than left to the components:
//
//  - `brandedCatalogName` MIRRORS `BrandedFood::catalog_name` in crates/vegify-core/src/branded.rs.
//    The server decides the name a promoted row actually gets; the composer shows its own label on the
//    row it just added. If the two drift, a just-added ingredient reads under a different name than the
//    one it was saved as, so the mirror is a contract and these cases come straight from the Rust doc.
//  - `attributionLines` is the ODbL credit path: one line per distinct source in a result set. Dropping
//    a line because a list happened to be mixed would drop the Open Food Facts attribution, which is a
//    licence condition, not a nicety.

import { createElement } from "react"
import {
  attributionLines,
  type BrandedDietFlagVM,
  type BrandedFoodVM,
  brandedAsSearchItem,
  brandedCatalogName,
  DietFlagNotice,
  isPlausibleBarcode,
  normalizeBarcode
} from "@vegify/ui/branded"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

const USDA_ATTRIBUTION =
  "Data from USDA FoodData Central (Branded Foods), public domain (CC0)."
const OFF_ATTRIBUTION =
  "Data from Open Food Facts, licensed under the Open Database License (ODbL)."

const food = (over: Partial<BrandedFoodVM> = {}): BrandedFoodVM => ({
  source: "usda-branded",
  externalId: "12345",
  gtin: "0001234567890",
  name: "Oat Drink",
  brand: "Oatly",
  servingGrams: 240,
  servingUnit: "cup",
  ingredientsText: "Oat base (water, oats), rapeseed oil.",
  caloriesPer100g: 60,
  nutrients: [{ name: "Iron", amountPer100g: 0.4, unit: "mg" }],
  ingredientId: null,
  dietFlags: [],
  attribution: USDA_ATTRIBUTION,
  ...over
})

describe("brandedCatalogName", () => {
  it("prefixes the brand when the product name omits it", () => {
    expect(brandedCatalogName({ name: "Oat Drink", brand: "Oatly" })).toBe(
      "Oatly Oat Drink"
    )
  })

  it("leaves a name that already carries the brand alone", () => {
    expect(
      brandedCatalogName({ name: "Oatly Oat Drink", brand: "Oatly" })
    ).toBe("Oatly Oat Drink")
  })

  it("matches the brand case-insensitively", () => {
    expect(
      brandedCatalogName({ name: "OATLY OAT DRINK", brand: "Oatly" })
    ).toBe("OATLY OAT DRINK")
  })

  it("passes the name through when no brand is stated", () => {
    expect(brandedCatalogName({ name: "Oat Drink", brand: null })).toBe(
      "Oat Drink"
    )
    expect(brandedCatalogName({ name: "Oat Drink", brand: "  " })).toBe(
      "Oat Drink"
    )
  })
})

describe("brandedAsSearchItem", () => {
  it("carries the promoted catalog id, not the third-party id", () => {
    const item = brandedAsSearchItem(food(), "ing_01")
    expect(item.id).toBe("ing_01")
    expect(item.name).toBe("Oatly Oat Drink")
    expect(item.servingGrams).toBe(240)
    expect(item.servingUnit).toBe("cup")
  })

  it("coerces an unknown nutrient amount to 0 rather than dropping the row", () => {
    const item = brandedAsSearchItem(
      food({ nutrients: [{ name: "Zinc", amountPer100g: null, unit: "mg" }] }),
      "ing_01"
    )
    expect(item.readings).toEqual([
      { name: "Zinc", amountPer100g: 0, unit: "mg" }
    ])
  })
})

describe("attributionLines", () => {
  it("credits every distinct source exactly once", () => {
    const lines = attributionLines([
      food(),
      food({ externalId: "2" }),
      food({ source: "off", externalId: "3", attribution: OFF_ATTRIBUTION })
    ])
    expect(lines).toEqual([USDA_ATTRIBUTION, OFF_ATTRIBUTION])
  })

  it("never drops the ODbL line from a mixed result set", () => {
    const lines = attributionLines([
      food(),
      food({ source: "off", externalId: "3", attribution: OFF_ATTRIBUTION })
    ])
    expect(lines).toContain(OFF_ATTRIBUTION)
  })

  it("is empty for an empty result set", () => {
    expect(attributionLines([])).toEqual([])
  })
})

// The one-directional-flag invariant, rendered. The Rust matcher never returns "vegan"
// (crates/vegify-core/src/diet.rs) — this pins the OTHER half of that promise: that no client turns
// "we matched nothing" into a certification. It is a wording test on purpose, because the wording IS
// the claim; a green check or the word "vegan" appearing here would be the product lying.
const renderNotice = (
  flags: BrandedDietFlagVM[],
  hasIngredientsText: boolean
) =>
  renderToStaticMarkup(
    createElement(DietFlagNotice, { flags, hasIngredientsText })
  )

describe("DietFlagNotice", () => {
  it("quotes the label's own spelling of every term it found", () => {
    const html = renderNotice(
      [
        { term: "milk", matched: "MILK", category: "dairy" },
        { term: "rennet", matched: "Rennet", category: "meat" }
      ],
      true
    )
    expect(html).toContain("Contains:")
    expect(html).toContain("milk, rennet")
    expect(html).toContain("dairy, meat")
  })

  it("renders an empty result as an explicit NON-answer, never a vegan claim", () => {
    const html = renderNotice([], true)
    expect(html).toContain("No animal-derived ingredients matched this label")
    expect(html).toContain("not a vegan certification")
    expect(html.toLowerCase()).not.toContain(">vegan<")
  })

  it("distinguishes 'checked, found nothing' from 'there was nothing to check'", () => {
    const checked = renderNotice([], true)
    const unchecked = renderNotice([], false)
    expect(unchecked).toContain("published no ingredient list")
    expect(unchecked).not.toBe(checked)
    // Both end on the same instruction: the packaging is the authority, not us.
    expect(checked).toContain("Check the packaging.")
    expect(unchecked).toContain("Check the packaging.")
  })

  it("never claims a food is vegan, in any branch", () => {
    for (const html of [
      renderNotice([], true),
      renderNotice([], false),
      renderNotice([{ term: "milk", matched: "Milk", category: "dairy" }], true)
    ]) {
      expect(html).not.toMatch(/\bis vegan\b/i)
      expect(html).not.toMatch(/vegan-friendly|certified vegan|100% vegan/i)
    }
  })
})

describe("barcode input", () => {
  it("strips the separators labels and scanners add", () => {
    expect(normalizeBarcode(" 0 12345-67890 5 ")).toBe("012345678905")
  })

  it("accepts the GTIN lengths the lookup can serve", () => {
    expect(isPlausibleBarcode("12345670")).toBe(true) // GTIN-8
    expect(isPlausibleBarcode("012345678905")).toBe(true) // UPC-A
    expect(isPlausibleBarcode("4006381333931")).toBe(true) // EAN-13
    expect(isPlausibleBarcode("10012345678902")).toBe(true) // GTIN-14
  })

  it("rejects a half-typed code before it costs an outbound lookup", () => {
    expect(isPlausibleBarcode("")).toBe(false)
    expect(isPlausibleBarcode("0123")).toBe(false)
    expect(isPlausibleBarcode("012345678")).toBe(false)
    expect(isPlausibleBarcode("abcdefghijkl")).toBe(false)
  })
})
