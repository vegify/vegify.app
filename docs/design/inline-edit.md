# Inline editing — the Linear-like edit flow

Goal (John, 2026-07-01): "a more Linear-like experience of just clicking the field and starting to type, or being able to quickly change the amount like it was a ticket's status" — the interface gets perfect **before** profiles/targets expand the data shapes. This spec covers the interaction model; it is the contract the build follows.

## Current state (what dissolves)

Editing is form-page based: detail screens are read-only (`RecipeDetailVM`/`IngredientDetailVM`, `canEdit` only toggles an Edit link), `/recipes/<id>/edit` + `/ingredients/<id>/edit` render `RecipeForm`/`IngredientForm`, and saves are whole-object (`saveRecipe(RecipeFormInput)` → both shells, then refetch). Delete lives on the edit form. The clunk: every tweak is navigate → form → find field → save → navigate back.

## Interaction model

**The detail page IS the editor when you own the thing.** With `canEdit`, fields render exactly as they do read-only, plus a subtle hover affordance (cursor + faint underline/box on hover — discoverable, not noisy). Logged-out/non-owner render is unchanged, pixel-identical to today.

Per element on the recipe detail:

- **Name (h1), subtitle**: click → becomes an input in place (same typography — no layout shift), caret lands where you clicked, current value retained (not select-all). Type; **Enter or blur commits, Esc reverts**. Empty name reverts (name is required).
- **Directions**: click → in-place textarea (auto-growing, same type styles). **Cmd/Ctrl+Enter or blur commits, Esc reverts** (plain Enter = newline).
- **Visibility**: a small pill (public/unlisted/private) → click opens a Base-UI select popover with type-to-filter — this one literally is Linear's status pattern.
- **Item amounts (the marquee interaction)**: the amount+unit chip on each ingredient row is the "ticket status" — click and it's instantly editable: digits pre-selected so typing replaces, **↑/↓ steps** (1 below 20, 5 to 100, 25 above; Shift×10), unit is an adjacent mini-segment (g/ml/…, existing units only in v1), Enter commits, Esc reverts, Tab commits + moves to the next row's amount — so retuning a whole recipe is: click, type, Tab, type, Tab.
- **Item add**: a ghost row at the list bottom ("+ add ingredient") → inline type-to-search (the existing ingredient search), Enter attaches with a default amount already selected for typing-over.
- **Item remove**: per-row ⋯ → Remove (with the row's name in the menu item — no dialog for a row; recoverable by re-adding).
- **Serving / batch**: same in-place numeric treatment as amounts.
- **Recipe delete**: page-level ⋯ overflow → "Delete recipe…" → confirm dialog (destructive, keeps the one dialog we actually need).

**Commit semantics**: per-field, immediate, optimistic. The UI updates on commit; the shell composes current edit-state + the one change into the existing whole-object `saveRecipe` and refetches; on failure the field reverts with an inline error tick. Concurrency stays LWW like the sync model. No draft mode, no Save button, no dirty-state banner. (Undo stack: out of scope v1; error-revert only.)

**Keyboard (locked 2026-07-01: full Linear-grade in v1)**: the field contracts above (Enter/Esc/blur/Tab), plus single-key shortcuts on detail pages when you own the thing — `e` edit name, `a` add ingredient (opens the ghost row's search), `v` visibility menu, `⌘⌫` delete (confirm dialog), `?` shortcut sheet. Shortcuts suspend while any field is editing; the chrome search keeps `/`. One shared hook owns the map so shells can't drift.

**Nested-recipe rows** (a recipe used as an ingredient): amount edits in place like any row; the name still navigates.

## Architecture

- New shared primitives in `@vegify/ui` (Base UI + cva, shadcn-shaped): `InlineText`, `InlineTextarea`, `InlineNumber` (step logic), `InlineSelect` — all controlled, all "render-as-static-until-clicked", all owning the Enter/Esc/blur contract, none knowing about recipes.
- Screens grow an **optional edit adapter** prop: `RecipeDetailView({ recipe, edit? })` where `edit` carries per-field async callbacks (`rename`, `setSubtitle`, `setDirections`, `setVisibility`, `setItemAmount`, `addItem`, `removeItem`, `remove`) + the ingredient search hook. No adapter ⇒ read-only, today's render.
- Shells implement the adapter over the EXISTING surface: fetch edit-data, merge one field, `saveRecipe`, invalidate — web via server fns + TanStack Query, desktop via the IPC bindings (local-first, so commits are ~instant there). **No server/DAL changes in P1.** Per-field PATCH endpoints are a later optimization if the compose round-trip ever hurts.
- Benchmark-fairness/shared-screens: the interaction lives entirely in `@vegify/ui`; each shell only supplies the adapter — one implementation, three platforms.

## Create flow (locked 2026-07-01: create-blank → inline)

"New recipe" creates a blank draft immediately and lands on its detail page with the name field open and selected — the Linear new-issue feel, one editing model everywhere. Draft semantics: the blank is born as "Untitled recipe" (schema requires a name), name opens select-all so typing replaces; while a recipe is still untitled with no items, the page shows a "Discard draft" affordance (and the ⋯ menu always offers Delete). No auto-reaping in v1 — an explicit discard beats surprise deletion; revisit if abandoned blanks actually accumulate.

## Phasing (updated for the locked decisions)

- **P1** (one PR): recipe detail fully inline — name, subtitle, directions, visibility, item amounts+units, item add (ghost row, needed for `a`) and remove, serving — plus page ⋯ delete, the full keyboard map + `?` sheet, and create-blank → inline (the New Recipe entry points stop routing to the form). `/recipes/<id>/edit` still exists but nothing links to it.
- **P2**: ingredient detail gets the same treatment (name, description, price, per-100g numbers); ingredient create-blank.
- **P3**: kill the `/edit` + `/new` routes (redirect → detail).
- **P4 (with targets later)**: the same primitives serve profile editing — the reason this precedes profiles.

## Out of scope v1

Undo/history, multi-field draft buffers, collaborative cursors/presence, custom units, mobile long-press affordances (iOS gets hover-free tap — the affordance shows on first tap, edits on second… to validate on the sim).
