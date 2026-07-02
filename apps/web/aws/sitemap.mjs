// Dynamic sitemap for the web SSR shell, served at /sitemap.xml by BOTH runtime wrappers (serve-bun +
// lambda-handler). It is NOT a TanStack route — page routes can't emit raw XML in this Start version —
// and NOT a static file, because the recipe/ingredient URLs grow as UGC is created. The dynamic,
// SEO-valuable URLs (every public recipe + ingredient, by canonical slug) come from the Axum content
// API (GET /api/content/sitemap, public data only). The small, stable set of top-level pages + blog
// posts is listed here: blog posts are authored in code (@vegify/ui/blog BLOG_POSTS) and can't be
// imported into this runtime .mjs from the Lambda bundle, so mirror a new post's slug here (low churn).
export const SITEMAP_PATH = "/sitemap.xml";

const STATIC_PATHS = ["/", "/recipes", "/ingredients", "/blog"];
// Keep in sync with @vegify/ui/blog BLOG_POSTS when publishing a post.
const BLOG_POST_SLUGS = ["no-such-thing-as-100-percent", "vegan-honestly"];

const xmlEscape = (s) =>
  s.replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&apos;" })[c],
  );
const urlTag = (origin, path) => `  <url><loc>${xmlEscape(origin + path)}</loc></url>`;

// Build the sitemap XML. `origin` is the public site origin (e.g. https://vegify.app); `apiBaseUrl` is
// the Axum base (VEGIFY_API_URL). If the API is unreachable we still emit the static + blog URLs.
export async function sitemapResponse(apiBaseUrl, origin) {
  let data = { recipes: [], ingredients: [] };
  try {
    const res = await fetch(`${apiBaseUrl}/api/content/sitemap`);
    if (res.ok) data = await res.json();
  } catch {
    /* API down → degrade to the static + blog URLs rather than 500 the crawler */
  }

  const paths = [
    ...STATIC_PATHS,
    ...BLOG_POST_SLUGS.map((s) => `/blog/${s}`),
    ...data.recipes.map((r) => `/${r.username}/${r.slug}`),
    ...data.ingredients.map((s) => `/ingredients/${s}`),
  ];
  const body =
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
    `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n` +
    `${paths.map((p) => urlTag(origin, p)).join("\n")}\n` +
    `</urlset>\n`;
  return new Response(body, {
    status: 200,
    headers: {
      "content-type": "application/xml; charset=utf-8",
      "cache-control": "public, max-age=3600",
    },
  });
}
