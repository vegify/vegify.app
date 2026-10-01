import {
  queryOptions,
  useQueryClient,
  useSuspenseQuery
} from "@tanstack/react-query"
import { createFileRoute, notFound } from "@tanstack/react-router"
import { createServerFn } from "@tanstack/react-start"
import {
  ProfileView,
  type ProfileVM,
  type ReportReason
} from "@vegify/ui/screens"

import { LinkAdapter } from "../link"
import {
  composeDescription,
  counted,
  listOf,
  pageHead,
  siteOrigin
} from "../seo"

// Root-level dynamic handle: /<username>. Static routes (/recipes, /settings, …) outrank this, and
// the backend reserves those segments (handles.rs), so a handle can never shadow a real route. The
// profile is public and shareable: __root's auth gate treats "/<username>" as a public path, and
// getProfile is an anonymous (optionally-authed) read — logged-out visitors and crawlers see it too.
const getProfileFn = createServerFn({ method: "GET" })
  .validator((username: string) => username)
  .handler(async ({ data }): Promise<ProfileVM | null> => {
    const { getProfile } = await import("../content")
    const profile = await getProfile(data) // null => no account claims this handle
    if (!profile) return null
    const { mediaUrl } = await import("../content")
    return {
      username: profile.username,
      name: profile.name,
      avatarUrl: mediaUrl(profile.avatarKey),
      recipes: profile.recipes.map((r) => ({
        ...r,
        photoUrl: mediaUrl(r.photoKey)
      })),
      ingredients: profile.ingredients
    }
  })

const profileQuery = (username: string) =>
  queryOptions({
    queryKey: ["profile", username],
    queryFn: () => getProfileFn({ data: username })
  })

const reportUserFn = createServerFn({ method: "POST" })
  .validator((d: { username: string; reason: ReportReason; note: string }) => d)
  .handler(async ({ data }) => {
    const { reportContent, getProfile } = await import("../content")
    const p = await getProfile(data.username)
    if (p)
      await reportContent({
        targetType: "user",
        targetId: data.username,
        reason: data.reason,
        note: data.note
      })
  })

const blockUserFn = createServerFn({ method: "POST" })
  .validator((d: { username: string; block: boolean }) => d)
  .handler(async ({ data }) => {
    const { blockUser, unblockUser } = await import("../content")
    if (data.block) await blockUser(data.username)
    else await unblockUser(data.username)
  })

export const Route = createFileRoute("/$username/")({
  loader: async ({ context, params }) => {
    const profile = await context.queryClient.ensureQueryData(
      profileQuery(params.username)
    )
    // An unclaimed handle is a real 404 (notFoundComponent below keeps the friendly view).
    if (!profile) throw notFound()
    return profile
  },
  head: async ({ loaderData: profile, params }) => {
    const origin = await siteOrigin()
    const path = `/${params.username}`
    if (!profile)
      return pageHead({
        origin,
        path,
        title: "Profile not found",
        description: "No Vegify account uses this handle.",
        noindex: true
      })
    const who = `${profile.name} (@${profile.username})`
    const has = [
      profile.recipes.length && counted(profile.recipes.length, "recipe"),
      profile.ingredients.length &&
        counted(profile.ingredients.length, "ingredient")
    ].filter((s): s is string => !!s)
    return pageHead({
      origin,
      path,
      title: who,
      type: "profile",
      description: composeDescription([
        has.length
          ? `${who} on Vegify: ${listOf(has)}, each with its vitamins and minerals worked out.`
          : `${who} on Vegify.`
      ]),
      image: profile.avatarUrl,
      jsonLd: {
        "@context": "https://schema.org",
        "@type": "ProfilePage",
        url: `${origin}${path}`,
        mainEntity: {
          "@type": "Person",
          name: profile.name,
          alternateName: `@${profile.username}`,
          url: `${origin}${path}`,
          image: profile.avatarUrl ?? undefined
        }
      }
    })
  },
  component: ProfilePage,
  notFoundComponent: ProfileNotFound
})

function ProfilePage() {
  const { username } = Route.useParams()
  const { user } = Route.useRouteContext()
  const { data: profile } = useSuspenseQuery(profileQuery(username))
  const queryClient = useQueryClient()
  // Safety affordances only for a signed-in viewer looking at someone else.
  const canModerate = !!user && user.username !== username
  return (
    <ProfileView
      username={username}
      profile={profile}
      LinkComponent={LinkAdapter}
      canMessage={canModerate}
      onReport={
        canModerate
          ? (reason, note) => reportUserFn({ data: { username, reason, note } })
          : undefined
      }
      onToggleBlock={
        canModerate
          ? async () => {
              await blockUserFn({ data: { username, block: true } })
              await queryClient.invalidateQueries({
                queryKey: ["messages-unread"]
              })
            }
          : undefined
      }
    />
  )
}

// The same "no one goes by that handle" view the profile screen shows, now served with a 404.
function ProfileNotFound() {
  const { username } = Route.useParams()
  return (
    <ProfileView
      username={username}
      profile={null}
      LinkComponent={LinkAdapter}
    />
  )
}
