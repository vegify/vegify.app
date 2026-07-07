import type { ReactNode } from "react"
import { marked } from "marked"

import { NutrientByGroupChart, NutrientRangeChart } from "./blog-charts"
import type { NavLink } from "./screens"
import { VegifyLogo } from "./vegify-logo"

/**
 * BLOG — the public, unauthenticated writing surface (vegify.app/blog), the GEO/SEO layer's citable
 * content. Renders bare (no AppShell), extends the brand system (serif display + tokens), web-only.
 *
 * Posts are DATA, served from the DB (`services/api` `posts` table) over the content API — NOT
 * authored in code, so publishing a post never bumps the app version or triggers a deploy. A body is a
 * list of blocks: `prose` (markdown) + `figure` (a blog chart + caption + markdown note). These views
 * are pure: they take the fetched data + the `LinkComponent` nav port; the web routes do the fetching.
 */

/** A post-body block: `prose` markdown, or a `figure` (one of the blog charts) with caption + note. */
export type BlogBlock =
  | { type: "prose"; md: string }
  | {
      type: "figure"
      variant: "range"
      name: string
      target: number
      ceiling: number
      unit: string
      caption: string
      note: string
    }
  | {
      type: "figure"
      variant: "group"
      unit: string
      ceiling?: number
      groups: { label: string; value: number }[]
      caption: string
      note: string
    }

/** Index-card shape (no body). */
export type BlogSummary = {
  slug: string
  title: string
  description: string
  datePublished: string
  dateDisplay: string
}
/** A full post: the summary plus its block body. */
export type BlogPostData = BlogSummary & { body: BlogBlock[] }

// Prose styling for a rendered-markdown block: space its paragraphs/lists and style links + strong + ul
// (Tailwind preflight strips default <p>/<ul> margins, so spacing is explicit). `space-y-5` spaces the
// <p>/<ul> that `marked` emits as direct children.
const PROSE =
  "space-y-5 [&_a]:font-medium [&_a]:text-primary-dark dark:[&_a]:text-primary-light [&_a]:underline [&_a]:underline-offset-2 hover:[&_a]:text-primary dark:hover:[&_a]:text-primary [&_strong]:font-semibold [&_ul]:list-disc [&_ul]:space-y-3 [&_ul]:pl-6"
const NOTE_LINKS =
  "[&_a]:font-medium [&_a]:text-primary-dark dark:[&_a]:text-primary-light [&_a]:underline [&_a]:underline-offset-2"

// Post content is trusted (our seed / the owner's authoring) — no untrusted UGC here, so rendering the
// markdown → HTML directly is fine. Revisit sanitization if the blog ever takes third-party submissions.
const mdBlock = (md: string) => marked.parse(md, { async: false }) as string
const mdInline = (md: string) =>
  marked.parseInline(md, { async: false }) as string

function BlockView({ block }: { block: BlogBlock }) {
  if (block.type === "prose") {
    return (
      <div
        className={PROSE}
        // biome-ignore lint/security/noDangerouslySetInnerHtml: post markdown is repo-authored content rendered through marked; no user input reaches it
        dangerouslySetInnerHTML={{ __html: mdBlock(block.md) }}
      />
    )
  }
  return (
    <figure className="rounded-xl bg-card p-5 ring-1 ring-foreground/10">
      <figcaption className="mb-3 font-semibold text-foreground text-sm">
        {block.caption}
      </figcaption>
      {block.variant === "range" ? (
        <NutrientRangeChart
          name={block.name}
          target={block.target}
          ceiling={block.ceiling}
          unit={block.unit}
        />
      ) : (
        <NutrientByGroupChart
          unit={block.unit}
          ceiling={block.ceiling}
          groups={block.groups}
        />
      )}
      <p
        className={`mt-2 text-muted-foreground text-sm ${NOTE_LINKS}`}
        // biome-ignore lint/security/noDangerouslySetInnerHtml: figure notes are repo-authored markdown; no user input reaches them
        dangerouslySetInnerHTML={{ __html: mdInline(block.note) }}
      />
    </figure>
  )
}

