// The servings→grams derivation is the load-bearing shared math behind BOTH the recipe form's save
// and the inline editor's per-field commits (composeRecipeInput in @vegify/ui/recipe-form). If it
// drifts, edited recipes silently get wrong serving sizes, so it's pinned here.
import { describe, expect, it } from 'vitest'
import { composeRecipeInput, type RecipeEditState } from '@vegify/ui/recipe-form'

const base: RecipeEditState = {
  id: 'r1',
  visibility: 'public',
  name: 'Biga',
  subtitle: null,
  directions: null,
  servings: 2,
  items: [
    { ingredientId: 'flour', grams: 300 },
    { ingredientId: 'water', grams: 100 },
  ],
}

describe('composeRecipeInput', () => {
  it('derives servingGrams = total/servings and batchGrams = total', () => {
    const out = composeRecipeInput(base)
    expect(out.batchGrams).toBe(400)
    expect(out.servingGrams).toBe(200)
    expect(out.items).toEqual([
      { ingredientId: 'flour', grams: 300 },
      { ingredientId: 'water', grams: 100 },
    ])
    expect(out.id).toBe('r1')
    expect(out.visibility).toBe('public')
  })

  it('treats null/zero servings as one serving (never divides by zero)', () => {
    expect(composeRecipeInput({ ...base, servings: null }).servingGrams).toBe(400)
    expect(composeRecipeInput({ ...base, servings: 0 }).servingGrams).toBe(400)
  })

  it('nulls both grams fields when the recipe has no weighted items', () => {
    const out = composeRecipeInput({ ...base, items: [] })
    expect(out.servingGrams).toBeNull()
    expect(out.batchGrams).toBeNull()
    expect(out.items).toEqual([])
  })

  it('carries a single amount edit straight through (the inline-commit path)', () => {
    // Simulate InlineNumber committing water 100 → 150 g.
    const edited: RecipeEditState = {
      ...base,
      items: base.items.map((i) => (i.ingredientId === 'water' ? { ...i, grams: 150 } : i)),
    }
    const out = composeRecipeInput(edited)
    expect(out.batchGrams).toBe(450)
    expect(out.servingGrams).toBe(225)
  })
})
