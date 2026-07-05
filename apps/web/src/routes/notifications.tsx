import { createFileRoute } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { queryOptions, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'
import { useEffect } from 'react'
import { NotificationsView, type NotificationVM } from '@vegify/ui/notifications'
import { LinkAdapter } from '../link'

// The bell. A static top-level route file, so the auth gate derives "gated" automatically. The list
// renders with unread highlights first, THEN a mount effect marks everything read (bell-standard:
// you saw it, the badge drops — but this visit still shows what was new).
const getNotificationsFn = createServerFn({ method: 'GET' }).handler(
  async (): Promise<NotificationVM[]> => {
    const { listNotifications } = await import('../notifications')
    return listNotifications()
  },
)

const markReadFn = createServerFn({ method: 'POST' }).handler(async (): Promise<void> => {
  const { markNotificationsRead } = await import('../notifications')
  await markNotificationsRead()
})

const notificationsQuery = queryOptions({
  queryKey: ['notifications'],
  queryFn: () => getNotificationsFn(),
})

export const Route = createFileRoute('/notifications')({
  loader: ({ context }) => context.queryClient.ensureQueryData(notificationsQuery),
  component: NotificationsPage,
})

function NotificationsPage() {
  const queryClient = useQueryClient()
  const { data: notifications } = useSuspenseQuery(notificationsQuery)

  useEffect(() => {
    void (async () => {
      await markReadFn()
      queryClient.invalidateQueries({ queryKey: ['notifications-unread'] })
      // The next visit shows these as read; this render keeps the unread highlights.
      queryClient.invalidateQueries({ queryKey: ['notifications'], refetchType: 'none' })
    })()
  }, [queryClient])

  return <NotificationsView notifications={notifications} LinkComponent={LinkAdapter} />
}
