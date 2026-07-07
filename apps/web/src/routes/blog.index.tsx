import { queryOptions, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import { BlogIndexView } from "@vegify/ui/blog";
import { LinkAdapter } from "../link";

// Public blog index — part of the SEO/GEO surface alongside the landing. Posts are DB-backed now
// (served by vegify-server), so this fetches the list; the auth gate lets /blog through logged-out
// via PUBLIC_SECTIONS (../auth-gate).
const getPosts = createServerFn({ method: "GET" }).handler(async () => {
  const { listBlogPosts } = await import("../content");
  return listBlogPosts();
});

const postsQuery = queryOptions({
  queryKey: ["blog"],
  queryFn: () => getPosts(),
});

const TITLE = "Blog | Vegify";
const DESCRIPTION =
  "Notes on plant-based nutrition from Vegify: research-led, citations included, honest about the caveats.";
const URL = "https://vegify.app/blog";

export const Route = createFileRoute("/blog/")({
  loader: ({ context }) => context.queryClient.ensureQueryData(postsQuery),
  head: () => ({
    meta: [
      { title: TITLE },
      { name: "description", content: DESCRIPTION },
      { property: "og:type", content: "website" },
      { property: "og:site_name", content: "Vegify" },
      { property: "og:title", content: TITLE },
      { property: "og:description", content: DESCRIPTION },
      { property: "og:url", content: URL },
      { name: "twitter:card", content: "summary" },
      { name: "twitter:title", content: TITLE },
      { name: "twitter:description", content: DESCRIPTION },
    ],
    links: [{ rel: "canonical", href: URL }],
  }),
  component: BlogIndexPage,
});

function BlogIndexPage() {
  const { data } = useSuspenseQuery(postsQuery);
  return <BlogIndexView posts={data} LinkComponent={LinkAdapter} />;
}
