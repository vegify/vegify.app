import { createServerFn } from "@tanstack/react-start"
import type { BrandedAdapter } from "@vegify/ui/branded"

// The web's branded-food port (P2.1/P2.2) — one adapter shared by the Day add-flow and the recipe
// composer. Every call goes through a server-fn rather than straight from the browser, exactly like
// the rest of ./content: the session cookie stays the only credential in play and the API origin is
// never contacted from the client.
//
// The two reads are public and IP-budgeted (60 lookups/hour — the bucket exists to protect the
// OUTBOUND USDA quota, so the UI asks for branded results on an explicit action, never per keystroke).
// `promote` is authed server-side; it is the only branded route that writes the communal catalog.

const searchFn = createServerFn({ method: "GET" })
  .validator((p: { q: string }) => p)
  .handler(async ({ data }) => {
    const { searchBranded } = await import("./content")
    return searchBranded(data.q)
  })

const barcodeFn = createServerFn({ method: "GET" })
  .validator((p: { gtin: string }) => p)
  .handler(async ({ data }) => {
    const { lookupBarcode } = await import("./content")
    return lookupBarcode(data.gtin)
  })

const promoteFn = createServerFn({ method: "POST" })
  .validator((p: { source: "usda-branded" | "off"; externalId: string }) => p)
  .handler(async ({ data }) => {
    const { promoteBranded } = await import("./content")
    return promoteBranded(data.source, data.externalId)
  })

// `scanNative` is deliberately absent on web: barcode entry here is the manual field plus the
// feature-detected photo scan inside BarcodeEntry (P2.3 rules out getUserMedia scanning off-device).
// The native scanner port is what iOS fills once P2.3's native half is deploy-verified.
export const webBrandedAdapter: BrandedAdapter = {
  search: (q) => searchFn({ data: { q } }),
  barcode: (gtin) => barcodeFn({ data: { gtin } }),
  promote: (food) =>
    promoteFn({ data: { source: food.source, externalId: food.externalId } })
}
