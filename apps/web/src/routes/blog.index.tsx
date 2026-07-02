import { createFileRoute } from '@tanstack/react-router'
import { BlogIndexView } from '@vegify/ui/blog'
import { LinkAdapter } from '../link'

// Public blog index — part of the SEO/GEO surface alongside the landing. Static module content
// (posts are authored in code in @vegify/ui/blog), so there is no loader; the auth gate lets
// /blog through logged-out via PUBLIC_SECTIONS (../auth-gate).

const TITLE = 'Blog | Vegify'
const DESCRIPTION =
  'Notes on plant-based nutrition from Vegify: research-led, citations included, honest about the caveats.'
const URL = 'https://vegify.app/blog'

export const Route = createFileRoute('/blog/')({
  head: () => ({
    meta: [
      { title: TITLE },
      { name: 'description', content: DESCRIPTION },
      { property: 'og:type', content: 'website' },
      { property: 'og:site_name', content: 'Vegify' },
      { property: 'og:title', content: TITLE },
      { property: 'og:description', content: DESCRIPTION },
      { property: 'og:url', content: URL },
      { name: 'twitter:card', content: 'summary' },
      { name: 'twitter:title', content: TITLE },
      { name: 'twitter:description', content: DESCRIPTION },
    ],
    links: [{ rel: 'canonical', href: URL }],
  }),
  component: BlogIndexPage,
})

function BlogIndexPage() {
  return <BlogIndexView LinkComponent={LinkAdapter} />
}
