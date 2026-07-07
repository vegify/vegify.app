// Dynamic sitemap for the web SSR shell, served at /sitemap.xml by BOTH runtime wrappers (serve-bun +
// lambda-handler). It is NOT a TanStack route — page routes can't emit raw XML in this Start version —
// and NOT a static file, because the URLs grow as content is created. Everything dynamic comes from the
// Axum content API (public data only): public recipes + ingredients from GET /api/content/sitemap, and
// blog posts from GET /api/content/blog (the blog is DB-backed now, so a new post auto-appears here —
// no hardcoded slug list to maintain). Only the stable top-level section pages are listed inline.
export const SITEMAP_PATH = "/sitemap.xml"

const STATIC_PATHS = ["/", "/recipes", "/ingredients", "/blog"]

const xmlEscape = (s) =>
  s.replace(
    /[&<>"']/g,
    (c) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&apos;"
      })[c]
  )
const urlTag = (origin, path) =>
  `  <url><loc>${xmlEscape(origin + path)}</loc></url>`

// Build the sitemap XML. `origin` is the public site origin (e.g. https://vegify.app); `apiBaseUrl` is
// the Axum base (VEGIFY_API_URL). If the API is unreachable we still emit the static + blog URLs.
export async function sitemapResponse(apiBaseUrl, origin) {
  let data = { recipes: [], ingredients: [] }
  let posts = []
  try {
    const [sm, blog] = await Promise.all([
      fetch(`${apiBaseUrl}/api/content/sitemap`),
      fetch(`${apiBaseUrl}/api/content/blog`)
    ])
    if (sm.ok) data = await sm.json()
    if (blog.ok) posts = await blog.json()
  } catch {
    /* API down → degrade to the static top-level URLs rather than 500 the crawler */
  }

  const paths = [
    ...STATIC_PATHS,
    ...posts.map((p) => `/blog/${p.slug}`),
    ...data.recipes.map((r) => `/${r.username}/${r.slug}`),
    // Owned ingredients are canonical under their creator; the catalog stays global.
    ...data.ingredients.map((i) =>
      i.username
        ? `/${i.username}/ingredients/${i.slug}`
        : `/ingredients/${i.slug}`
    )
  ]
  const body =
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
    `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n` +
    `${paths.map((p) => urlTag(origin, p)).join("\n")}\n` +
    `</urlset>\n`
  return new Response(body, {
    status: 200,
    headers: {
      "content-type": "application/xml; charset=utf-8",
      "cache-control": "public, max-age=3600"
    }
  })
}
