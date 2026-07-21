import { type ComponentType, useEffect, useRef, useState } from "react"
import { ChevronLeft, ChevronRight, Plus, Trash2 } from "lucide-react"

import type { AppShellLinkProps } from "./app-shell"
import { buttonClasses } from "./button"
import { cn } from "./cn"
import { InlineNumber } from "./inline"
import { NutritionFacts, type NutritionFactsData } from "./nutrition-facts"
import { NutritionFactsFab } from "./nutrition-facts-fab"
import type { IngredientSearchItem } from "./recipe-form"

/**
 * THE DAY SCREEN — the food diary, shared by both shells (web + desktop/iOS), purely presentational
 * like every screen in `./screens`. It takes a day view-model + a `LinkComponent` nav port + an
 * optional `log` adapter (a bag of async callbacks); presence of `log` = the signed-in owner (the
 * diary is PRIVATE + authed-only, so the shell only renders this behind auth). Each shell fills the
 * adapter its own way — web with `/api/log/*` server-fns, desktop with local-first IPC + sync.
 *
 * The nutrition totals reuse `NutritionFacts` (the same FDA panel recipes use) — the day's ABSOLUTE
 * per-nutrient totals shown against the 2,000-kcal Daily Values. Personalized vegan-aware targets
 * replace the FDA framing in P1.3; until then %DV is the honest, familiar readout.
 */
export type NavLink = ComponentType<AppShellLinkProps>

/** One logged food as the Day screen renders it. `grams` is editable; `calories` is the entry's
 *  frozen contribution (snapshot × grams/100). `href` links to the food's page; `ingredientId` is
 *  carried so the shell can rebuild the upsert when the amount is edited. */
export type DayEntryVM = {
  id: string
  ingredientId: string
  name: string
  href: string
  grams: number
  calories: number | null
}

/** A recently-logged food, prepended in the add-flow before global search for fast re-logging. */
export type DayRecentVM = {
  ingredientId: string
  name: string
  /** The most recent log's grams, to prefill on re-log. */
  lastGrams: number | null
}

/** One diary day as a view-model (each shell maps the server's `DayLog`, coercing null numbers). */
export type DayVM = {
  /** The 'YYYY-MM-DD' local calendar date this covers. */
  date: string
  entries: DayEntryVM[]
  /** Total calories for the day; null when nothing carries calorie data. */
  calories: number | null
  /** Per-nutrient ABSOLUTE totals for the day (name + amount + unit). */
  totals: { name: string; amount: number; unit: string }[]
  /** The viewer's recents, for the add-flow. */
  recents: DayRecentVM[]
}

/** The write port — present ⇒ the owner can log/edit/delete. Each shell wires these to its transport. */
export type DayLogAdapter = {
  addEntry: (input: {
    ingredientId: string
    grams: number
    unit?: string | null
  }) => Promise<void>
  setEntryAmount: (id: string, grams: number) => Promise<void>
  removeEntry: (id: string) => Promise<void>
  /** Global ingredient/recipe search for the add-flow (reuses /api/content/search). */
  search: (q: string) => Promise<IngredientSearchItem[]>
  /** Re-log yesterday's foods onto this day (each re-snapshots at today's values). Optional. */
  copyYesterday?: () => Promise<void>
}

const pad = (n: number) => String(n).padStart(2, "0")

/** Today as a user-local YYYY-MM-DD — the client picks the calendar day; the server does no tz math. */
export function todayLocal(): string {
  const d = new Date()
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

/** Shift a YYYY-MM-DD by `n` local-calendar days. */
export function addDays(date: string, n: number): string {
  const [y, m, d] = date.split("-").map(Number)
  const dt = new Date(y ?? 1970, (m ?? 1) - 1, (d ?? 1) + n)
  return `${dt.getFullYear()}-${pad(dt.getMonth() + 1)}-${pad(dt.getDate())}`
}

/** Human label for a day: "Today" / "Yesterday" / "Tomorrow", else "Mon, Jul 21". */
export function formatDayLabel(date: string): string {
  const today = todayLocal()
  if (date === today) return "Today"
  if (date === addDays(today, -1)) return "Yesterday"
  if (date === addDays(today, 1)) return "Tomorrow"
  const [y, m, d] = date.split("-").map(Number)
  return new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1).toLocaleDateString(
    undefined,
    {
      weekday: "short",
      month: "short",
      day: "numeric"
    }
  )
}

