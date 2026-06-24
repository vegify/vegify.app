import { createFileRoute } from '@tanstack/react-router'
import { HomeView } from '@vegify/ui'
import { LinkAdapter } from '../link'

export const Route = createFileRoute('/')({ component: Home })

function Home() {
  return <HomeView LinkComponent={LinkAdapter} />
}
