// UGC visibility policy (the app is public-default sharing — see the ugc-public-default note).
// One source of truth for the TS side; the desktop Rust DAL mirrors the same rules.

export type Visibility = "public" | "private" | "unlisted"

type Id = string | null | undefined

/** Appears in public lists + search: public content, or your own (any visibility). */
export const isListed = (
  visibility: string,
  ownerId: Id,
  viewerId: Id
): boolean => visibility === "public" || (!!ownerId && ownerId === viewerId)

/** Readable by direct id/link: anything not private, or your own. */
export const canView = (
  visibility: string,
  ownerId: Id,
  viewerId: Id
): boolean => visibility !== "private" || (!!ownerId && ownerId === viewerId)

/** Edit/delete + the edit-load gate: owner only. */
export const isOwner = (ownerId: Id, viewerId: Id): boolean =>
  !!ownerId && ownerId === viewerId
