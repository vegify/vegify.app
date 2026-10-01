import type { NutritionFactsData } from "@vegify/ui/nutrition-facts"
import { describe, expect, it } from "vitest"

import {
  clampDescription,
  composeDescription,
  directionSteps,
  nutritionSentence,
  pageHead
} from "./seo"

// Walnut-like readings per 100 g: copper 1.586/0.9 mg = 176% DV, manganese 3.414/2.3 mg = 148%,
// magnesium 158/420 mg = 38%, vitamin B6 0.537/1.7 mg = 32%, iron 0.5/18 mg = 3%.
const per100g: NutritionFactsData = {
  caloriesPerServing: 654,
  serving: null,
  readings: [
    { name: "Iron", amountPer100g: 0.5, unit: "mg" },
    { name: "Magnesium", amountPer100g: 158, unit: "mg" },
    { name: "Copper", amountPer100g: 1.586, unit: "mg" },
    { name: "Vitamin B6", amountPer100g: 0.537, unit: "mg" },
    { name: "Manganese", amountPer100g: 3.414, unit: "mg" },
    { name: "Sodium", amountPer100g: 2000, unit: "mg" } // 87% DV, but never a selling point
  ]
}

describe("nutritionSentence", () => {
  it("leads with calories and the three richest vitamins and minerals by %DV", () => {
    expect(nutritionSentence(per100g)).toBe(
      "Per 100 g: 654 kcal, richest in copper (176% DV), manganese (148% DV), and magnesium (38% DV)."
    )
  })

  it("scales to the declared serving, like the panel does", () => {
    expect(
      nutritionSentence({
        ...per100g,
        serving: { grams: 28 },
        caloriesPerServing: 183.12
      })
    ).toBe(
      "Per serving (28 g): 183 kcal, richest in copper (49% DV), manganese (42% DV), and magnesium (11% DV)."
    )
  })

  it("names vitamins the way prose does", () => {
    expect(
      nutritionSentence({
        caloriesPerServing: null,
        readings: [
          { name: "Vitamin K", amountPer100g: 60, unit: "µg" },
          { name: "Folate", amountPer100g: 80, unit: "µg" }
        ]
      })
    ).toBe("Per 100 g: richest in vitamin K (50% DV) and folate (20% DV).")
  })

  it("won't repeat numbers denser than fat — the weights behind them are wrong", () => {
    // A 47 g recipe whose "1 g" crust meant one crust: 1817 kcal per 100 g.
    expect(
      nutritionSentence({ ...per100g, caloriesPerServing: 1817 })
    ).toBeNull()
    // The same density per serving: 545 kcal in 30 g.
    expect(
      nutritionSentence({
        ...per100g,
        serving: { grams: 30 },
        caloriesPerServing: 545
      })
    ).toBeNull()
    // Oil, label-rounded a little past 900, is still believable.
    expect(nutritionSentence({ caloriesPerServing: 929, readings: [] })).toBe(
      "Per 100 g: 929 kcal."
    )
  })

  it("says nothing when there is nothing to say", () => {
    expect(nutritionSentence({ caloriesPerServing: null, readings: [] })).toBe(
      null
    )
  })
})

describe("composeDescription", () => {
  it("joins sentences in order, skipping blanks", () => {
    expect(composeDescription(["One.", null, false, "", "Two."])).toBe(
      "One. Two."
    )
  })

  it("drops a sentence that would run past the limit but keeps a later one that fits", () => {
    expect(
      composeDescription(
        ["Short one.", "This sentence is far too long.", "Tiny."],
        20
      )
    ).toBe("Short one. Tiny.")
  })

  it("cuts a lone over-long first sentence at a word boundary", () => {
    const out = composeDescription(["word ".repeat(50)], 24)
    expect(out.length).toBeLessThanOrEqual(24)
    expect(out).toBe("word word word word…")
  })
})

describe("directionSteps", () => {
  it("splits free-text directions into steps and drops list markers", () => {
    expect(
      directionSteps(
        "1. Mix the flour and water.\n2) Rest 18 hours.\n\n- Shape\n• Bake at 260°C"
      )
    ).toEqual([
      "Mix the flour and water.",
      "Rest 18 hours.",
      "Shape",
      "Bake at 260°C"
    ])
  })

  it("keeps a single paragraph whole and handles none", () => {
    expect(directionSteps("Stir it all together.")).toEqual([
      "Stir it all together."
    ])
    expect(directionSteps(null)).toEqual([])
  })

  it("leaves a step that merely starts with a number alone", () => {
    expect(directionSteps("1.5 cups water\n2 tbsp oil")).toEqual([
      "1.5 cups water",
      "2 tbsp oil"
    ])
  })
})

describe("clampDescription", () => {
  it("leaves short text alone and normalizes whitespace", () => {
    expect(clampDescription("  a\n b  ")).toBe("a b")
  })
})

describe("pageHead", () => {
  const head = pageHead({
    origin: "https://example.org",
    path: "/simone/biga",
    title: "Biga",
    description: "A preferment."
  })
  const meta = (key: string) =>
    head.meta.find(
      (m) =>
        ("name" in m && m.name === key) ||
        ("property" in m && m.property === key)
    )

  it("builds every absolute URL from the given origin — no hardcoded domain", () => {
    expect(head.links).toEqual([
      { rel: "canonical", href: "https://example.org/simone/biga" }
    ])
    expect(meta("og:url")).toMatchObject({
      content: "https://example.org/simone/biga"
    })
    expect(meta("og:image")).toMatchObject({
      content: "https://example.org/logo512.png"
    })
  })

  it("suffixes the title and leaves robots alone unless asked", () => {
    expect(head.meta[0]).toEqual({ title: "Biga | Vegify" })
    expect(meta("robots")).toBeUndefined()
    expect(meta("twitter:card")).toMatchObject({ content: "summary" })
    expect(head.meta.some((m) => "script:ld+json" in m)).toBe(false)
  })

  it("adds noindex, a large preview for a real image, and JSON-LD when given", () => {
    const h = pageHead({
      origin: "https://example.org",
      path: "/x",
      title: "X",
      description: "X.",
      image: "https://cdn.example.org/x.jpg",
      noindex: true,
      jsonLd: { "@type": "Recipe", name: "X" }
    })
    expect(h.meta).toContainEqual({ name: "robots", content: "noindex" })
    expect(h.meta).toContainEqual({
      name: "twitter:card",
      content: "summary_large_image"
    })
    expect(h.meta).toContainEqual({
      "script:ld+json": { "@type": "Recipe", name: "X" }
    })
  })
})
