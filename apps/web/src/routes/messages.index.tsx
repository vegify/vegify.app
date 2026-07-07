import { queryOptions, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import { type ConversationSummary, MessagesView } from "@vegify/ui/messages";

import { LinkAdapter } from "../link";

// The inbox: the viewer's conversations. A static top-level route file, so the auth gate derives
// "/messages is gated" automatically (auth-gate.ts STATIC_TOP_LEVEL) — logged-out visitors bounce
// to /login before this loader ever runs.
const getConversationsFn = createServerFn({ method: "GET" }).handler(
  async (): Promise<ConversationSummary[]> => {
    const { listConversations } = await import("../messages");
    return listConversations();
  },
);

const conversationsQuery = queryOptions({
  queryKey: ["conversations"],
  queryFn: () => getConversationsFn(),
});

export const Route = createFileRoute("/messages/")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(conversationsQuery),
  component: MessagesPage,
});

function MessagesPage() {
  const { data: conversations } = useSuspenseQuery(conversationsQuery);
  return (
    <MessagesView conversations={conversations} LinkComponent={LinkAdapter} />
  );
}
