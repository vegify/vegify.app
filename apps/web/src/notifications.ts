// Typed view of the backend's /api/notifications (the bell) — sibling of messages.ts. Server-only.

import type { NotificationVM } from "@vegify/ui/notifications";
import { api } from "./api";

/** The viewer's recent notifications, newest first. */
export function listNotifications(): Promise<NotificationVM[]> {
  return api<NotificationVM[]>("/api/notifications");
}

/** Unread count — the bell badge number. */
export async function unreadNotifications(): Promise<number> {
  const { count } = await api<{ count: number }>("/api/notifications/unread");
  return count;
}

/** Read everything (fired when the notifications page opens — bell-standard semantics). */
export function markNotificationsRead(): Promise<void> {
  return api<void>("/api/notifications/read", { method: "POST", body: {} });
}
