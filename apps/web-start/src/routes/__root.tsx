import {
  HeadContent,
  Link,
  Scripts,
  createRootRoute,
  useRouterState,
} from '@tanstack/react-router'
import { TanStackRouterDevtoolsPanel } from '@tanstack/react-router-devtools'
import { TanStackDevtools } from '@tanstack/react-devtools'
import { AppShell, type AppShellLinkProps } from '@vegify/ui'

import appCss from '../styles.css?url'
import faviconUrl from '../favicon.svg?url'

function LinkAdapter({ href, ...props }: AppShellLinkProps) {
  return <Link to={href} {...props} />
}

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
        type: 'image/svg+xml',
        href: faviconUrl,
      },
    ],
  }),
  shellComponent: RootDocument,
})

function RootDocument({ children }: { children: React.ReactNode }) {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  return (
    <html lang="en">
      <head>
        <HeadContent />
      </head>
      <body>
        <AppShell currentPath={pathname} LinkComponent={LinkAdapter} ingredientsNav>
          {children}
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
