import { useEffect } from "react"
import {
  queryOptions,
  useQueryClient,
  useSuspenseQuery
} from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import { createServerFn } from "@tanstack/react-start"
import {
  addDays,
  type DayLogAdapter,
  DayView,
  type DayVM,
  todayLocal
} from "@vegify/ui/day"

import { LinkAdapter } from "../link"

// The diary is authed-only (a PRIVATE per-user food log). `/diary` is a new top-level route not in the
// public path policy, so __root's beforeLoad redirects a logged-out visitor to /login before this
// renders; every /api/log/* call also hard-401s server-side. Data flows via server-fns → ./content →
// the Axum backend (the session cookie is forwarded as a Bearer), exactly like the recipe routes.

const getDay = createServerFn({ method: "GET" })
  .validator((p: { date: string }) => p)
  .handler(async ({ data }): Promise<DayVM> => {
    const { getLogDay, getLogRecents } = await import("../content")
    const [day, recents] = await Promise.all([
      getLogDay(data.date),
      getLogRecents(20)
    ])
    return {
      date: day.date,
      entries: day.entries.map((e) => ({
        id: e.id,
        ingredientId: e.ingredientId,
        name: e.name,
        href: e.recipeId
          ? `/recipes/${e.recipeId}`
          : `/ingredients/${e.ingredientId}`,
        grams: e.amount.grams ?? 0,
        calories: e.calories
      })),
      calories: day.calories,
      totals: day.totals.map((t) => ({
        name: t.name,
        amount: t.amount ?? 0,
        unit: t.unit
      })),
      targets: day.targets.map((t) => ({
        name: t.name,
        amount: t.amount ?? 0,
        unit: t.unit,
        basis: t.basis,
        veganAdjusted: t.veganAdjusted,
        supplementCovered: t.supplementCovered,
        note: t.note ?? null
      })),
      recents: recents.map((r) => ({
        ingredientId: r.ingredientId,
        name: r.name,
        lastGrams: r.lastGrams
      })),
      supplements: {
        b12: day.supplements.b12 ?? false,
        vitD: day.supplements.vitD ?? false,
        algaeOil: day.supplements.algaeOil ?? false
      }
    }
  })

const saveEntry = createServerFn({ method: "POST" })
  .validator(
    (input: {
      id?: string
      ingredientId: string
      date: string
      grams: number
      unit?: string | null
    }) => input
  )
  .handler(async ({ data }) => {
    const { saveLogEntry } = await import("../content")
    return saveLogEntry({
      id: data.id ?? null,
      ingredientId: data.ingredientId,
      date: data.date,
      grams: data.grams,
      slot: null,
      unit: data.unit ?? null,
      loggedAt: null
    })
  })

const deleteEntry = createServerFn({ method: "POST" })
  .validator((p: { id: string }) => p)
  .handler(async ({ data }) => {
    const { deleteLogEntry } = await import("../content")
    await deleteLogEntry(data.id)
  })

// Copy every entry from `from` onto `to` — each re-logs at TODAY's values (a fresh snapshot).
const copyDay = createServerFn({ method: "POST" })
  .validator((p: { from: string; to: string }) => p)
  .handler(async ({ data }) => {
    const { getLogDay, saveLogEntry } = await import("../content")
    const src = await getLogDay(data.from)
    for (const e of src.entries) {
      await saveLogEntry({
        id: null,
        ingredientId: e.ingredientId,
        date: data.to,
        grams: e.amount.grams ?? 0,
        slot: e.slot,
        unit: e.amount.unit,
        loggedAt: null
      })
    }
  })

const searchFn = createServerFn({ method: "GET" })
  .validator((p: { q: string }) => p)
  .handler(async ({ data }) => {
    const { searchIngredients } = await import("../content")
    return searchIngredients(data.q)
  })

const saveSupplements = createServerFn({ method: "POST" })
  .validator(
    (p: { date: string; b12: boolean; vitD: boolean; algaeOil: boolean }) => p
  )
  .handler(async ({ data }) => {
    const { saveDaySupplements } = await import("../content")
    await saveDaySupplements(data)
  })

const dayQuery = (date: string) =>
  queryOptions({
    queryKey: ["diary", date],
    queryFn: () => getDay({ data: { date } })
  })

export const Route = createFileRoute("/diary")({
  validateSearch: (s: { date?: string }): { date?: string } => ({
    date: typeof s.date === "string" && s.date ? s.date : undefined
  }),
  loaderDeps: ({ search }) => ({ date: search.date }),
  loader: ({ context, deps }) =>
    deps.date
      ? context.queryClient.ensureQueryData(dayQuery(deps.date))
      : undefined,
  component: DiaryPage
})

function DiaryPage() {
  const { date } = Route.useSearch()
  const navigate = Route.useNavigate()
  // The diary date is the viewer's LOCAL calendar day. SSR can't know the tz, so when no ?date= is
  // present we set it client-side to today (replace, so Back doesn't bounce here). The loader then
  // runs for the resolved date, so the render below never suspends unhandled.
  useEffect(() => {
    if (!date) navigate({ search: { date: todayLocal() }, replace: true })
  }, [date, navigate])
  if (!date) return null
  return <DiaryDay date={date} />
}

function DiaryDay({ date }: { date: string }) {
  const qc = useQueryClient()
  const navigate = Route.useNavigate()
  const { data: day } = useSuspenseQuery(dayQuery(date))
  const refresh = () => qc.invalidateQueries({ queryKey: ["diary", date] })

  const log: DayLogAdapter = {
    addEntry: async ({ ingredientId, grams, unit }) => {
      await saveEntry({ data: { ingredientId, date, grams, unit } })
      await refresh()
    },
    setEntryAmount: async (id, grams) => {
      const entry = day.entries.find((e) => e.id === id)
      if (!entry) return
      await saveEntry({
        data: { id, ingredientId: entry.ingredientId, date, grams }
      })
      await refresh()
    },
    removeEntry: async (id) => {
      await deleteEntry({ data: { id } })
      await refresh()
    },
    search: (q) => searchFn({ data: { q } }),
    copyYesterday: async () => {
      await copyDay({ data: { from: addDays(date, -1), to: date } })
      await refresh()
    },
    setSupplements: async (next) => {
      await saveSupplements({ data: { date, ...next } })
      await refresh()
    }
  }

  return (
    <DayView
      day={day}
      LinkComponent={LinkAdapter}
      log={log}
      onNavigateDate={(d) => navigate({ search: { date: d } })}
    />
  )
}
