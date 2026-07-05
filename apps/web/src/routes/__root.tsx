import {
  HeadContent,
  Scripts,
  createRootRouteWithContext,
  redirect,
  useRouter,
  useRouterState,
} from '@tanstack/react-router'
import { useEffect } from 'react'
import { useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query'
import { createServerFn } from '@tanstack/react-start'
import { TanStackRouterDevtoolsPanel } from '@tanstack/react-router-devtools'
import { TanStackDevtools } from '@tanstack/react-devtools'
import { AppShell } from '@vegify/ui/app-shell'
import { EmailVerificationBanner } from '@vegify/ui/auth-form'
import { themeScript } from '@vegify/ui/use-theme'
import { useChromeSearch } from '@vegify/ui/use-chrome-search'

import { LinkAdapter } from '../link'
import { SearchOverlay } from '../search'
import { fetchUser, logoutFn, requestEmailVerificationFn } from '../auth'
import { initClientLogging } from '../client-log'
import { BOUNCE_WHEN_AUTHED, isBarePath, isPublicPath } from '../auth-gate'
import appCss from '../styles.css?url'
import faviconUrl from '../favicon.ico?url'

// Path policy (which paths are reachable logged-out, which bounce a signed-in user) lives in ../auth-gate,
// where it is derived fail-closed from the route files and unit-tested.

// The chrome's unread badges (DMs + bell). Client-polled (60s + window focus + invalidated by the
// consuming routes) — the web has no push channel yet; the counts are cheap and auth-scoped.
const getUnreadFn = createServerFn({ method: 'GET' }).handler(async (): Promise<number> => {
  const { unreadCount } = await import('../messages')
  return unreadCount()
})

const getUnreadNotificationsFn = createServerFn({ method: 'GET' }).handler(async (): Promise<number> => {
  const { unreadNotifications } = await import('../notifications')
  return unreadNotifications()
})

export const Route = createRootRouteWithContext<{ queryClient: QueryClient }>()(
  {
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
      // Graceful degradation: fetchUser returns null on a clean 401, so a throw here means the backend is
      // unreachable (e.g. the brief window during a backend redeploy). Don't take the public pages down
      // with it — render the landing and auth forms anonymously; only gated pages (which we can't serve
      // without knowing the user) fall through to the retry boundary.
      const user = await fetchUser().catch((e) => {
        if (isPublicPath(location.pathname)) return null
        throw e
      })
      if (!user && !isPublicPath(location.pathname))
        throw redirect({ to: '/login' })
      if (user && BOUNCE_WHEN_AUTHED.has(location.pathname))
        throw redirect({ to: '/' })
      return { user }
    },
    errorComponent: RootErrorBoundary,
    shellComponent: RootDocument,
  },
)

// Graceful catch-all: a loader that fails after retries (or any render error) shows this instead of a
// white screen. Retry re-runs the route's loaders — usually enough for a transient throttle.
function RootErrorBoundary() {
  const router = useRouter()
  return (
    <div className="flex min-h-[60vh] flex-col items-center justify-center gap-3 p-8 text-center">
      <p className="text-lg font-medium">
        Something went wrong loading this page.
      </p>
      <p className="text-sm text-muted-foreground">
        It may have been a brief hiccup.
      </p>
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
  const { data: unreadMessages } = useQuery({
    queryKey: ['messages-unread'],
    queryFn: () => getUnreadFn(),
    enabled: !!user,
    refetchInterval: 60_000,
    refetchOnWindowFocus: true,
  })
  const { data: unreadNotifications } = useQuery({
    queryKey: ['notifications-unread'],
    queryFn: () => getUnreadNotificationsFn(),
    enabled: !!user,
    refetchInterval: 60_000,
    refetchOnWindowFocus: true,
  })
  // The app chrome wraps every page EXCEPT the bare surfaces (auth/token forms + the blog, which
  // carries its own chrome) and the logged-out "/" marketing landing. A logged-out visitor browsing
  // the public catalog (/recipes, /<username>, …) still gets the shell — with a "Sign in" affordance
  // and no New/Edit controls (the shared screens gate those on session).
  const showShell = !isBarePath(pathname) && (pathname !== '/' || !!user)

  // Client-only: install the non-blocking browser log shipper (global error capture + flush-on-hide).
  useEffect(() => {
    initClientLogging()
  }, [])

  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <HeadContent />
        {/* No-FOUC: set the theme class before first paint. */}
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
      </head>
      <body>
        {showShell ? (
          <AppShell
            currentPath={pathname}
            LinkComponent={LinkAdapter}
            ingredientsNav
            searchValue={search}
            onSearchChange={setSearch}
            user={user ? { name: user.name, email: user.email, username: user.username } : null}
            unreadMessages={unreadMessages ?? 0}
            unreadNotifications={unreadNotifications ?? 0}
            onSignOut={async () => {
              await logoutFn()
              queryClient.clear() // drop the prior session's cached content before the gate flips
              await router.invalidate()
              // Stay HOME after sign-out (public browsing is the default experience) — bouncing to
              // /login read as an eviction.
              await router.navigate({ to: '/' })
            }}
          >
            {user && !user.email_verified ? (
              <EmailVerificationBanner
                email={user.email}
                onResend={async () => {
                  await requestEmailVerificationFn({
                    data: { email: user.email },
                  })
                }}
              />
            ) : null}
            {query ? (
              <SearchOverlay query={query} LinkComponent={LinkAdapter} />
            ) : (
              children
            )}
          </AppShell>
        ) : (
          children
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
