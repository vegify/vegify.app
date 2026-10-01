import { createIsomorphicFn } from "@tanstack/react-start"
import {
  type NutritionFactsData,
  topMicronutrients
} from "@vegify/ui/nutrition-facts"

// Per-page <head> for the public pages. Every public route builds its head through pageHead(), so each
// page carries the same set — its own title, a description, a canonical URL, Open Graph and Twitter
// tags, plus robots and JSON-LD where the page has them — and none of them hardcodes the domain.

const SITE_NAME = "Vegify"

/** The public site origin for absolute URLs (canonical, og:url, JSON-LD). Server: VEGIFY_PUBLIC_URL,
 *  which the CDK sets on deploys (behind CloudFront the request host is the function URL, never the
 *  site); unset, the request's own origin, which is right for local serving. Client: the page's. */
export const siteOrigin = createIsomorphicFn()
  .server(async () => {
    const { publicUrl } = await import("@vegify/config")
    const configured = publicUrl()
    if (configured) return configured.replace(/\/+$/, "")
    const { getRequestUrl } = await import("@tanstack/react-start/server")
    return getRequestUrl().origin
  })
  .client(() => window.location.origin)

export type PageHeadInput = {
  origin: string
  /** Site-relative canonical path, e.g. "/john/biga". */
  path: string
  /** The page's own title; " | Vegify" is appended. */
  title: string
  description: string
  type?: "website" | "article" | "profile"
  /** Absolute image URL for link previews; defaults to the app icon. */
  image?: string | null
  /** Keep the page out of search indexes (unlisted, deleted, not found). */
  noindex?: boolean
  jsonLd?: Record<string, unknown>
  /** Extra Open Graph properties, e.g. article:published_time. */
  meta?: { property: string; content: string }[]
}

export function pageHead(p: PageHeadInput) {
  const url = `${p.origin}${p.path}`
  const title = `${p.title} | ${SITE_NAME}`
  const { description } = p // authored, or generated through composeDescription(), which keeps it short
  const image = p.image ?? `${p.origin}/logo512.png`
  return {
    meta: [
      { title },
      { name: "description", content: description },
      ...(p.noindex ? [{ name: "robots", content: "noindex" }] : []),
      { property: "og:type", content: p.type ?? "website" },
      { property: "og:site_name", content: SITE_NAME },
      { property: "og:title", content: title },
      { property: "og:description", content: description },
      { property: "og:url", content: url },
      { property: "og:image", content: image },
      ...(p.meta ?? []),
      {
        name: "twitter:card",
        content: p.image ? "summary_large_image" : "summary"
      },
      { name: "twitter:title", content: title },
      { name: "twitter:description", content: description },
      { name: "twitter:image", content: image },
      ...(p.jsonLd ? [{ "script:ld+json": p.jsonLd }] : [])
    ],
    links: [{ rel: "canonical", href: url }]
  }
}

const DESCRIPTION_MAX = 160 // search results cut descriptions off around here

/** Cut an over-long description at a word boundary, with an ellipsis. */
export function clampDescription(text: string, max = DESCRIPTION_MAX): string {
  const t = text.replace(/\s+/g, " ").trim()
  if (t.length <= max) return t
  const cut = t.slice(0, max - 1)
  const space = cut.lastIndexOf(" ")
  return `${(space > 0 ? cut.slice(0, space) : cut).replace(/[\s,;:]+$/, "")}…`
}

/** Join sentences in priority order, skipping any that would run past the limit, so a description
 *  ends on a whole sentence. Only a first sentence that is too long on its own gets cut. */
export function composeDescription(
  sentences: (string | null | undefined | false)[],
  max = DESCRIPTION_MAX
): string {
  let out = ""
  for (const s of sentences) {
    const t = s ? s.replace(/\s+/g, " ").trim() : ""
    if (!t) continue
    const next = out ? `${out} ${t}` : t
    if (next.length <= max) out = next
    else if (!out) return clampDescription(t, max)
  }
  return out
}

/** Close a fragment as a sentence ("Silky and rich" → "Silky and rich."). */
export const sentence = (s: string) => {
  const t = s.trim()
  return /[.!?…]$/.test(t) ? t : `${t}.`
}

/** Free-text directions as steps: one per line, list markers ("1.", "2)", "-", "•") dropped. */
export const directionSteps = (directions: string | null | undefined) =>
  (directions ?? "")
    .split(/\n+/)
    .map((line) => line.replace(/^\s*(?:\d+[.)]|[-*•])\s+/, "").trim())
    .filter(Boolean)

/** "1 recipe", "3 ingredients". */
export const counted = (n: number, noun: string) =>
  `${n} ${noun}${n === 1 ? "" : "s"}`

const LIST = new Intl.ListFormat("en", { type: "conjunction" })

/** "a", "a and b", "a, b, and c". */
export const listOf = (items: string[]) => LIST.format(items)

/** One decimal at most, no trailing ".0" — the Nutrition Facts panel's rounding. */
export const round1 = (n: number) => String(Math.round(n * 10) / 10)

// "Vitamin B12" reads as "vitamin B12" mid-sentence, "Iron" as "iron".
const inProse = (label: string) =>
  label.startsWith("Vitamin ")
    ? `vitamin ${label.slice("Vitamin ".length)}`
    : label.toLowerCase()

// Nothing edible is denser than fat, about 900 kcal per 100 g (the margin covers label rounding on
// oils). Past that, a recipe's weights are off — a "1 g" that meant one crust — and its per-gram
// numbers are wrong everywhere, so they aren't worth repeating to a search engine.
const MAX_KCAL_PER_100G = 1000

/** False when the energy density is physically impossible, i.e. the weights behind it are wrong. */
export function plausibleNutrition(n: NutritionFactsData): boolean {
  if (n.caloriesPerServing == null) return true
  const per100g = n.serving?.grams
    ? (n.caloriesPerServing * 100) / n.serving.grams
    : n.caloriesPerServing
  return per100g <= MAX_KCAL_PER_100G
}

/** "Per 100 g: 654 kcal, richest in copper (176% DV), manganese (149% DV), and magnesium (38% DV)."
 *  Built from the same numbers as the page's Nutrition Facts panel; null when there's nothing to say
 *  or the numbers can't be right. */
export function nutritionSentence(n: NutritionFactsData): string | null {
  if (!plausibleNutrition(n)) return null
  const basis = n.serving?.grams
    ? `Per serving (${round1(n.serving.grams)} g)`
    : "Per 100 g"
  const kcal =
    n.caloriesPerServing != null
      ? `${Math.round(n.caloriesPerServing)} kcal`
      : null
  const top = topMicronutrients(n).map(
    (m) => `${inProse(m.label)} (${m.pct}% DV)`
  )
  const richest = top.length ? `richest in ${listOf(top)}` : null
  const body = [kcal, richest].filter(Boolean).join(", ")
  return body ? `${basis}: ${body}.` : null
}