/** 1-decimal number, trailing .0 stripped (matches the nutrition panel's local `fmt`). */
function num(n: number): string {
  const r = Math.round(n * 10) / 10
  return Number.isInteger(r) ? r.toString() : r.toFixed(1)
}

function DayNav({
  date,
  onNavigate,
  onCopyYesterday
}: {
  date: string
  onNavigate: (date: string) => void
  onCopyYesterday?: () => Promise<void>
}) {
  const [copying, setCopying] = useState(false)
  const isToday = date === todayLocal()
  return (
    <div className="mb-6 flex flex-wrap items-center justify-between gap-3">
      <div className="flex items-center gap-1">
        <button
          type="button"
          aria-label="Previous day"
          onClick={() => onNavigate(addDays(date, -1))}
          className={buttonClasses({ variant: "ghost", size: "icon-sm" })}
        >
          <ChevronLeft className="size-4" />
        </button>
        <h1 className="min-w-[8rem] text-center font-bold font-serif text-xl">
          {formatDayLabel(date)}
        </h1>
        <button
          type="button"
          aria-label="Next day"
          onClick={() => onNavigate(addDays(date, 1))}
          className={buttonClasses({ variant: "ghost", size: "icon-sm" })}
        >
          <ChevronRight className="size-4" />
        </button>
      </div>
      <div className="flex items-center gap-2">
        {isToday ? null : (
          <button
            type="button"
            onClick={() => onNavigate(todayLocal())}
            className={buttonClasses({ variant: "outline", size: "sm" })}
          >
            Today
          </button>
        )}
        {onCopyYesterday ? (
          <button
            type="button"
            disabled={copying}
            onClick={async () => {
              setCopying(true)
              try {
                await onCopyYesterday()
              } finally {
                setCopying(false)
              }
            }}
            className={buttonClasses({ variant: "outline", size: "sm" })}
          >
            {copying ? "Copying…" : "Copy yesterday"}
          </button>
        ) : null}
      </div>
    </div>
  )
}

function EntryRow({
  entry,
  LinkComponent,
  log
}: {
  entry: DayEntryVM
  LinkComponent: NavLink
  log?: DayLogAdapter
}) {
  return (
    <li className="flex items-center gap-3 border-border border-b py-2">
      <LinkComponent
        href={entry.href}
        className="min-w-0 flex-1 truncate font-medium hover:text-primary hover:underline"
      >
        {entry.name}
      </LinkComponent>
      {log ? (
        <InlineNumber
          value={entry.grams}
          suffix="g"
          min={0}
          ariaLabel={`${entry.name} grams`}
          onCommit={(g) => log.setEntryAmount(entry.id, g)}
        />
      ) : (
        <span className="text-muted-foreground text-sm tabular-nums">
          {num(entry.grams)} g
        </span>
      )}
      <span className="w-16 shrink-0 text-right text-muted-foreground text-sm tabular-nums">
        {entry.calories == null ? "—" : `${Math.round(entry.calories)} cal`}
      </span>
      {log ? (
        <button
          type="button"
          aria-label={`Remove ${entry.name}`}
          onClick={() => log.removeEntry(entry.id)}
          className="shrink-0 rounded p-1 text-muted-foreground transition hover:text-destructive"
        >
          <Trash2 className="size-4" />
        </button>
      ) : null}
    </li>
  )
}

/** The "+ Add food" affordance: empty query shows RECENTS; typing runs the global search (250ms
 *  debounce). Picking either logs it with a sensible default gram amount (adjust after via the row). */
