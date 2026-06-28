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
import {
  AppShell,
  EmailVerificationBanner,
  themeScript,
  useChromeSearch,
} from '@vegify/ui'

import { LinkAdapter } from '../link'
import { SearchOverlay } from '../search'
import { fetchUser, logoutFn, requestEmailVerificationFn } from '../auth'
import { initClientLogging } from '../client-log'
import appCss from '../styles.css?url'
import faviconUrl from '../favicon.ico?url'

// Auth and token pages render bare (no app shell): the auth forms plus the email-link actions.
const BARE_PATHS = new Set([
  '/login',
  '/signup',
  '/forgot',
  '/reset',
  '/verify',
])
// Pages reachable without a session. "/" is the public marketing landing for logged-out
// visitors (and crawlers) AND the app home for signed-in users — the index route branches on
// `user` — so it is public but never redirected in either direction. Everything else is gated.
const PUBLIC_PATHS = new Set(['/', ...BARE_PATHS])
// Auth FORMS a signed-in user has no use for → bounce them to "/". Deliberately NOT /verify or
// /reset: those consume a one-time token from an email link and must work even while signed in. (A
// signed-in user who clicks a verification link would otherwise be redirected to "/" before the
// token is consumed, so the email never gets marked verified and the banner never clears.)
const BOUNCE_WHEN_AUTHED = new Set(['/login', '/signup', '/forgot'])

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
      const user = await fetchUser()
      if (!user && !PUBLIC_PATHS.has(location.pathname))
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
  // The app chrome is for signed-in users only. Logged-out "/" (the marketing landing) and the
  // auth forms render bare — they bring their own layout.
  const showShell = !!user && !BARE_PATHS.has(pathname)

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
            user={user ? { name: user.name, email: user.email } : null}
            onSignOut={async () => {
              await logoutFn()
              queryClient.clear() // drop the prior session's cached content before the gate flips
              await router.invalidate()
              await router.navigate({ to: '/login' })
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
