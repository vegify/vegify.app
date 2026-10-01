import { queryOptions, useSuspenseQuery } from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import { createServerFn } from "@tanstack/react-start"
import { BlogIndexView } from "@vegify/ui/blog"

import { LinkAdapter } from "../link"
import { pageHead, siteOrigin } from "../seo"

// Public blog index — part of the SEO/GEO surface alongside the landing. Posts are DB-backed now
// (served by vegify-server), so this fetches the list; the auth gate lets /blog through logged-out
// via PUBLIC_SECTIONS (../auth-gate).
const getPosts = createServerFn({ method: "GET" }).handler(async () => {
  const { listBlogPosts } = await import("../content")
  return listBlogPosts()
})

const postsQuery = queryOptions({
  queryKey: ["blog"],
  queryFn: () => getPosts()
})

export const Route = createFileRoute("/blog/")({
  loader: ({ context }) => context.queryClient.ensureQueryData(postsQuery),
  head: async () =>
    pageHead({
      origin: await siteOrigin(),
      path: "/blog",
      title: "Blog",
      description:
        "Notes on plant-based nutrition from Vegify: research-led, citations included, honest about the caveats."
    }),
  component: BlogIndexPage
})

function BlogIndexPage() {
  const { data } = useSuspenseQuery(postsQuery)
  return <BlogIndexView posts={data} LinkComponent={LinkAdapter} />
}
