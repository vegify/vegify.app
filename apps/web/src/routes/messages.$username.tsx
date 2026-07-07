import { useEffect, useState } from "react"
import {
  queryOptions,
  useQueryClient,
  useSuspenseQuery
} from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import { createServerFn } from "@tanstack/react-start"
import {
  type ThreadMessage,
  ThreadView,
  type ThreadVM
} from "@vegify/ui/messages"

import { LinkAdapter } from "../link"

// One thread, addressed by the other party's handle (a profile's "Message" button lands here — the
// screen renders an empty composer even before a conversation exists). Fetching the thread marks
// their messages read server-side, so the loader itself drains the unread badge.
const getThreadFn = createServerFn({ method: "GET" })
  .validator((username: string) => username)
  .handler(async ({ data }): Promise<ThreadVM> => {
    const { getThread } = await import("../messages")
    return getThread(data)
  })

const sendMessageFn = createServerFn({ method: "POST" })
  .validator((p: { to: string; body: string }) => p)
  .handler(async ({ data }): Promise<ThreadMessage> => {
    const { sendMessage } = await import("../messages")
    return sendMessage(data.to, data.body)
  })

const threadQuery = (username: string) =>
  queryOptions({
    queryKey: ["thread", username],
    queryFn: () => getThreadFn({ data: username }),
    // A DM thread is live-ish even without web push: refetch while the tab sits open.
    refetchInterval: 15_000
  })

export const Route = createFileRoute("/messages/$username")({
  loader: ({ context, params }) =>
    context.queryClient.ensureQueryData(threadQuery(params.username)),
  component: ThreadPage
})

function ThreadPage() {
  const { username } = Route.useParams()
  const queryClient = useQueryClient()
  const { data: thread } = useSuspenseQuery(threadQuery(username))
  const [sending, setSending] = useState(false)

  // Opening the thread consumed unread state server-side — drop the chrome badge without waiting
  // for its next poll.
  useEffect(() => {
    queryClient.invalidateQueries({ queryKey: ["messages-unread"] })
  }, [queryClient, username])

  return (
    <ThreadView
      thread={thread}
      sending={sending}
      LinkComponent={LinkAdapter}
      onSend={async (body) => {
        setSending(true)
        try {
          await sendMessageFn({ data: { to: username, body } })
          await queryClient.invalidateQueries({
            queryKey: ["thread", username]
          })
          queryClient.invalidateQueries({ queryKey: ["conversations"] })
        } finally {
          setSending(false)
        }
      }}
    />
  )
}
