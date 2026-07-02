import { type ComponentType, useEffect, useRef, useState } from "react";
import { MoreHorizontal, Plus, Trash2 } from "lucide-react";
import type { AppShellLinkProps } from "./app-shell";
import { buttonClasses } from "./button";
import { SORT_OPTIONS, type Sort } from "./catalog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./select";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "./dropdown-menu";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./dialog";
import { InlineNumber, InlinePillSelect, InlineText, InlineTextarea } from "./inline";
import { DETAIL_SHORTCUTS, useDetailShortcuts } from "./use-detail-shortcuts";
import type { IngredientSearchItem } from "./recipe-form";
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
  /** Whether the viewer owns this recipe — shows the edit affordance. Omitted/false ⇒ read-only. */
  canEdit?: boolean;
  directions?: string | null;
  items: RecipeDetailItem[];
  nutrition: NutritionFactsData;
};

export type Visibility = "public" | "unlisted" | "private";
const VISIBILITY_OPTIONS: readonly { value: Visibility; label: string }[] = [
  { value: "public", label: "Public" },
  { value: "unlisted", label: "Unlisted" },
  { value: "private", label: "Private" },
];

/**
 * One editable ingredient row. Amounts edit in GRAMS — the app's internal model and exactly what the
 * existing recipe form edits (the read view's "2 cups"-style labels are a display nicety layered on
 * top). So owner edit mode shows grams; a reader still sees the composed labels.
 */
export type RecipeEditRow = {
  ingredientId: string;
  name: string;
  href: string;
  grams: number;
};

/**
 * The inline-edit adapter (design/inline-edit.md). When present on RecipeDetailView, the detail page
 * BECOMES the editor: each field edits in place and commits one change through these callbacks (the
 * shell composes it into the existing whole-object save). Absent ⇒ today's read-only render, so a
 * logged-out or non-owner view is byte-identical to before inline editing existed.
 */
