// The shared ingredient detail page, rendered by BOTH canonical routes: /$username/ingredients/$slug
// (owned ingredients, browsable under their creator) and /ingredients/$segment (the communal catalog,
// plus slug-history/legacy-id 301s). Mirrors recipe-detail.tsx's shape: server-fns + query + the page
// component with its inline-edit adapter live here; the route files only resolve/redirect.

import {
  queryOptions,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { redirect, useRouteContext, useRouter } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import type { IngredientFormInput } from "@vegify/ui/ingredient-form";
import type { NutritionFactsData } from "@vegify/ui/nutrition-facts";
import {
  IngredientDetailView,
  type IngredientDetailVM,
  type IngredientEditAdapter,
  type Visibility,
} from "@vegify/ui/screens";
import { useEditHistory } from "@vegify/ui/use-edit-history";
import type { IngredientEditData, IngredientSlugHit } from "./content";
import { LinkAdapter } from "./link";

// The detail payload: the read VM plus, for an owner, the editable state the inline editor patches.
type IngredientDetailPayload = {
  vm: IngredientDetailVM;
  edit: IngredientEditData | null;
  canonical: string | null; // the current slug
  creator: string | null; // owner handle; canonical URL is /<creator>/ingredients/<slug> when set
};

export const getIngredient = createServerFn({ method: "GET" })
  .validator((ingredientId: string) => ingredientId)
  .handler(async ({ data }): Promise<IngredientDetailPayload | null> => {
    const { getIngredientView } = await import("./content");
    const ing = await getIngredientView(data); // backend gates canView; null => forbidden/missing
    if (!ing) return null;

    const scale = ing.servingGrams ? ing.servingGrams / 100 : 1;
    const nutrition: NutritionFactsData = {
      heading: "This Ingredient",
      caloriesPerServing:
        ing.caloriesPer100g != null ? ing.caloriesPer100g * scale : null,
      // The backend's IngredientEditData carries serving GRAMS only (no amount/unit) — the same shape
      // the desktop renders. Enriching it with the serving amount/unit is a possible follow-up.
      serving:
        ing.servingGrams != null
          ? { amount: null, unit: null, grams: ing.servingGrams }
          : null,
      servingsPerBatch:
        ing.packageGrams != null && ing.servingGrams
          ? ing.packageGrams / ing.servingGrams
          : null,
      readings: ing.nutrients.map((n) => ({
        ...n,
        amountPer100g: n.amountPer100g ?? 0,
      })),
    };

    const vm: IngredientDetailVM = {
      id: ing.id,
      name: ing.name,
      description: ing.description,
      canEdit: ing.canEdit,
      deleted: ing.deleted,
      creator: ing.creator,
      nutrition,
    };
    return {
      vm,
      edit: ing.canEdit ? ing : null,
      canonical: ing.slug,
      creator: ing.creator,
    };
  });

// Resolve a slug segment → { ingredientId, canonicalSlug, username } (or null when it's not a slug —
// likely a legacy id, handled by the global route's id fallback).
export const resolveIngredientFn = createServerFn({ method: "GET" })
  .validator((slug: string) => slug)
  .handler(async ({ data }): Promise<IngredientSlugHit | null> => {
    const { resolveIngredientBySlug } = await import("./content");
    return resolveIngredientBySlug(data);
  });

const saveFn = createServerFn({ method: "POST" })
  .validator((input: IngredientFormInput) => input)
  .handler(async ({ data }) => {
    const { saveIngredient } = await import("./content");
    return saveIngredient(data);
  });

const deleteFn = createServerFn({ method: "POST" })
  .validator((id: string) => id)
  .handler(async ({ data }) => {
    const { deleteIngredient } = await import("./content");
    await deleteIngredient(data);
  });

const reportIngredientFn = createServerFn({ method: "POST" })
  .validator((d: { id: string; reason: string; note: string }) => d)
  .handler(async ({ data }) => {
    const { reportContent } = await import("./content");
    await reportContent({
      targetType: "ingredient",
      targetId: data.id,
      reason: data.reason as never,
      note: data.note,
    });
  });

export const ingredientQuery = (id: string) =>
  queryOptions({
    queryKey: ["ingredient", id],
    queryFn: () => getIngredient({ data: id }),
  });

/** The canonical redirect for a resolved slug hit: owned rows live under their creator, the catalog
 * at the global path. Throws when the requested location isn't the canonical one. */
export function redirectToCanonical(
  hit: IngredientSlugHit,
  requested: { username?: string; slug: string },
) {
  const canonicalUser = hit.username ?? undefined;
  if (
    canonicalUser === requested.username &&
    hit.canonicalSlug === requested.slug
  )
    return;
  if (canonicalUser) {
    throw redirect({
      to: "/$username/ingredients/$slug",
      params: { username: canonicalUser, slug: hit.canonicalSlug },
      statusCode: 301,
    });
  }
  throw redirect({
    to: "/ingredients/$ingredientId",
    params: { ingredientId: hit.canonicalSlug },
    statusCode: 301,
  });
}

// Map the full editable state to the save shape (1:1 — the ingredient stores everything per-100g /
// absolute, so no conversion). Preserving `nutrients` here is why a name edit doesn't wipe the panel.
const toInput = (d: IngredientEditData): IngredientFormInput => ({
  id: d.id,
  visibility: d.visibility,
  name: d.name,
  description: d.description,
  price: d.price,
  caloriesPer100g: d.caloriesPer100g,
  servingGrams: d.servingGrams,
  packageGrams: d.packageGrams,
  nutrients: d.nutrients.map((n) => ({
    name: n.name,
    amountPer100g: n.amountPer100g ?? 0,
    unit: n.unit,
  })),
});

export function IngredientDetailPage({
  ingredientId,
}: {
  ingredientId: string;
}) {
  const { user } = useRouteContext({ from: "__root__" });
  const { data } = useSuspenseQuery(ingredientQuery(ingredientId));
  const queryClient = useQueryClient();
  const router = useRouter();
  // commit + history before the early return so hook order stays stable across renders.
  const commit = async (next: IngredientEditData) => {
    const id = await saveFn({ data: toInput(next) });
    await queryClient.invalidateQueries({
      queryKey: ["ingredient", String(id)],
    });
    await queryClient.invalidateQueries({ queryKey: ["ingredients"] });
    // A recipe using this ingredient shows its name/nutrition — refetch recipe views too.
    await queryClient.invalidateQueries({ queryKey: ["recipe"] });
  };
  const history = useEditHistory(commit);
  if (!data)
    return (
      <div className="p-8 text-muted-foreground">Ingredient not found.</div>
    );

  const state = data.edit;
  const patch = (from: IngredientEditData, p: Partial<IngredientEditData>) => {
    history.record(from);
    return commit({ ...from, ...p });
  };

  const edit: IngredientEditAdapter | undefined = state
    ? {
        visibility: state.visibility,
        rename: (name) => patch(state, { name }),
        setDescription: (description) =>
          patch(state, { description: description || null }),
        setVisibility: (visibility) =>
          patch(state, { visibility: visibility as Visibility }),
        remove: async () => {
          await deleteFn({ data: state.id });
          queryClient.removeQueries({ queryKey: ["ingredient", state.id] });
          await queryClient.invalidateQueries({ queryKey: ["ingredients"] });
          await router.navigate({
            to: "/ingredients",
            search: { sort: "newest" },
          });
        },
        undo: () => void history.undo(state),
        redo: () => void history.redo(state),
        canUndo: history.canUndo,
        canRedo: history.canRedo,
      }
    : undefined;

  const onReportContent =
    user && !edit
      ? (reason: string, note: string) =>
          reportIngredientFn({ data: { id: ingredientId, reason, note } })
      : undefined;

  return (
    <IngredientDetailView
      ingredient={data.vm}
      LinkComponent={LinkAdapter}
      edit={edit}
      onReportContent={onReportContent}
    />
  );
}
