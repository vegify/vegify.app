import { queryOptions, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, notFound } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import { BlogPostView } from "@vegify/ui/blog";

import { LinkAdapter } from "../link";

// A single public blog post, fetched from the DB-backed blog API. The loader loads the post (404 on a
// miss) and returns its summary for the per-post head() meta; the full block body is read from the
// query cache in the component. The Article JSON-LD renders inside BlogPostView (landing precedent).
const getPost = createServerFn({ method: "GET" })
  .validator((slug: string) => slug)
  .handler(async ({ data }) => {
    const { getBlogPost } = await import("../content");
    return getBlogPost(data);
  });

const postQuery = (slug: string) =>
  queryOptions({
    queryKey: ["blog", slug],
    queryFn: () => getPost({ data: slug }),
  });

export const Route = createFileRoute("/blog/$slug")({
  loader: async ({ context, params }) => {
    const post = await context.queryClient.ensureQueryData(
      postQuery(params.slug),
    );
    if (!post) throw notFound();
    return {
      slug: post.slug,
      title: post.title,
      description: post.description,
      datePublished: post.datePublished,
    };
  },
  head: ({ loaderData }) => {
    if (!loaderData) return {};
    const url = `https://vegify.app/blog/${loaderData.slug}`;
    return {
      meta: [
        { title: `${loaderData.title} | Vegify` },
        { name: "description", content: loaderData.description },
        { property: "og:type", content: "article" },
        { property: "og:site_name", content: "Vegify" },
        { property: "og:title", content: loaderData.title },
        { property: "og:description", content: loaderData.description },
        { property: "og:url", content: url },
        {
          property: "article:published_time",
          content: loaderData.datePublished,
        },
        { name: "twitter:card", content: "summary" },
        { name: "twitter:title", content: loaderData.title },
        { name: "twitter:description", content: loaderData.description },
      ],
      links: [{ rel: "canonical", href: url }],
    };
  },
  component: BlogPostPage,
});

function BlogPostPage() {
  const { slug } = Route.useParams();
  const { data } = useSuspenseQuery(postQuery(slug));
  if (!data) return null; // loader already 404s on a miss; this satisfies the nullable query type
  return <BlogPostView post={data} LinkComponent={LinkAdapter} />;
}
