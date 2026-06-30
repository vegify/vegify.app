import { Link } from '@tanstack/react-router'
import type { AppShellLinkProps } from '@vegify/ui/app-shell'

/**
 * web's navigation port. The shared shell + screens (@vegify/ui) navigate through an
 * href-based `LinkComponent`; here that maps to a TanStack Router <Link> (client-side, prefetched).
 * The desktop supplies its own adapter that maps the same hrefs to its in-process view state.
 */
export function LinkAdapter({ href, ...props }: AppShellLinkProps) {
  return <Link to={href} {...props} />
}
