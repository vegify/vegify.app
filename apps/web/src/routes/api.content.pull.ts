import { createFileRoute } from '@tanstack/react-router'
import { withUser } from '../api-auth'
import { pullContent } from '../content'

// P1 content API — full pull for the desktop sync engine. GET → { recipes, ingredients } in MUTATION
// shape (ids + asIngredientId + owner + items) for the viewer's listed set (public + own). The desktop
// applies each via do_save_* (FK off) and prunes locals not returned. Bearer-authed; scoped per-viewer.
export const Route = createFileRoute('/api/content/pull')({
  server: {
    handlers: {
      GET: ({ request }) => withUser(request, (me) => pullContent(me)),
    },
  },
})
