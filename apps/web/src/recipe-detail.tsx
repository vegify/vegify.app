import { useRouter } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { queryOptions, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'
import {
  RecipeDetailView,
  type RecipeDetailVM,
  type RecipeEditAdapter,
  type RecipeEditRow,
  type Visibility,
} from '@vegify/ui/screens'
import { composeRecipeInput, type RecipeEditState } from '@vegify/ui/recipe-form'
import { useEditHistory } from '@vegify/ui/use-edit-history'
import type { IngredientSearchItem } from '@vegify/ui/recipe-form'
import type { NutritionFactsData } from '@vegify/ui/nutrition-facts'
import { LinkAdapter } from './link'

// The recipe detail — SHARED by the canonical `/<username>/<slug>` route (renders) and the legacy
// `/recipes/<id>` route (301s to canonical). Keyed by recipe id throughout; the slug route resolves
// its id first. `canonical` carries the owner handle + slug so `/recipes/<id>` can redirect.
export type RecipeDetailPayload = {
  vm: RecipeDetailVM
  edit: { state: RecipeEditState; rows: RecipeEditRow[] } | null
  canonical: { username: string; slug: string } | null
}

const getRecipe = createServerFn({ method: 'GET' })
  .validator((recipeId: string) => recipeId)
  .handler(async ({ data }): Promise<RecipeDetailPayload | null> => {
    const { getRecipeView, getRecipeEdit } = await import('./content')
    const recipe = await getRecipeView(data) // backend gates canView; null => forbidden/missing
    if (!recipe) return null

    const serving = recipe.serving
    const nutrition: NutritionFactsData = {
      heading: 'This Recipe',
      serving,
      servingsPerBatch:
        recipe.batchGrams != null && serving?.grams ? recipe.batchGrams / serving.grams : null,
      caloriesPerServing:
        recipe.nutrition.caloriesPer100g != null && serving?.grams
          ? (recipe.nutrition.caloriesPer100g * serving.grams) / 100
          : recipe.nutrition.caloriesPer100g,
      readings: recipe.nutrition.readings,
    }

    const href = (item: (typeof recipe.items)[number]) =>
      item.recipeId ? `/recipes/${item.recipeId}` : `/ingredients/${item.id}`

    const vm: RecipeDetailVM = {
      id: recipe.id,
      name: recipe.name,
      subtitle: recipe.subtitle,
      creator: recipe.creator ?? undefined,
      canEdit: recipe.canEdit,
      directions: recipe.directions,
      items: recipe.items.map((item, i) => ({
        key: `${item.id}-${i}`,
        label: `${item.amount.amount ?? ''} ${item.amount.unit ?? ''} ${item.name}`.trim(),
        href: href(item),
        ingredientId: item.id,
        deleted: item.deleted,
      })),
      nutrition,
    }

    const canonical = recipe.creator && recipe.slug ? { username: recipe.creator, slug: recipe.slug } : null

    let edit: RecipeDetailPayload['edit'] = null
    if (recipe.canEdit) {
      const editData = await getRecipeEdit(data)
      if (editData) {
        const hrefById = new Map(recipe.items.map((it) => [it.id, href(it)]))
        edit = {
          state: {
            id: editData.id,
            visibility: editData.visibility,
            name: editData.name,
            subtitle: editData.subtitle,
            directions: editData.directions,
            servings: editData.servings,
            items: editData.items.map((i) => ({ ingredientId: i.ingredientId, grams: i.grams })),
          },
          rows: editData.items.map((i) => ({
            ingredientId: i.ingredientId,
            name: i.name,
            grams: i.grams,
            href: hrefById.get(i.ingredientId) ?? `/ingredients/${i.ingredientId}`,
          })),
        }
      }
    }

    return { vm, edit, canonical }
  })

const searchFn = createServerFn({ method: 'GET' })
  .validator((query: string) => query)
  .handler(async ({ data }) => {
    const { searchIngredients } = await import('./content')
    return searchIngredients(data)
  })

const saveFn = createServerFn({ method: 'POST' })
  .validator((input: ReturnType<typeof composeRecipeInput>) => input)
  .handler(async ({ data }) => {
    const { saveRecipe } = await import('./content')
    return saveRecipe(data)
  })

const deleteFn = createServerFn({ method: 'POST' })
  .validator((id: string) => id)
  .handler(async ({ data }) => {
    const { deleteRecipe } = await import('./content')
    await deleteRecipe(data)
  })

const restoreIngredientFn = createServerFn({ method: 'POST' })
  .validator((id: string) => id)
  .handler(async ({ data }) => {
    const { restoreIngredient } = await import('./content')
    await restoreIngredient(data)
  })

export const recipeQuery = (id: string) =>
  queryOptions({ queryKey: ['recipe', id], queryFn: () => getRecipe({ data: id }) })

/** The detail page, keyed by recipe id (the slug route resolves the id before rendering this). */
export function RecipeDetailPage({ recipeId }: { recipeId: string }) {
  const { data } = useSuspenseQuery(recipeQuery(recipeId))
  const queryClient = useQueryClient()
  const router = useRouter()
  const commit = async (next: RecipeEditState) => {
    const id = await saveFn({ data: composeRecipeInput(next) })
    await queryClient.invalidateQueries({ queryKey: ['recipe', String(id)] })
    await queryClient.invalidateQueries({ queryKey: ['recipes'] })
  }
  const history = useEditHistory(commit)
  if (!data) return <div className="p-8 text-muted-foreground">Recipe not found.</div>

  const editState = data.edit?.state
  const patch = (p: Partial<RecipeEditState>) => {
    history.record(editState!)
    return commit({ ...editState!, ...p })
  }

  const edit: RecipeEditAdapter | undefined =
    data.edit && editState
      ? {
          visibility: editState.visibility,
          items: data.edit.rows,
          rename: (name) => patch({ name }),
          setSubtitle: (subtitle) => patch({ subtitle: subtitle || null }),
          setDirections: (directions) => patch({ directions: directions || null }),
          setVisibility: (visibility) => patch({ visibility: visibility as Visibility }),
          setItemAmount: (ingredientId, grams) =>
            patch({
              items: editState.items.map((i) => (i.ingredientId === ingredientId ? { ...i, grams } : i)),
            }),
          addItem: (ingredient: IngredientSearchItem) =>
            patch({
              items: [
                ...editState.items,
                { ingredientId: ingredient.id, grams: ingredient.servingGrams ?? 100 },
              ],
            }),
          removeItem: (ingredientId) =>
            patch({ items: editState.items.filter((i) => i.ingredientId !== ingredientId) }),
          remove: async () => {
            await deleteFn({ data: editState.id })
            queryClient.removeQueries({ queryKey: ['recipe', editState.id] })
            await queryClient.invalidateQueries({ queryKey: ['recipes'] })
            await router.navigate({ to: '/recipes', search: { sort: 'newest' } })
          },
          search: (q) => searchFn({ data: q }),
          undo: () => void history.undo(editState),
          redo: () => void history.redo(editState),
          canUndo: history.canUndo,
          canRedo: history.canRedo,
        }
      : undefined

  // Restore is owner-scoped like edit: a tombstoned row only ever flags in the deleter's own
  // recipes, and only the owner gets the affordance (edit presence = ownership).
  const onRestoreIngredient = edit
    ? async (ingredientId: string) => {
        await restoreIngredientFn({ data: ingredientId })
        await queryClient.invalidateQueries({ queryKey: ['recipe', recipeId] })
        await queryClient.invalidateQueries({ queryKey: ['ingredients'] })
      }
    : undefined

  return (
    <RecipeDetailView
      recipe={data.vm}
      LinkComponent={LinkAdapter}
      edit={edit}
      onRestoreIngredient={onRestoreIngredient}
    />
  )
}
