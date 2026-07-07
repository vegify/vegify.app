import type { ReactNode } from "react"

// Shared chrome for the standalone legal pages (Terms, Privacy) — bare (their own minimal header +
// back link), public + crawlable, readable typography. Linked from the footer and the App Store
// listing; the zero-tolerance clause in Terms is an App Review 1.2 requirement.
export function LegalPage({
  title,
  updated,
  children
}: {
  title: string
  updated: string
  children: ReactNode
}) {
  return (
    <div className="min-h-screen bg-background text-foreground">
      <header className="border-border border-b">
        <div className="mx-auto flex max-w-3xl items-center justify-between p-4">
          <a
            href="/"
            className="font-bold font-serif text-2xl text-primary-dark dark:text-primary-light"
          >
            Vegify
          </a>
          <a
            href="/"
            className="text-muted-foreground text-sm underline-offset-2 hover:underline"
          >
            Back to app
          </a>
        </div>
      </header>
      <main className="mx-auto max-w-3xl px-4 py-10">
        <h1 className="mb-1 font-bold font-serif text-4xl text-primary-dark dark:text-primary-light">
          {title}
        </h1>
        <p className="mb-8 text-muted-foreground text-sm">
          Last updated {updated}
        </p>
        <article className="flex flex-col gap-4 leading-relaxed [&_a]:text-primary [&_a]:underline [&_h2]:mt-6 [&_h2]:font-bold [&_h2]:font-serif [&_h2]:text-xl [&_li]:ml-5 [&_li]:list-disc [&_ul]:flex [&_ul]:flex-col [&_ul]:gap-1">
          {children}
        </article>
        <footer className="mt-12 border-border border-t pt-6 text-muted-foreground text-sm">
          <a href="/terms" className="hover:underline">
            Terms
          </a>
          <span aria-hidden className="mx-2">
            ·
          </span>
          <a href="/privacy" className="hover:underline">
            Privacy
          </a>
          <span aria-hidden className="mx-2">
            ·
          </span>
          <span>© 2026 Vegify</span>
        </footer>
      </main>
    </div>
  )
}
