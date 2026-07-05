import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { queryOptions, useSuspenseQuery } from '@tanstack/react-query'
import { ProfileView, type ProfileVM } from '@vegify/ui/screens'
import { LinkAdapter } from '../link'

// Root-level dynamic handle: /<username>. Static routes (/recipes, /settings, …) outrank this, and
// the backend reserves those segments (handles.rs), so a handle can never shadow a real route. The
// profile is public and shareable: __root's auth gate treats "/<username>" as a public path, and
// getProfile is an anonymous (optionally-authed) read — logged-out visitors and crawlers see it too.
const getProfileFn = createServerFn({ method: 'GET' })
  .validator((username: string) => username)
  .handler(async ({ data }): Promise<ProfileVM | null> => {
    const { getProfile } = await import('../content')
    const profile = await getProfile(data) // null => no account claims this handle
    if (!profile) return null
    return {
      username: profile.username,
      name: profile.name,
      recipes: profile.recipes,
      ingredients: profile.ingredients,
    }
  })

const profileQuery = (username: string) =>
  queryOptions({ queryKey: ['profile', username], queryFn: () => getProfileFn({ data: username }) })

export const Route = createFileRoute('/$username/')({
  loader: ({ context, params }) => context.queryClient.ensureQueryData(profileQuery(params.username)),
  component: ProfilePage,
})

function ProfilePage() {
  const { username } = Route.useParams()
  const { user } = Route.useRouteContext()
  const { data: profile } = useSuspenseQuery(profileQuery(username))
  return (
    <ProfileView
      username={username}
      profile={profile}
      LinkComponent={LinkAdapter}
      canMessage={!!user && user.username !== username}
    />
  )
}
