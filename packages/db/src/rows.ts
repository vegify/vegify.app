/** First row of an insert/select that must produce exactly one (drizzle types
 * `.returning()` as an array); throws loudly instead of letting `undefined`
 * propagate into foreign keys. */
export const one = <T>(rows: T[]): T => {
  const [row] = rows
  if (!row) throw new Error("expected a row")
  return row
}
