import {
  HeadContent,
  Scripts,
  createRootRoute,
  redirect,
  useRouter,
  useRouterState,
} from '@tanstack/react-router'
import { useState } from 'react'
import { TanStackRouterDevtoolsPanel } from '@tanstack/react-router-devtools'
import { TanStackDevtools } from '@tanstack/react-devtools'
import { AppShell, themeScript } from '@vegify/ui'

import { LinkAdapter } from '../link'
import { SearchOverlay } from '../search'
import { fetchUser, logoutFn } from '../auth'
import appCss from '../styles.css?url'
import faviconUrl from '../favicon.ico?url'

// Accounts are required: every page is gated except these. Login/signup render bare (no chrome).
const PUBLIC_PATHS = new Set(['/login', '/signup'])

export const Route = createRootRoute({
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
    const user = await fetchUser()
    const isPublic = PUBLIC_PATHS.has(location.pathname)
    if (!user && !isPublic) throw redirect({ to: '/login' })
    if (user && isPublic) throw redirect({ to: '/' })
    return { user }
  },
  shellComponent: RootDocument,
})

function RootDocument({ children }: { children: React.ReactNode }) {
  const { user } = Route.useRouteContext()
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const router = useRouter()
  const [search, setSearch] = useState('')
  const isPublic = PUBLIC_PATHS.has(pathname)

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
              await router.invalidate()
              await router.navigate({ to: '/login' })
            }}
          >
            {search.trim() ? (
              <SearchOverlay query={search.trim()} LinkComponent={LinkAdapter} />
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
