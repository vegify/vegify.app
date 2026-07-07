import {
  queryOptions,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import {
  ProfileView,
  type ProfileVM,
  type ReportReason,
} from "@vegify/ui/screens";
import { LinkAdapter } from "../link";

// Root-level dynamic handle: /<username>. Static routes (/recipes, /settings, …) outrank this, and
// the backend reserves those segments (handles.rs), so a handle can never shadow a real route. The
// profile is public and shareable: __root's auth gate treats "/<username>" as a public path, and
// getProfile is an anonymous (optionally-authed) read — logged-out visitors and crawlers see it too.
const getProfileFn = createServerFn({ method: "GET" })
  .validator((username: string) => username)
  .handler(async ({ data }): Promise<ProfileVM | null> => {
    const { getProfile } = await import("../content");
    const profile = await getProfile(data); // null => no account claims this handle
    if (!profile) return null;
    const { mediaUrl } = await import("../content");
    return {
      username: profile.username,
      name: profile.name,
      avatarUrl: mediaUrl(profile.avatarKey),
      recipes: profile.recipes.map((r) => ({
        ...r,
        photoUrl: mediaUrl(r.photoKey),
      })),
      ingredients: profile.ingredients,
    };
  });

const profileQuery = (username: string) =>
  queryOptions({
    queryKey: ["profile", username],
    queryFn: () => getProfileFn({ data: username }),
  });

const reportUserFn = createServerFn({ method: "POST" })
  .validator((d: { username: string; reason: ReportReason; note: string }) => d)
  .handler(async ({ data }) => {
    const { reportContent, getProfile } = await import("../content");
    const p = await getProfile(data.username);
    if (p)
      await reportContent({
        targetType: "user",
        targetId: data.username,
        reason: data.reason,
        note: data.note,
      });
  });

const blockUserFn = createServerFn({ method: "POST" })
  .validator((d: { username: string; block: boolean }) => d)
  .handler(async ({ data }) => {
    const { blockUser, unblockUser } = await import("../content");
    if (data.block) await blockUser(data.username);
    else await unblockUser(data.username);
  });

export const Route = createFileRoute("/$username/")({
  loader: ({ context, params }) =>
    context.queryClient.ensureQueryData(profileQuery(params.username)),
  component: ProfilePage,
});

function ProfilePage() {
  const { username } = Route.useParams();
  const { user } = Route.useRouteContext();
  const { data: profile } = useSuspenseQuery(profileQuery(username));
  const queryClient = useQueryClient();
  // Safety affordances only for a signed-in viewer looking at someone else.
  const canModerate = !!user && user.username !== username;
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
              await blockUserFn({ data: { username, block: true } });
              await queryClient.invalidateQueries({
                queryKey: ["messages-unread"],
              });
            }
          : undefined
      }
    />
  );
}
