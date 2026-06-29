import type { ComponentType } from "react";
import type { AppShellLinkProps } from "./app-shell";
import { buttonClasses } from "./button";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "./breadcrumb";
import { DetailHero } from "./detail-hero";
import { NutritionFacts, type NutritionFactsData } from "./nutrition-facts";
import { NutritionFactsFab } from "./nutrition-facts-fab";
import { ThemeSetting } from "./theme-setting";

/**
 * SHARED SCREENS — the actual pages (recipe list, detail, ingredient list/detail, search, home),
 * written ONCE and rendered by BOTH shells. They are purely presentational: they take a
 * view-model + a `LinkComponent` nav port, and do no data-fetching or routing of their own.
 *
 * Each shell supplies the two things that genuinely differ:
 *   - data: the desktop maps its on-device IPC results to these view-models; web maps its
 *     loader/Drizzle results to the SAME view-models.
 *   - navigation: web passes a router <Link>; the desktop passes an adapter that maps the
 *     href to its in-process view state. Every navigable element here renders through it, so a new
 *     screen is written once and never drifts between the two apps.
 */
export type NavLink = ComponentType<AppShellLinkProps>;

export type RecipeListItem = { id: string; name: string; subtitle?: string | null };
export type IngredientListItem = { id: string; name: string; caloriesPer100g?: number | null };
/** One ingredient line in a recipe — `href` points at its ingredient page (or recipe page if it's a sub-recipe). */
export type RecipeDetailItem = { key: string; label: string; href: string };
export type RecipeDetailVM = {
  id: string;
  name: string;
  subtitle?: string | null;
  creator?: string | null;
  directions?: string | null;
  items: RecipeDetailItem[];
  nutrition: NutritionFactsData;
};
export type IngredientDetailVM = {
  id: string;
  name: string;
  description?: string | null;
  nutrition: NutritionFactsData;
};

const cardClass =
  "flex items-center gap-4 rounded-xl bg-card p-3 ring-1 ring-foreground/10 transition duration-150 hover:-translate-y-0.5 hover:shadow-lg hover:ring-orange/70";

export function HomeView({ LinkComponent }: { LinkComponent: NavLink }) {
  return (
    <div className="mx-auto flex min-h-[70vh] max-w-3xl flex-col items-center justify-center gap-6 p-8 text-center">
      <h1 className="font-serif text-5xl font-bold text-primary-dark">Vegify</h1>
      <p className="w-full max-w-md text-lg text-gray-500">
        Micronutrition tracking for plant-based cooking
      </p>
      <LinkComponent href="/recipes" className={buttonClasses({ size: "lg" })}>
        Browse recipes
      </LinkComponent>
    </div>
  );
}

export function RecipeListView({
  recipes,
  LinkComponent,
}: {
  recipes: RecipeListItem[];
  LinkComponent: NavLink;
}) {
  return (
    <div className="mx-auto max-w-3xl p-8">
      <div className="mb-8 flex items-end justify-between gap-4">
        <div>
          <h1 className="mb-1 font-serif text-4xl font-bold text-primary-dark">Recipes</h1>
          <p className="text-gray-500">{recipes.length} recipes</p>
        </div>
        <LinkComponent href="/recipes/new" className={buttonClasses({ size: "sm" })}>
          + New recipe
        </LinkComponent>
      </div>
      {recipes.length === 0 ? (
        <p className="text-muted-foreground">No recipes yet — add one.</p>
      ) : (
        <div className="flex flex-col gap-4">
          {recipes.map((r) => (
            <LinkComponent key={r.id} href={`/recipes/${r.id}`} className="block">
              <div className={cardClass}>
                <div className="size-16 shrink-0 rounded-lg bg-muted" />
                <div className="min-w-0">
                  <h3 className="truncate font-serif text-2xl font-semibold">{r.name}</h3>
                  <p className="truncate text-sm text-muted-foreground">{r.subtitle ?? "Recipe"}</p>
                </div>
              </div>
            </LinkComponent>
          ))}
        </div>
      )}
    </div>
  );
}

