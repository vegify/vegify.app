import { createRouter as createTanStackRouter } from '@tanstack/react-router'
import { QueryClient } from '@tanstack/react-query'
import { setupRouterSsrQueryIntegration } from '@tanstack/react-router-ssr-query'
import { routeTree } from './routeTree.gen'

export function getRouter() {
  // One QueryClient per call — fresh per request on the server (no cross-request cache bleed), one on
  // the client. Content reads are server-fetched in loaders via ensureQueryData, dehydrated into the
  // SSR stream by the ssr-query integration, then hydrated + reused on the client via useSuspenseQuery.
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        // A short staleTime keeps client nav instant while a write's invalidateQueries still forces a
        // refetch. The old transient 429 (EFS single-writer) is gone (standing Axum), so retry is modest.
        staleTime: 30_000,
        retry: 1,
      },
    },
  })

  const router = createTanStackRouter({
    routeTree,
    context: { queryClient },
    scrollRestoration: true,
    defaultPreload: 'intent',
    // Let TanStack Query own read caching; don't double-cache loader data in the router.
    defaultPreloadStaleTime: 0,
  })

  setupRouterSsrQueryIntegration({ router, queryClient })

  return router
}

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof getRouter>
  }
}
