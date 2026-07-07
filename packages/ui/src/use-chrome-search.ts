"use client"

import { useEffect, useState } from "react"

/**
 * Chrome search state shared by BOTH app shells (web + desktop).
 *
 * Owns the search-box value plus the one piece of framework-AGNOSTIC behavior that used to sit
 * duplicated in each shell's root: dismiss the search overlay on navigation. The chrome renders the
 * SearchOverlay whenever the query is non-empty, so without this a result/nav click changes the route
 * underneath a still-open overlay and appears to do nothing. Each shell passes its own router's
 * pathname and keeps owning its <Outlet>/children + its own data-bound SearchOverlay — only the
 * behavior is shared, never the framework plumbing (the benchmark's variable).
 */
export function useChromeSearch(pathname: string) {
  const [search, setSearch] = useState("")
  // Clear on every route change so the overlay reveals the destination.
  useEffect(() => {
    setSearch("")
  }, [pathname])
  return { search, setSearch, query: search.trim() }
}