export function IngredientListView({
  ingredients,
  LinkComponent,
}: {
  ingredients: IngredientListItem[];
  LinkComponent: NavLink;
}) {
  return (
    <div className="mx-auto max-w-3xl p-8">
      <div className="mb-8 flex items-end justify-between gap-4">
        <div>
          <h1 className="mb-1 font-serif text-4xl font-bold text-primary-dark">Ingredients</h1>
          <p className="text-gray-500">{ingredients.length} ingredients</p>
        </div>
        <LinkComponent href="/ingredients/new" className={buttonClasses({ size: "sm" })}>
          + New ingredient
        </LinkComponent>
      </div>
      {ingredients.length === 0 ? (
        <p className="text-muted-foreground">No ingredients yet — add one.</p>
      ) : (
        <div className="flex flex-col gap-4">
          {ingredients.map((i) => (
            <LinkComponent key={i.id} href={`/ingredients/${i.id}`} className="block">
              <div className={cardClass}>
                <div className="size-16 shrink-0 rounded-lg bg-muted" />
                <div className="min-w-0">
                  <h3 className="truncate font-serif text-2xl font-semibold">{i.name}</h3>
                  {i.caloriesPer100g != null ? (
                    <p className="text-sm text-muted-foreground">
                      {Math.round(i.caloriesPer100g)} cal/100g
                    </p>
                  ) : null}
                </div>
              </div>
            </LinkComponent>
          ))}
        </div>
      )}
    </div>
  );
}

export function RecipeDetailView({
  recipe,
  LinkComponent,
}: {
  recipe: RecipeDetailVM;
  LinkComponent: NavLink;
}) {
  return (
    <div className="flex">
      <div className="min-w-0 flex-1">
        <div className="mx-auto max-w-3xl p-6 lg:p-8">
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem>
                <BreadcrumbLink>@{recipe.creator ?? "user"}</BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem>
                <BreadcrumbPage>{recipe.name}</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>

          <DetailHero
            label="Recipe Image"
            editHref={`/recipes/${recipe.id}/edit`}
            LinkComponent={LinkComponent}
            className="mt-4"
          />

          <h1 className="mt-10 text-center font-serif text-4xl font-bold text-primary-dark">
            {recipe.name}
          </h1>
          {recipe.subtitle ? (
            <p className="mt-1 text-center text-muted-foreground">{recipe.subtitle}</p>
          ) : null}

          <h2 className="mt-8 text-center font-serif text-xl font-bold">Ingredients</h2>
          <ul className="mx-auto mt-4 grid max-w-2xl grid-cols-1 gap-x-8 gap-y-1.5 sm:grid-cols-2 lg:grid-cols-3">
            {recipe.items.map((item) => (
              <li key={item.key} className="flex items-start gap-2">
                <span aria-hidden className="mt-[0.55rem] size-1.5 shrink-0 rounded-full bg-primary" />
                <LinkComponent href={item.href} className="text-left hover:text-primary hover:underline">
                  {item.label}
                </LinkComponent>
              </li>
            ))}
          </ul>

          <h2 className="mt-8 text-center font-serif text-xl font-bold">Directions</h2>
          <p className="mt-3 text-muted-foreground">{recipe.directions ?? "No directions yet."}</p>
        </div>
      </div>

      <aside className="hidden w-80 shrink-0 border-l border-border p-6 lg:block">
        <div className="lg:sticky lg:top-6">
          <NutritionFacts data={recipe.nutrition} />
        </div>
      </aside>

      <NutritionFactsFab data={recipe.nutrition} />
    </div>
  );
}

