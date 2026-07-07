// Shared catalog-list vocabulary for both shells: the sort options shown in the list header, and the
// page size for infinite scroll. The data layer (vegify-core `Sort`/`Page`) mirrors these values.

export type Sort = "newest" | "oldest" | "name_asc" | "name_desc"

export const SORT_OPTIONS: readonly { value: Sort; label: string }[] = [
  { value: "newest", label: "Newest" },
  { value: "oldest", label: "Oldest" },
  { value: "name_asc", label: "A → Z" },
  { value: "name_desc", label: "Z → A" }
]

/** Coerce an unknown URL/search value to a valid Sort, defaulting to newest. */
export const parseSort = (v: unknown): Sort =>
  SORT_OPTIONS.some((o) => o.value === v) ? (v as Sort) : "newest"

/** Catalog page size for infinite scroll — one fetch per scroll into view. */
export const PAGE_SIZE = 24
