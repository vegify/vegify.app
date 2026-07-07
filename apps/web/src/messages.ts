// Typed view of the backend's /api/messages/* (1:1 DMs) — the messaging sibling of content.ts.
// Server-only: everything goes through api(), which attaches the session Bearer from the httpOnly
// cookie. The VM types are the shared screens' types — the server's JSON is shaped for them verbatim
// (viewer-relative `mine`/`lastIsMine` flags are computed server-side).

import type {
  ConversationSummary,
  ThreadMessage,
  ThreadVM
} from "@vegify/ui/messages"

import { api } from "./api"

/** The viewer's conversations, most recently active first. */
export function listConversations(): Promise<ConversationSummary[]> {
  return api<ConversationSummary[]>("/api/messages/conversations")
}

/** The thread with a user (by handle), oldest first. Opening it marks their messages read server-side. */
export function getThread(withUsername: string): Promise<ThreadVM> {
  return api<ThreadVM>(
    `/api/messages/thread?with=${encodeURIComponent(withUsername)}`
  )
}

/** Send a DM (creates the conversation on first contact). Returns the stored message. */
export function sendMessage(to: string, body: string): Promise<ThreadMessage> {
  return api<ThreadMessage>("/api/messages/send", {
    method: "POST",
    body: { to, body }
  })
}

/** Total unread across conversations — the chrome badge number. */
export async function unreadCount(): Promise<number> {
  const { count } = await api<{ count: number }>("/api/messages/unread")
  return count
}
