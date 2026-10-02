import { Link } from "@tanstack/react-router"
import type { AppShellLinkProps } from "@vegify/ui/app-shell"

/**
 * web's navigation port. The shared shell + screens (@vegify/ui) navigate through an
 * href-based `LinkComponent`; here that maps to a TanStack Router <Link> (client-side, prefetched).
 * The desktop supplies its own adapter that maps the same hrefs to its in-process view state.
 * The full href goes to <Link href>, which the router parses into path + search (and prefers over
 * `to`), so an href may carry a query ("/ingredients?letter=p"). Inside `to`, the "?" would stay in
 * the path and match no route; `to` carries just the path, for the types.
 */
export function LinkAdapter({ href, exact, ...props }: AppShellLinkProps) {
  return (
    <Link
      to={href.split("?", 1)[0]}
      href={href}
      activeOptions={exact ? { exact: true } : undefined}
      {...props}
    />
  )
}
