import { Link, createFileRoute } from '@tanstack/react-router'
import { buttonClasses } from '@vegify/ui'

export const Route = createFileRoute('/')({ component: Home })

function Home() {
  return (
    <main className="mx-auto flex min-h-screen max-w-3xl flex-col items-center justify-center gap-6 p-8">
      <h1 className="text-5xl font-bold text-primary-dark">Vegify</h1>
      <p className="text-lg text-gray-500">
        Micronutrition tracking for plant-based cooking — TanStack Start shell
      </p>
      <Link to="/recipes" className={buttonClasses({ size: 'lg' })}>
        Browse recipes
      </Link>
    </main>
  )
}
