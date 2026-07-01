import { createFileRoute, notFound } from '@tanstack/react-router'
import { BlogPostView, postBySlug } from '@vegify/ui/blog'
import { LinkAdapter } from '../link'

// A single public blog post. Posts are a static in-code registry (@vegify/ui/blog), so the
// loader just validates the slug (404 on a miss) and hands head() what it needs for the
// per-post meta; the Article JSON-LD renders inside BlogPostView (landing precedent).

export const Route = createFileRoute('/blog/$slug')({
  loader: ({ params }) => {
    const post = postBySlug(params.slug)
    if (!post) throw notFound()
    return { slug: post.slug }
  },
  head: ({ loaderData }) => {
    const post = loaderData ? postBySlug(loaderData.slug) : undefined
    if (!post) return {}
    const url = `https://vegify.app/blog/${post.slug}`
    return {
      meta: [
        { title: `${post.title} | Vegify` },
        { name: 'description', content: post.description },
        { property: 'og:type', content: 'article' },
        { property: 'og:site_name', content: 'Vegify' },
        { property: 'og:title', content: post.title },
        { property: 'og:description', content: post.description },
        { property: 'og:url', content: url },
        { property: 'article:published_time', content: post.datePublished },
        { name: 'twitter:card', content: 'summary' },
        { name: 'twitter:title', content: post.title },
        { name: 'twitter:description', content: post.description },
      ],
      links: [{ rel: 'canonical', href: url }],
    }
  },
  component: BlogPostPage,
})

function BlogPostPage() {
  const { slug } = Route.useLoaderData()
  const post = postBySlug(slug)!
  return <BlogPostView post={post} LinkComponent={LinkAdapter} />
}
