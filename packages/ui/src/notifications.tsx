/**
 * NOTIFICATIONS — the shared bell screen (pure VM + ports, like messages.tsx). The server's payload
 * is an opaque per-kind JSON blob: kind "message" renders "<name> sent you a message" linking into
 * the thread; unknown kinds render a generic row instead of breaking, so new server-side events
 * never require a lockstep client release.
 */

import { Bell, Salad } from "lucide-react";
import type { ComponentType } from "react";
import type { AppShellLinkProps } from "./app-shell";
import { cn } from "./cn";

type NavLink = ComponentType<AppShellLinkProps>;

/** JSON-safe value — concrete (no `unknown`) so the web's server-fn serializer accepts the VM. */
export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [k: string]: JsonValue };

export type NotificationVM = {
  id: string;
  kind: string;
  /** Per-kind payload (kind "message": `{from: {id,name,username}, preview}`). */
  payload: { [k: string]: JsonValue } | null;
  createdAt: number;
  read: boolean;
};

/** Short local timestamp: time for today, month+day otherwise. */
function shortWhen(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  return sameDay
    ? d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })
    : d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

type IngredientUpdatedPayload = {
  ingredient?: { id?: string; name?: string; slug?: string | null };
  by?: { name?: string; username?: string };
};

/** The renderable essence of one notification — also used by the desktop's native toasts.
 *  The bell is reserved for personal-impact events (DMs ring the Mail badge instead): v1's kind is
 *  "ingredient-updated" — an ingredient your recipes use changed, so their nutrition did too. */
export function describeNotification(n: NotificationVM): {
  title: string;
  detail?: string;
  href?: string;
} {
  if (n.kind === "ingredient-updated") {
    const p = (n.payload ?? {}) as IngredientUpdatedPayload;
    const who = p.by?.name ?? "Someone";
    const what = p.ingredient?.name ?? "an ingredient";
    return {
      title: `${who} updated ${what}`,
      detail: "Your recipes that use it now reflect the change.",
      href: p.ingredient?.slug
        ? `/ingredients/${p.ingredient.slug}`
        : p.ingredient?.id
          ? `/ingredients/${p.ingredient.id}`
          : undefined,
    };
  }
  return { title: "Something happened on Vegify" };
}

export function NotificationsView({
  notifications,
  LinkComponent,
}: {
  notifications: NotificationVM[];
  LinkComponent: NavLink;
}) {
  return (
    <div className="mx-auto max-w-3xl p-8">
      <div className="mb-8">
        <h1 className="mb-1 font-serif text-4xl font-bold text-primary-dark">
          Notifications
        </h1>
        <p className="text-gray-500">
          {notifications.length === 0 ? "Nothing yet" : "Your recent activity"}
        </p>
      </div>
      {notifications.length === 0 ? (
        <p className="text-muted-foreground">
          When something that affects you happens — like an ingredient your
          recipes use getting updated — it lands here.
        </p>
      ) : (
        <div className="flex flex-col gap-3">
          {notifications.map((n) => {
            const d = describeNotification(n);
            const Icon = n.kind === "ingredient-updated" ? Salad : Bell;
            const row = (
              <div
                key={n.id}
                className={cn(
                  "flex items-center gap-4 rounded-xl bg-card p-4 ring-1 transition",
                  n.read ? "ring-foreground/10" : "ring-primary/40",
                  d.href ? "hover:ring-primary/60" : "",
                )}
              >
                <span
                  className={cn(
                    "flex size-10 shrink-0 items-center justify-center rounded-full",
                    n.read
                      ? "bg-muted text-muted-foreground"
                      : "bg-primary/10 text-primary-dark",
                  )}
                >
                  <Icon className="size-5" />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline justify-between gap-3">
                    <p
                      className={cn(
                        "truncate",
                        n.read
                          ? "text-foreground"
                          : "font-semibold text-foreground",
                      )}
                    >
                      {d.title}
                    </p>
                    <span className="shrink-0 text-xs text-muted-foreground">
                      {shortWhen(n.createdAt)}
                    </span>
                  </div>
                  {d.detail ? (
                    <p className="truncate text-sm text-muted-foreground">
                      {d.detail}
                    </p>
                  ) : null}
                </div>
                {!n.read ? (
                  <span className="size-2 shrink-0 rounded-full bg-orange" />
                ) : null}
              </div>
            );
            return d.href ? (
              <LinkComponent key={n.id} href={d.href} className="block">
                {row}
              </LinkComponent>
            ) : (
              <div key={n.id}>{row}</div>
            );
          })}
        </div>
      )}
    </div>
  );
}
