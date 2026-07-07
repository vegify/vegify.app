import { useQuery } from "@tanstack/react-query";
import { createServerFn } from "@tanstack/react-start";
import {
  type IngredientListItem,
  type NavLink,
  type RecipeListItem,
  SearchResultsView,
} from "@vegify/ui/screens";

// Database-wide search: recipe names + standalone-ingredient names. Small dataset, so it filters in
// JS; swap for a LIKE query if the catalog grows. Mirrors the desktop's chrome search.
const searchAll = createServerFn({ method: "GET" })
  .validator((query: string) => query)
  .handler(async ({ data }) => {
    // The backend's lists are already viewer-scoped (recipe as-ingredients excluded); filter by name
    // in JS — the catalog is small. Swap for a server-side query if it grows. Mirrors the desktop.
    const { listRecipeCards, listIngredientCards } = await import("./content");
    const q = data.toLowerCase();
    const [recipes, ingredients] = await Promise.all([
      listRecipeCards(),
      listIngredientCards(),
    ]);
    const recipeHits: RecipeListItem[] = recipes.filter((r) =>
      r.name.toLowerCase().includes(q),
    );
    const ingredientHits: IngredientListItem[] = ingredients.filter((i) =>
      i.name.toLowerCase().includes(q),
    );
    return { recipes: recipeHits, ingredients: ingredientHits };
  });

const EMPTY = {
  recipes: [] as RecipeListItem[],
  ingredients: [] as IngredientListItem[],
};

/** Chrome search overlay — renders the shared SearchResultsView from a TanStack Query (client-side
 *  live search). placeholderData keeps the prior results on screen while the next keystroke's query
 *  loads, so the list doesn't flicker to empty between debounced inputs. */
export function SearchOverlay({
  query,
  LinkComponent,
}: {
  query: string;
  LinkComponent: NavLink;
}) {
  const { data } = useQuery({
    queryKey: ["search", query],
    queryFn: () => searchAll({ data: query }),
    placeholderData: (prev) => prev,
  });
  const res = data ?? EMPTY;
  return (
    <SearchResultsView
      query={query}
      recipes={res.recipes}
      ingredients={res.ingredients}
      LinkComponent={LinkComponent}
    />
  );
}