/** Slim bare-page chrome shared by the index and post pages (the blog renders without the AppShell). */
function BlogChrome({
  LinkComponent,
  children
}: {
  LinkComponent: NavLink
  children: ReactNode
}) {
  return (
    <div className="min-h-screen bg-background text-foreground">
      <header className="bg-green-dark">
        <div className="mx-auto flex max-w-2xl items-center justify-between px-6 py-4">
          {/* text-white: the mark inherits currentColor; without it light mode paints it near-black on the green band. */}
          <LinkComponent
            href="/"
            className="block w-28 text-white"
            aria-label="Vegify home"
          >
            <VegifyLogo className="h-auto w-full" />
          </LinkComponent>
          <nav className="flex items-center gap-5 font-semibold text-sm text-white/90">
            <LinkComponent href="/blog" className="transition hover:text-white">
              Blog
            </LinkComponent>
            <LinkComponent
              href="/recipes"
              className="transition hover:text-white"
            >
              Browse recipes
            </LinkComponent>
          </nav>
        </div>
      </header>
      <main className="mx-auto max-w-2xl px-6 py-12">{children}</main>
      <footer className="mx-auto max-w-2xl px-6 pb-12 text-muted-foreground text-sm">
        <LinkComponent
          href="/"
          className="font-medium text-primary-dark underline underline-offset-2 dark:text-primary-light"
        >
          Vegify
        </LinkComponent>
        <span className="mx-2" aria-hidden>
          ·
        </span>
        micronutrition tracking for plant-based cooking.
      </footer>
    </div>
  )
}

export function BlogIndexView({
  posts,
  LinkComponent
}: {
  posts: BlogSummary[]
  LinkComponent: NavLink
}) {
  return (
    <BlogChrome LinkComponent={LinkComponent}>
      <h1 className="mb-2 font-bold font-serif text-4xl text-primary-dark dark:text-primary-light">
        Blog
      </h1>
      <p className="mb-10 text-muted-foreground">
        Notes on plant-based nutrition: research-led, citations included, honest
        about the caveats.
      </p>
      <ul className="space-y-8">
        {posts.map((post) => (
          <li key={post.slug}>
            <LinkComponent href={`/blog/${post.slug}`} className="group block">
              <h2 className="font-semibold font-serif text-2xl group-hover:text-primary-dark dark:group-hover:text-primary-light">
                {post.title}
              </h2>
              <p className="mt-1 text-muted-foreground text-sm">
                {post.dateDisplay}
              </p>
              <p className="mt-2 text-muted-foreground">{post.description}</p>
            </LinkComponent>
          </li>
        ))}
      </ul>
    </BlogChrome>
  )
}

/** Article JSON-LD, mirroring the landing's inline-script pattern — the GEO-citable node. */
function BlogPostJsonLd({ post }: { post: BlogSummary }) {
  const data = {
    "@context": "https://schema.org",
    "@type": "Article",
    headline: post.title,
    description: post.description,
    datePublished: post.datePublished,
    url: `https://vegify.app/blog/${post.slug}`,
    mainEntityOfPage: `https://vegify.app/blog/${post.slug}`,
    author: {
      "@type": "Person",
      name: "John M. Carmack",
      url: "https://vegify.app"
    },
    publisher: {
      "@type": "Organization",
      name: "Vegify",
      url: "https://vegify.app/"
    }
  }
  return (
    <script
      type="application/ld+json"
      // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD of locally built post metadata; the standard way to emit structured data
      dangerouslySetInnerHTML={{ __html: JSON.stringify(data) }}
    />
  )
}

export function BlogPostView({
  post,
  LinkComponent
}: {
  post: BlogPostData
  LinkComponent: NavLink
}) {
  return (
    <BlogChrome LinkComponent={LinkComponent}>
      <BlogPostJsonLd post={post} />
      <p className="mb-3 text-muted-foreground text-sm">
        <time dateTime={post.datePublished}>{post.dateDisplay}</time>
        <span className="mx-2" aria-hidden>
          ·
        </span>
        John M. Carmack
      </p>
      <h1 className="mb-8 font-bold font-serif text-4xl text-primary-dark leading-tight dark:text-primary-light">
        {post.title}
      </h1>
      <div className="space-y-6 text-[17px] text-foreground leading-relaxed">
        {post.body
          .map((block, i) => ({ block, key: `block-${i}` }))
          .map(({ block, key }) => (
            <BlockView key={key} block={block} />
          ))}
      </div>
      <p className="mt-12">
        <LinkComponent
          href="/blog"
          className="font-medium text-primary-dark underline underline-offset-2 dark:text-primary-light"
        >
          ← All posts
        </LinkComponent>
      </p>
    </BlogChrome>
  )
}
