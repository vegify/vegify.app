/**
 * MESSAGES — the shared 1:1 DM screens, rendered by BOTH shells (the screens are pure view + ports:
 * data arrives as a VM, navigation through the injected LinkComponent, actions through callback
 * props — no fetching, no routing in here; see screens.tsx's contract note).
 *
 * The VM shapes mirror the server's /api/messages/* JSON verbatim (services/api/src/messages.rs):
 * `mine`/`lastIsMine` are viewer-relative flags computed server-side, so the screens never compare
 * user ids. Threads are addressed by public handle — a profile's "Message" button links to
 * /messages/<username>, which renders an empty composer even before a conversation exists.
 */
import { type ComponentType, useEffect, useRef, useState } from "react";

import type { AppShellLinkProps } from "./app-shell";
import { buttonClasses } from "./button";
import { cn } from "./cn";

type NavLink = ComponentType<AppShellLinkProps>;

export type MessageParty = {
  id: string;
  name: string;
  username: string;
};

export type ConversationSummary = {
  id: string;
  with: MessageParty;
  lastBody: string;
  lastAt: number;
  lastIsMine: boolean;
  unread: number;
};

export type ThreadMessage = {
  id: string;
  body: string;
  createdAt: number;
  mine: boolean;
};

export type ThreadVM = {
  with: MessageParty;
  messages: ThreadMessage[];
};

/** Short local timestamp: time for today, month+day otherwise (chat-list style). */
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

export function MessagesView({
  conversations,
  LinkComponent,
}: {
  conversations: ConversationSummary[];
  LinkComponent: NavLink;
}) {
  return (
    <div className="mx-auto max-w-3xl p-8">
      <div className="mb-8">
        <h1 className="mb-1 font-bold font-serif text-4xl text-primary-dark">
          Inbox
        </h1>
        <p className="text-gray-500">
          {conversations.length === 1
            ? "1 conversation"
            : `${conversations.length} conversations`}
        </p>
      </div>
      {conversations.length === 0 ? (
        <p className="text-muted-foreground">
          No messages yet — say hi from someone's profile.
        </p>
      ) : (
        <div className="flex flex-col gap-3">
          {conversations.map((c) => (
            <LinkComponent
              key={c.id}
              href={`/messages/${c.with.username}`}
              className="block"
            >
              <div className="flex items-center gap-4 rounded-xl bg-card p-4 ring-1 ring-foreground/10 transition hover:ring-primary/40">
                <div className="flex size-12 shrink-0 items-center justify-center rounded-full bg-primary/10 font-bold font-serif text-primary-dark text-xl uppercase">
                  {c.with.name.trim().charAt(0) || "?"}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline justify-between gap-3">
                    <h3 className="truncate font-semibold font-serif text-xl">
                      {c.with.name}
                    </h3>
                    <span className="shrink-0 text-muted-foreground text-xs">
                      {shortWhen(c.lastAt)}
                    </span>
                  </div>
                  <p
                    className={cn(
                      "truncate text-sm",
                      c.unread > 0
                        ? "font-semibold text-foreground"
                        : "text-muted-foreground",
                    )}
                  >
                    {c.lastIsMine ? "You: " : ""}
                    {c.lastBody}
                  </p>
                </div>
                {c.unread > 0 ? (
                  <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-orange font-bold text-white text-xs">
                    {c.unread > 99 ? "99+" : c.unread}
                  </span>
                ) : null}
              </div>
            </LinkComponent>
          ))}
        </div>
      )}
    </div>
  );
}

export function ThreadView({
  thread,
  onSend,
  sending = false,
  LinkComponent,
}: {
  thread: ThreadVM;
  /** Send the composer's text. The shells own the mutation (and refetch on settle). */
  onSend: (body: string) => void;
  /** True while a send is in flight — disables the composer so a message can't double-send. */
  sending?: boolean;
  LinkComponent: NavLink;
}) {
  const [draft, setDraft] = useState("");
  const endRef = useRef<HTMLDivElement>(null);

  // Keep the newest message in view — on open and whenever one lands (ours or theirs via refetch).
  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [thread.messages.length]);

  const submit = () => {
    const body = draft.trim();
    if (!body || sending) return;
    onSend(body);
    setDraft("");
  };

  return (
    <div className="mx-auto flex h-full max-w-3xl flex-col p-8">
      <header className="mb-6 flex items-center gap-4">
        <LinkComponent
          href="/messages"
          className="text-muted-foreground text-sm hover:text-foreground"
        >
          ← Inbox
        </LinkComponent>
        <LinkComponent
          href={`/${thread.with.username}`}
          className="flex min-w-0 items-center gap-3"
        >
          <div className="flex size-10 shrink-0 items-center justify-center rounded-full bg-primary/10 font-bold font-serif text-lg text-primary-dark uppercase">
            {thread.with.name.trim().charAt(0) || "?"}
          </div>
          <div className="min-w-0">
            <h1 className="truncate font-bold font-serif text-2xl text-primary-dark">
              {thread.with.name}
            </h1>
            <p className="truncate text-muted-foreground text-sm">
              @{thread.with.username}
            </p>
          </div>
        </LinkComponent>
      </header>

      <div className="flex-1 space-y-3 overflow-y-auto pb-4">
        {thread.messages.length === 0 ? (
          <p className="pt-8 text-center text-muted-foreground">
            Say hi — this is the start of your conversation with{" "}
            {thread.with.name}.
          </p>
        ) : (
          thread.messages.map((m) => (
            <div
              key={m.id}
              className={cn("flex", m.mine ? "justify-end" : "justify-start")}
            >
              <div
                className={cn(
                  "max-w-[75%] rounded-2xl px-4 py-2.5",
                  m.mine
                    ? "rounded-br-sm bg-primary text-primary-foreground"
                    : "rounded-bl-sm bg-card ring-1 ring-foreground/10",
                )}
              >
                <p className="whitespace-pre-wrap break-words text-sm">
                  {m.body}
                </p>
                <p
                  className={cn(
                    "mt-1 text-right text-[0.65rem]",
                    m.mine
                      ? "text-primary-foreground/70"
                      : "text-muted-foreground",
                  )}
                >
                  {shortWhen(m.createdAt)}
                </p>
              </div>
            </div>
          ))
        )}
        <div ref={endRef} />
      </div>

      <form
        className="mt-2 flex items-end gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              submit();
            }
          }}
          rows={2}
          placeholder={`Message ${thread.with.name}…`}
          aria-label="Message"
          className="min-h-[2.75rem] flex-1 resize-none rounded-xl border border-input bg-card px-4 py-2.5 text-foreground text-sm outline-none placeholder:text-muted-foreground focus:border-primary"
        />
        <button
          type="submit"
          disabled={sending || !draft.trim()}
          className={cn(buttonClasses({ size: "sm" }), "disabled:opacity-50")}
        >
          {sending ? "Sending…" : "Send"}
        </button>
      </form>
    </div>
  );
}