export function IngredientDetailView({
  ingredient,
  LinkComponent,
}: {
  ingredient: IngredientDetailVM;
  LinkComponent: NavLink;
}) {
  return (
    <div className="flex">
      <div className="min-w-0 flex-1">
        <div className="mx-auto max-w-2xl p-6 lg:p-8">
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem>
                <BreadcrumbLink>@user</BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem>
                <BreadcrumbPage>{ingredient.name}</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>

          <DetailHero
            label="Ingredient Image"
            editHref={`/ingredients/${ingredient.id}/edit`}
            LinkComponent={LinkComponent}
            className="mt-4"
          />

          <h1 className="mt-10 text-center font-serif text-4xl font-bold text-primary-dark">
            {ingredient.name}
          </h1>
          <h2 className="mt-6 text-center font-serif text-xl font-bold">Information</h2>
          <p className="mt-3 text-muted-foreground">
            {ingredient.description ?? "No description yet."}
          </p>
        </div>
      </div>

      <aside className="hidden w-80 shrink-0 border-l border-border p-6 lg:block">
        <div className="lg:sticky lg:top-6">
          <NutritionFacts data={ingredient.nutrition} />
        </div>
      </aside>

      <NutritionFactsFab data={ingredient.nutrition} />
    </div>
  );
}

function ResultRow({ name, sub, href, LinkComponent }: { name: string; sub: string; href: string; LinkComponent: NavLink }) {
  return (
    <LinkComponent href={href} className="block">
      <div className="flex items-center gap-4 rounded-xl bg-card p-3 ring-1 ring-foreground/10 transition duration-150 hover:-translate-y-0.5 hover:shadow-lg hover:ring-orange/70">
        <div className="size-12 shrink-0 rounded-lg bg-muted" />
        <div className="min-w-0">
          <h3 className="truncate font-serif text-xl font-semibold">{name}</h3>
          <p className="truncate text-sm text-muted-foreground">{sub}</p>
        </div>
      </div>
    </LinkComponent>
  );
}

export function SearchResultsView({
  query,
  recipes,
  ingredients,
  LinkComponent,
}: {
  query: string;
  recipes: RecipeListItem[];
  ingredients: IngredientListItem[];
  LinkComponent: NavLink;
}) {
  const total = recipes.length + ingredients.length;
  return (
    <div className="mx-auto max-w-3xl p-8">
      <h1 className="mb-1 font-serif text-4xl font-bold text-primary-dark">Search</h1>
      <p className="mb-8 text-gray-500">
        {total} {total === 1 ? "result" : "results"} for “{query}”
      </p>
      {total === 0 ? (
        <p className="text-muted-foreground">No recipes or ingredients match.</p>
      ) : (
        <div className="space-y-8">
          {recipes.length > 0 && (
            <section>
              <h2 className="mb-3 font-serif text-xl font-bold">Recipes</h2>
              <div className="flex flex-col gap-3">
                {recipes.map((r) => (
                  <ResultRow
                    key={r.id}
                    name={r.name}
                    sub={r.subtitle ?? "Recipe"}
                    href={`/recipes/${r.id}`}
                    LinkComponent={LinkComponent}
                  />
                ))}
              </div>
            </section>
          )}
          {ingredients.length > 0 && (
            <section>
              <h2 className="mb-3 font-serif text-xl font-bold">Ingredients</h2>
              <div className="flex flex-col gap-3">
                {ingredients.map((i) => (
                  <ResultRow
                    key={i.id}
                    name={i.name}
                    sub={i.caloriesPer100g != null ? `${Math.round(i.caloriesPer100g)} cal/100g` : "Ingredient"}
                    href={`/ingredients/${i.id}`}
                    LinkComponent={LinkComponent}
                  />
                ))}
              </div>
            </section>
          )}
        </div>
      )}
    </div>
  );
}

export function SettingsView() {
  return (
    <div className="mx-auto max-w-3xl p-8">
      <h1 className="mb-1 font-serif text-4xl font-bold text-primary-dark">Settings</h1>
      <p className="mb-8 text-gray-500">Preferences for this account</p>

      <section>
        <h2 className="mb-3 font-serif text-xl font-bold">Appearance</h2>
        <div className="flex flex-col gap-3 rounded-xl bg-card p-4 ring-1 ring-foreground/10 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
          <div className="min-w-0">
            <p className="font-semibold">Theme</p>
            <p className="text-sm text-muted-foreground">
              Match your system, or always use light or dark.
            </p>
          </div>
          <ThemeSetting className="self-start sm:self-auto" />
        </div>
      </section>
    </div>
  );
}
