import {
  HeadContent,
  Scripts,
  createRootRoute,
  useRouterState,
} from '@tanstack/react-router'
import { useState } from 'react'
import { TanStackRouterDevtoolsPanel } from '@tanstack/react-router-devtools'
import { TanStackDevtools } from '@tanstack/react-devtools'
import { AppShell, themeScript } from '@vegify/ui'

import { LinkAdapter } from '../link'
import { SearchOverlay } from '../search'
import appCss from '../styles.css?url'
import faviconUrl from '../favicon.ico?url'

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
  shellComponent: RootDocument,
})

function RootDocument({ children }: { children: React.ReactNode }) {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const [search, setSearch] = useState('')
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <HeadContent />
        {/* No-FOUC: set the theme class before first paint (the example.com pattern). */}
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
      </head>
      <body>
        <AppShell
          currentPath={pathname}
          LinkComponent={LinkAdapter}
          ingredientsNav
          searchValue={search}
          onSearchChange={setSearch}
        >
          {search.trim() ? (
            <SearchOverlay query={search.trim()} LinkComponent={LinkAdapter} />
          ) : (
            children
          )}
        </AppShell>
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