export type RecipeEditAdapter = {
  visibility: Visibility;
  /** Structured items, parallel to recipe.items — the source for the inline amount chips. */
  items: RecipeEditRow[];
  rename: (next: string) => Promise<void>;
  setSubtitle: (next: string) => Promise<void>;
  setDirections: (next: string) => Promise<void>;
  setVisibility: (next: Visibility) => Promise<void>;
  setItemAmount: (ingredientId: string, amount: number) => Promise<void>;
  addItem: (ingredient: IngredientSearchItem) => Promise<void>;
  removeItem: (ingredientId: string) => Promise<void>;
  remove: () => Promise<void>;
  search: (q: string) => Promise<IngredientSearchItem[]>;
  /** Create-blank draft: the name auto-opens and, while still untitled + empty, a Discard is offered. */
  isDraft?: boolean;
  discard?: () => Promise<void>;
};
export type IngredientDetailVM = {
  id: string;
  name: string;
  description?: string | null;
  /** Whether the viewer owns this ingredient — shows the edit affordance. Omitted/false ⇒ read-only. */
  canEdit?: boolean;
  nutrition: NutritionFactsData;
};
/** A public profile: the handle, display name, and the user's visible recipes (shared by both shells). */
export type ProfileVM = {
  username: string;
  name: string;
  recipes: RecipeListItem[];
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

/** Sort dropdown for the catalog lists. The selected value is owned by the shell (URL-backed). */
function SortControl({ value, onChange }: { value: Sort; onChange: (s: Sort) => void }) {
  return (
    <Select items={SORT_OPTIONS} value={value} onValueChange={(v) => v && onChange(v as Sort)}>
      <SelectTrigger size="sm" aria-label="Sort order">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {SORT_OPTIONS.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/**
 * Bottom-of-list marker for infinite scroll. The shell wires `onLoadMore` to its infinite query's
 * fetchNextPage and `hasMore`/`isLoading` to its state; this asks for the next page when scrolled
 * near. The intersecting state is its own effect so a load that leaves the sentinel still in view
 * (short pages) re-fires once `isLoading` clears.
 */
function InfiniteSentinel({
  hasMore,
  isLoading,
  onLoadMore,
}: {
  hasMore?: boolean;
  isLoading?: boolean;
  onLoadMore: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [intersecting, setIntersecting] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const obs = new IntersectionObserver((entries) => setIntersecting(entries[0]?.isIntersecting ?? false), {
      rootMargin: "400px",
    });
    obs.observe(el);
    return () => obs.disconnect();
  }, []);
  useEffect(() => {
    if (intersecting && hasMore && !isLoading) onLoadMore();
  }, [intersecting, hasMore, isLoading, onLoadMore]);
  return (
    <div ref={ref} className="flex justify-center py-6 text-sm text-muted-foreground">
      {isLoading ? "Loading…" : null}
    </div>
  );
}

export function RecipeListView({
  recipes,
  canCreate = false,
  LinkComponent,
  sort,
  onSortChange,
  onLoadMore,
  hasMore,
  isLoadingMore,
}: {
  recipes: RecipeListItem[];
  /** Whether the viewer can add recipes (signed in). Omitted/false hides the "New recipe" action. */
  canCreate?: boolean;
  LinkComponent: NavLink;
  /** Current sort + change handler. Omitted (e.g. a profile's recipe list) hides the sort control. */
  sort?: Sort;
  onSortChange?: (s: Sort) => void;
  /** Infinite scroll: when `onLoadMore` is set, a sentinel requests the next page on scroll. */
  onLoadMore?: () => void;
  hasMore?: boolean;
  isLoadingMore?: boolean;
}) {
  return (
    <div className="mx-auto max-w-3xl p-8">
      <div className="mb-8 flex items-end justify-between gap-4">
        <div>
          <h1 className="mb-1 font-serif text-4xl font-bold text-primary-dark">Recipes</h1>
          <p className="text-gray-500">{recipes.length} recipes</p>
        </div>
        <div className="flex items-center gap-2">
          {onSortChange ? <SortControl value={sort ?? "newest"} onChange={onSortChange} /> : null}
          {canCreate ? (
            <LinkComponent href="/recipes/new" className={buttonClasses({ size: "sm" })}>
              + New recipe
            </LinkComponent>
          ) : null}
        </div>
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
          {onLoadMore ? (
            <InfiniteSentinel hasMore={hasMore} isLoading={isLoadingMore} onLoadMore={onLoadMore} />
          ) : null}
        </div>
      )}
    </div>
  );
}

export function ProfileView({
  username,
  profile,
  LinkComponent,
}: {
  /** The handle from the route — shown when no account claims it. */
  username: string;
  profile: ProfileVM | null;
  LinkComponent: NavLink;
}) {
  if (!profile) {
    return (
      <div className="mx-auto max-w-3xl p-8 text-center">
        <h1 className="mb-2 font-serif text-4xl font-bold text-primary-dark">@{username}</h1>
        <p className="text-muted-foreground">No one goes by that handle.</p>
      </div>
    );
  }
  return (
    <div className="mx-auto max-w-3xl p-8">
      <header className="mb-8 flex items-center gap-5">
        <div className="flex size-20 shrink-0 items-center justify-center rounded-full bg-primary/10 font-serif text-3xl font-bold uppercase text-primary-dark">
          {profile.name.trim().charAt(0) || "?"}
        </div>
        <div className="min-w-0">
          <h1 className="truncate font-serif text-4xl font-bold text-primary-dark">{profile.name}</h1>
          <p className="truncate text-lg text-muted-foreground">@{profile.username}</p>
        </div>
      </header>

      <section className="mb-10">
        <h2 className="mb-4 font-serif text-2xl font-semibold text-foreground">
          Recipes <span className="font-normal text-muted-foreground">· {profile.recipes.length}</span>
        </h2>
        {profile.recipes.length === 0 ? (
          <p className="text-muted-foreground">No public recipes yet.</p>
        ) : (
          <div className="flex flex-col gap-4">
            {profile.recipes.map((r) => (
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
      </section>

      {/* Site-map sections not built yet — surfaced as disabled placeholders, like the nav's "soon" items. */}
      <section className="grid gap-3 sm:grid-cols-3">
        {["Meal plans", "Followers", "Following"].map((label) => (
          <div
            key={label}
            className="flex items-center justify-between rounded-lg bg-card px-4 py-3 ring-1 ring-foreground/10"
          >
            <span className="text-sm font-medium text-muted-foreground">{label}</span>
            <span className="rounded-full bg-muted px-2 py-0.5 text-[0.65rem] font-semibold uppercase tracking-wide text-muted-foreground">
              soon
            </span>
          </div>
        ))}
      </section>
    </div>
  );
}

export function IngredientListView({
  ingredients,
  canCreate = false,
  LinkComponent,
  sort,
  onSortChange,
  onLoadMore,
  hasMore,
  isLoadingMore,
}: {
  ingredients: IngredientListItem[];
  /** Whether the viewer can add ingredients (signed in). Omitted/false hides the "New ingredient" action. */
  canCreate?: boolean;
  LinkComponent: NavLink;
  /** Current sort + change handler. Omitted hides the sort control. */
  sort?: Sort;
  onSortChange?: (s: Sort) => void;
  /** Infinite scroll: when `onLoadMore` is set, a sentinel requests the next page on scroll. */
  onLoadMore?: () => void;
  hasMore?: boolean;
  isLoadingMore?: boolean;
}) {
  return (
    <div className="mx-auto max-w-3xl p-8">
      <div className="mb-8 flex items-end justify-between gap-4">
        <div>
          <h1 className="mb-1 font-serif text-4xl font-bold text-primary-dark">Ingredients</h1>
          <p className="text-gray-500">{ingredients.length} ingredients</p>
        </div>
        <div className="flex items-center gap-2">
          {onSortChange ? <SortControl value={sort ?? "newest"} onChange={onSortChange} /> : null}
          {canCreate ? (
            <LinkComponent href="/ingredients/new" className={buttonClasses({ size: "sm" })}>
              + New ingredient
            </LinkComponent>
          ) : null}
        </div>
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
          {onLoadMore ? (
            <InfiniteSentinel hasMore={hasMore} isLoading={isLoadingMore} onLoadMore={onLoadMore} />
          ) : null}
        </div>
      )}
    </div>
  );
}

export function RecipeDetailView({
  recipe,
  LinkComponent,
  edit,
}: {
  recipe: RecipeDetailVM;
  LinkComponent: NavLink;
  /** Present ⇒ the page edits in place (owner). Absent ⇒ read-only, unchanged from before. */
  edit?: RecipeEditAdapter;
}) {
  const [addOpen, setAddOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  // Page-level shortcuts (owner only). `e`/`v` drive the inline fields via their DOM markers so the
  // primitives stay the single source of their own edit state; `a`/`?`/⌘⌫ open local UI.
  useDetailShortcuts(
    {
      onEditName: () => queryClick('[data-inline-field="name"]'),
      onAddIngredient: () => setAddOpen(true),
      onVisibility: () => queryFocus('[data-inline-field="visibility"]'),
      onHelp: () => setHelpOpen((v) => !v),
      onDelete: () => setDeleteOpen(true),
    },
    !!edit,
  );

  return (
    <div className="flex">
      <div className="min-w-0 flex-1">
        <div className="mx-auto max-w-3xl p-6 lg:p-8">
          <div className="flex items-start justify-between gap-4">
            <Breadcrumb>
              <BreadcrumbList>
                <BreadcrumbItem>
                  <BreadcrumbLink>@{recipe.creator ?? "user"}</BreadcrumbLink>
                </BreadcrumbItem>
                <BreadcrumbSeparator />
                <BreadcrumbItem>
                  <BreadcrumbPage>{recipe.name || "Untitled recipe"}</BreadcrumbPage>
                </BreadcrumbItem>
              </BreadcrumbList>
            </Breadcrumb>
            {edit ? (
              <div className="flex shrink-0 items-center gap-2">
                <InlinePillSelect
                  value={edit.visibility}
                  options={VISIBILITY_OPTIONS}
                  onCommit={edit.setVisibility}
                  ariaLabel="visibility"
                />
                <RecipeOverflowMenu
                  isDraft={edit.isDraft}
                  onDelete={() => setDeleteOpen(true)}
                  onDiscard={edit.discard}
                  onHelp={() => setHelpOpen(true)}
                />
              </div>
            ) : null}
          </div>

          <DetailHero
            label="Recipe Image"
            // Inline mode is the editor now — the hero no longer links to the /edit form.
            editHref={recipe.canEdit && !edit ? `/recipes/${recipe.id}/edit` : undefined}
            LinkComponent={LinkComponent}
            className="mt-4"
          />

          <h1 className="mt-10 text-center font-serif text-4xl font-bold text-primary-dark dark:text-primary-light">
            <InlineText
              as="span"
              value={recipe.name}
              onCommit={edit?.rename}
              required
              placeholder="Untitled recipe"
              ariaLabel="name"
              autoEdit={edit?.isDraft}
              selectAllOnEdit={edit?.isDraft}
              className="inline-block"
            />
          </h1>
          {edit || recipe.subtitle ? (
            <p className="mt-1 text-center text-muted-foreground">
              <InlineText
                as="span"
                value={recipe.subtitle ?? ""}
                onCommit={edit?.setSubtitle}
                placeholder={edit ? "Add a subtitle" : ""}
                ariaLabel="subtitle"
                className="inline-block"
              />
            </p>
          ) : null}

          <h2 className="mt-8 text-center font-serif text-xl font-bold">Ingredients</h2>
          <ul className="mx-auto mt-4 grid max-w-2xl grid-cols-1 gap-x-8 gap-y-1.5 sm:grid-cols-2 lg:grid-cols-3">
            {edit
              ? edit.items.map((row) => (
                  <li key={row.ingredientId} className="group flex items-start gap-2">
                    <span aria-hidden className="mt-[0.55rem] size-1.5 shrink-0 rounded-full bg-primary" />
                    <span className="min-w-0 flex-1 text-left">
                      <InlineNumber
                        value={row.grams}
                        suffix="g"
                        group="recipe-items"
                        onCommit={(n) => edit.setItemAmount(row.ingredientId, n)}
                        ariaLabel={`grams for ${row.name}`}
                        className="font-medium"
                      />{" "}
                      <LinkComponent href={row.href} className="hover:text-primary hover:underline">
                        {row.name}
                      </LinkComponent>
                    </span>
                    <button
                      type="button"
                      aria-label={`Remove ${row.name}`}
                      onClick={() => void edit.removeItem(row.ingredientId)}
                      className="mt-0.5 shrink-0 rounded-sm p-0.5 text-muted-foreground opacity-0 transition hover:bg-destructive/10 hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
                    >
                      <Trash2 className="size-3.5" />
                    </button>
                  </li>
                ))
              : recipe.items.map((item) => (
                  <li key={item.key} className="flex items-start gap-2">
                    <span aria-hidden className="mt-[0.55rem] size-1.5 shrink-0 rounded-full bg-primary" />
                    <LinkComponent href={item.href} className="text-left hover:text-primary hover:underline">
                      {item.label}
                    </LinkComponent>
                  </li>
                ))}
            {edit ? (
              <li className="col-span-full">
                <AddIngredientRow open={addOpen} onOpenChange={setAddOpen} edit={edit} />
              </li>
            ) : null}
          </ul>

          <h2 className="mt-8 text-center font-serif text-xl font-bold">Directions</h2>
          <InlineTextarea
            value={recipe.directions ?? ""}
            onCommit={edit?.setDirections}
            placeholder={edit ? "Add directions" : "No directions yet."}
            ariaLabel="directions"
            className="mt-3 text-muted-foreground"
          />
        </div>
      </div>

      <aside className="hidden w-80 shrink-0 border-l border-border p-6 lg:block">
        <div className="lg:sticky lg:top-6">
          <NutritionFacts data={recipe.nutrition} />
        </div>
      </aside>

      <NutritionFactsFab data={recipe.nutrition} />

      {edit ? (
        <>
          <DeleteRecipeDialog
            open={deleteOpen}
            onOpenChange={setDeleteOpen}
            name={recipe.name || "this recipe"}
            onConfirm={edit.remove}
          />
          <ShortcutSheet open={helpOpen} onOpenChange={setHelpOpen} />
        </>
      ) : null}
    </div>
  );
}

function queryClick(selector: string) {
  document.querySelector<HTMLElement>(selector)?.click();
}
function queryFocus(selector: string) {
  document.querySelector<HTMLElement>(selector)?.focus();
}

/** The ghost "+ add ingredient" row → inline type-to-search → pick attaches with a default amount. */
function AddIngredientRow({
  open,
  onOpenChange,
  edit,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  edit: RecipeEditAdapter;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<IngredientSearchItem[]>([]);
  const [searching, setSearching] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) inputRef.current?.focus();
    else {
      setQuery("");
      setResults([]);
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    setSearching(true);
    const t = setTimeout(async () => {
      try {
        const r = await edit.search(query);
        if (alive) setResults(r);
      } finally {
        if (alive) setSearching(false);
      }
    }, 250);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [query, open, edit]);

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => onOpenChange(true)}
        className="flex w-full items-center gap-2 rounded-sm py-1 text-left text-muted-foreground transition hover:text-primary"
      >
        <Plus className="size-4" />
        Add ingredient
      </button>
    );
  }

  return (
    <div className="relative">
      <input
        ref={inputRef}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search ingredients…"
        aria-label="Search ingredients to add"
        className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-primary"
        onKeyDown={(e) => {
          if (e.key === "Escape") onOpenChange(false);
        }}
      />
      {query ? (
        <ul className="absolute z-10 mt-1 max-h-64 w-full overflow-auto rounded-md border border-border bg-popover p-1 shadow-md">
          {searching ? (
            <li className="px-2 py-1.5 text-sm text-muted-foreground">Searching…</li>
          ) : results.length === 0 ? (
            <li className="px-2 py-1.5 text-sm text-muted-foreground">No matches.</li>
          ) : (
            results.map((r) => (
              <li key={r.id}>
                <button
                  type="button"
                  className="w-full rounded-sm px-2 py-1.5 text-left text-sm hover:bg-accent"
                  onClick={async () => {
                    await edit.addItem(r);
                    setQuery("");
                    inputRef.current?.focus();
                  }}
                >
                  {r.name}
                </button>
              </li>
            ))
          )}
        </ul>
      ) : null}
    </div>
  );
}

/** Page ⋯ menu (owner): delete, discard (drafts only), shortcuts. */
function RecipeOverflowMenu({
  isDraft,
  onDelete,
  onDiscard,
  onHelp,
}: {
  isDraft?: boolean;
  onDelete: () => void;
  onDiscard?: () => Promise<void>;
  onHelp: () => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label="Recipe actions"
        className="rounded-md p-1.5 text-muted-foreground transition hover:bg-accent hover:text-foreground"
      >
        <MoreHorizontal className="size-4" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {isDraft && onDiscard ? (
          <DropdownMenuItem onClick={() => void onDiscard()}>Discard draft</DropdownMenuItem>
        ) : null}
        <DropdownMenuItem onClick={onHelp}>Keyboard shortcuts</DropdownMenuItem>
        <DropdownMenuItem
          onClick={onDelete}
          className="text-destructive data-highlighted:bg-destructive/10 data-highlighted:text-destructive"
        >
          Delete recipe…
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function DeleteRecipeDialog({
  open,
  onOpenChange,
  name,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  name: string;
  onConfirm: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete {name}?</DialogTitle>
          <DialogDescription>This can’t be undone.</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose className={buttonClasses({ variant: "outline", size: "sm" })}>Cancel</DialogClose>
          <button
            type="button"
            disabled={busy}
            className={buttonClasses({ variant: "destructive", size: "sm" })}
            onClick={async () => {
              setBusy(true);
              try {
                await onConfirm();
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? "Deleting…" : "Delete"}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ShortcutSheet({ open, onOpenChange }: { open: boolean; onOpenChange: (v: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
        </DialogHeader>
        <ul className="divide-y divide-border">
          {DETAIL_SHORTCUTS.map((s) => (
            <li key={s.keys} className="flex items-center justify-between py-2 text-sm">
              <span className="text-muted-foreground">{s.label}</span>
              <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-xs">
                {s.keys}
              </kbd>
            </li>
          ))}
        </ul>
      </DialogContent>
    </Dialog>
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
            editHref={ingredient.canEdit ? `/ingredients/${ingredient.id}/edit` : undefined}
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