function AddFoodRow({
  log,
  recents
}: {
  log: DayLogAdapter
  recents: DayRecentVM[]
}) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")
  const [results, setResults] = useState<IngredientSearchItem[]>([])
  const [searching, setSearching] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (open) inputRef.current?.focus()
    else {
      setQuery("")
      setResults([])
    }
  }, [open])

  useEffect(() => {
    if (!open || !query.trim()) {
      setResults([])
      return
    }
    let alive = true
    setSearching(true)
    const t = setTimeout(async () => {
      try {
        const r = await log.search(query)
        if (alive) setResults(r)
      } finally {
        if (alive) setSearching(false)
      }
    }, 250)
    return () => {
      alive = false
      clearTimeout(t)
    }
  }, [query, open, log])

  const pick = async (
    ingredientId: string,
    grams: number,
    unit?: string | null
  ) => {
    await log.addEntry({ ingredientId, grams, unit: unit ?? null })
    setQuery("")
    inputRef.current?.focus()
  }

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="flex w-full items-center gap-2 rounded-sm py-2 text-left text-muted-foreground transition hover:text-primary"
      >
        <Plus className="size-4" />
        Add food
      </button>
    )
  }

  const showRecents = !query.trim()
  return (
    <div className="relative py-1">
      <input
        ref={inputRef}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search foods…"
        aria-label="Search foods to log"
        className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-primary"
        onKeyDown={(e) => {
          if (e.key === "Escape") setOpen(false)
        }}
      />
      <ul className="absolute z-10 mt-1 max-h-72 w-full overflow-auto rounded-md border border-border bg-popover p-1 shadow-md">
        {showRecents ? (
          recents.length === 0 ? (
            <li className="px-2 py-1.5 text-muted-foreground text-sm">
              Type to search foods.
            </li>
          ) : (
            <>
              <li className="px-2 pt-1 pb-0.5 font-medium text-muted-foreground text-xs">
                Recent
              </li>
              {recents.map((r) => (
                <li key={r.ingredientId}>
                  <button
                    type="button"
                    className="w-full rounded-sm px-2 py-1.5 text-left text-sm hover:bg-accent"
                    onClick={() => pick(r.ingredientId, r.lastGrams ?? 100)}
                  >
                    {r.name}
                  </button>
                </li>
              ))}
            </>
          )
        ) : searching ? (
          <li className="px-2 py-1.5 text-muted-foreground text-sm">
            Searching…
          </li>
        ) : results.length === 0 ? (
          <li className="px-2 py-1.5 text-muted-foreground text-sm">
            No matches.
          </li>
        ) : (
          results.map((r) => (
            <li key={r.id}>
              <button
                type="button"
                className="w-full rounded-sm px-2 py-1.5 text-left text-sm hover:bg-accent"
                onClick={() => pick(r.id, r.servingGrams ?? 100)}
              >
                {r.name}
              </button>
            </li>
          ))
        )}
      </ul>
    </div>
  )
}

export function DayView({
  day,
  LinkComponent,
  log,
  onNavigateDate
}: {
  day: DayVM
  LinkComponent: NavLink
  /** Present ⇒ signed-in owner (the shell renders this screen only behind auth). */
  log?: DayLogAdapter
  onNavigateDate: (date: string) => void
}) {
  const nutrition: NutritionFactsData = {
    heading: formatDayLabel(day.date),
    caloriesPerServing: day.calories,
    // No `serving` ⇒ scale = 1, so the per-100g readings render as the day's ABSOLUTE totals.
    readings: day.totals.map((t) => ({
      name: t.name,
      amountPer100g: t.amount,
      unit: t.unit
    }))
  }

  return (
    <div className="flex w-full">
      <div className="mx-auto w-full max-w-2xl flex-1 px-4 py-6 lg:px-8">
        <DayNav
          date={day.date}
          onNavigate={onNavigateDate}
          onCopyYesterday={log?.copyYesterday}
        />

        <ul className={cn("space-y-0", day.entries.length === 0 && "hidden")}>
          {day.entries.map((entry) => (
            <EntryRow
              key={entry.id}
              entry={entry}
              LinkComponent={LinkComponent}
              log={log}
            />
          ))}
        </ul>

        {day.entries.length === 0 ? (
          <p className="rounded-lg border border-border border-dashed px-4 py-8 text-center text-muted-foreground text-sm">
            Nothing logged for {formatDayLabel(day.date).toLowerCase()} yet.
            {log ? " Add your first food below." : ""}
          </p>
        ) : null}

        {log ? (
          <div className="mt-2">
            <AddFoodRow log={log} recents={day.recents} />
          </div>
        ) : null}
      </div>

      <aside className="hidden w-80 shrink-0 border-border border-l p-6 lg:block">
        <div className="lg:sticky lg:top-6">
          <NutritionFacts data={nutrition} />
        </div>
      </aside>

      <NutritionFactsFab data={nutrition} />
    </div>
  )
}
