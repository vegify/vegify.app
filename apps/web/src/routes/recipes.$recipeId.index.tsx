import { createFileRoute, notFound, useRouter } from '@tanstack/react-router'
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
import type { IngredientSearchItem } from '@vegify/ui/recipe-form'
import type { NutritionFactsData } from '@vegify/ui/nutrition-facts'
import { LinkAdapter } from '../link'

// The detail payload: the read view-model plus, when the viewer owns the recipe, the editable state
// the inline editor patches. Owner-only, so a reader's payload and render are unchanged.
type RecipeDetailPayload = {
  vm: RecipeDetailVM
  edit: { state: RecipeEditState; rows: RecipeEditRow[] } | null
}

const getRecipe = createServerFn({ method: 'GET' })
  .validator((recipeId: string) => recipeId)
  .handler(async ({ data }): Promise<RecipeDetailPayload | null> => {
    const { getRecipeView, getRecipeEdit } = await import('../content')
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
      // An item that is itself a recipe links to the recipe page (the backend resolves recipeId); a
      // leaf ingredient links to its ingredient page. item.id is the ingredient id (vegify-core shape).
      items: recipe.items.map((item, i) => ({
        key: `${item.id}-${i}`,
        label: `${item.amount.amount ?? ''} ${item.amount.unit ?? ''} ${item.name}`.trim(),
        href: href(item),
      })),
      nutrition,
    }

    // Owner → build the editable state. getRecipeEdit is the authoritative editable shape (visibility,
    // servings, item grams); hrefs come from the view items (which carry recipeId) joined by id.
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

    return { vm, edit }
  })

const searchFn = createServerFn({ method: 'GET' })
  .validator((query: string) => query)
  .handler(async ({ data }) => {
    const { searchIngredients } = await import('../content')
    return searchIngredients(data)
  })

const saveFn = createServerFn({ method: 'POST' })
  .validator((input: ReturnType<typeof composeRecipeInput>) => input)
  .handler(async ({ data }) => {
    const { saveRecipe } = await import('../content')
    return saveRecipe(data)
  })

const deleteFn = createServerFn({ method: 'POST' })
  .validator((id: string) => id)
  .handler(async ({ data }) => {
    const { deleteRecipe } = await import('../content')
    await deleteRecipe(data)
  })

const recipeQuery = (id: string) =>
  queryOptions({ queryKey: ['recipe', id], queryFn: () => getRecipe({ data: id }) })

export const Route = createFileRoute('/recipes/$recipeId/')({
  loader: async ({ context, params }) => {
    const recipe = await context.queryClient.ensureQueryData(recipeQuery(params.recipeId))
    if (!recipe) throw notFound()
  },
  component: RecipePage,
})

function RecipePage() {
  const { recipeId } = Route.useParams()
  const { data } = useSuspenseQuery(recipeQuery(recipeId))
  const queryClient = useQueryClient()
  const router = useRouter()
  if (!data) return <div className="p-8 text-muted-foreground">Recipe not found.</div>

  const editState = data.edit?.state
  // Every inline commit patches ONE field of the current edit state, composes the whole-object save
  // (the same shape the form produces), persists, and invalidates so the read view + list refetch.
  // Optimistic UI in the primitives shows the change instantly; a rejected save reverts the field.
  const commit = async (next: RecipeEditState) => {
    const id = await saveFn({ data: composeRecipeInput(next) })
    await queryClient.invalidateQueries({ queryKey: ['recipe', String(id)] })
    await queryClient.invalidateQueries({ queryKey: ['recipes'] })
  }
  const patch = (p: Partial<RecipeEditState>) => commit({ ...editState!, ...p })

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
        }
      : undefined

  return <RecipeDetailView recipe={data.vm} LinkComponent={LinkAdapter} edit={edit} />
}
