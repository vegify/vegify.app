import {
  HeadContent,
  Scripts,
  createRootRouteWithContext,
  redirect,
  useRouter,
  useRouterState,
} from '@tanstack/react-router'
import { useEffect } from 'react'
import { useQueryClient, type QueryClient } from '@tanstack/react-query'
import { TanStackRouterDevtoolsPanel } from '@tanstack/react-router-devtools'
import { TanStackDevtools } from '@tanstack/react-devtools'
import { AppShell, themeScript, useChromeSearch } from '@vegify/ui'

import { LinkAdapter } from '../link'
import { SearchOverlay } from '../search'
import { fetchUser, logoutFn } from '../auth'
import { initClientLogging } from '../client-log'
import appCss from '../styles.css?url'
import faviconUrl from '../favicon.ico?url'

// Accounts are required: every page is gated except these. Login/signup render bare (no chrome).
const PUBLIC_PATHS = new Set(['/login', '/signup', '/forgot', '/reset'])

export const Route = createRootRouteWithContext<{ queryClient: QueryClient }>()({
  head: () => ({
    meta: [
      {
        charSet: 'utf-8',
      },
      {
        name: 'viewport',
        content: 'width=device-width, initial-scale=1',
      },
      {
        title: 'Vegify',
      },
    ],
    links: [
      {
        rel: 'stylesheet',
        href: appCss,
      },
      {
        rel: 'icon',
        type: 'image/x-icon',
        href: faviconUrl,
      },
    ],
  }),
  beforeLoad: async ({ location }) => {
    // The auth gate is a plain per-navigation fetch (NOT a TanStack Query): caching the user in the
    // query cache would risk a stale-login gate. Content reads go through Query; this stays direct.
    const user = await fetchUser()
    const isPublic = PUBLIC_PATHS.has(location.pathname)
    if (!user && !isPublic) throw redirect({ to: '/login' })
    if (user && isPublic) throw redirect({ to: '/' })
    return { user }
  },
  errorComponent: RootErrorBoundary,
  shellComponent: RootDocument,
})

// Graceful catch-all: a loader that fails after retries (or any render error) shows this instead of a
// white screen. Retry re-runs the route's loaders — usually enough for a transient throttle.
function RootErrorBoundary() {
  const router = useRouter()
  return (
    <div className="flex min-h-[60vh] flex-col items-center justify-center gap-3 p-8 text-center">
      <p className="text-lg font-medium">Something went wrong loading this page.</p>
      <p className="text-sm text-muted-foreground">It may have been a brief hiccup.</p>
      <button
        type="button"
        onClick={() => router.invalidate()}
        className="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:opacity-90"
      >
        Retry
      </button>
    </div>
  )
}

function RootDocument({ children }: { children: React.ReactNode }) {
  const { user } = Route.useRouteContext()
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const router = useRouter()
  const queryClient = useQueryClient()
  const { search, setSearch, query } = useChromeSearch(pathname)
  const isPublic = PUBLIC_PATHS.has(pathname)

  // Client-only: install the non-blocking browser log shipper (global error capture + flush-on-hide).
  useEffect(() => {
    initClientLogging()
  }, [])

  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <HeadContent />
        {/* No-FOUC: set the theme class before first paint (the example.com pattern). */}
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
      </head>
      <body>
        {isPublic ? (
          children
        ) : (
          <AppShell
            currentPath={pathname}
            LinkComponent={LinkAdapter}
            ingredientsNav
            searchValue={search}
            onSearchChange={setSearch}
            user={user ? { name: user.name, email: user.email } : null}
            onSignOut={async () => {
              await logoutFn()
              queryClient.clear() // drop the prior session's cached content before the gate flips
              await router.invalidate()
              await router.navigate({ to: '/login' })
            }}
          >
            {query ? (
              <SearchOverlay query={query} LinkComponent={LinkAdapter} />
            ) : (
              children
            )}
          </AppShell>
        )}
        <TanStackDevtools
          config={{
            position: 'bottom-right',
          }}
          plugins={[
            {
              name: 'Tanstack Router',
              render: <TanStackRouterDevtoolsPanel />,
            },
          ]}
        />
        <Scripts />
      </body>
    </html>
  )
}
