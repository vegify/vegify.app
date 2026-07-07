import {
  Bell,
  Carrot,
  House,
  LogOut,
  Mail,
  Salad,
  Search,
  Settings,
  User,
} from "lucide-react";
import type { ComponentType, ReactNode } from "react";
import { cn } from "./cn";
import { VegifyLogo } from "./vegify-logo";

type IconType = ComponentType<{ className?: string; strokeWidth?: number }>;

/**
 * Link primitive each app injects (next/link or @tanstack/react-router Link)
 * so the shell JSX stays shared and the framework is the only variable.
 */
export type AppShellLinkProps = {
  href: string;
  className?: string;
  "aria-current"?: "page";
  "aria-label"?: string;
  children: ReactNode;
};

export type AppShellNavItem = {
  key: string;
  label: string;
  icon: IconType;
  /** Live route. Absent => rendered as a disabled "soon" item. */
  href?: string;
  /** Appears in the mobile bottom tab bar (icon-only). */
  inMobileBar?: boolean;
  /** Unread count — rendered as an orange pill in place of the "soon" badge slot. */
  badge?: number;
};

/**
 * Nav destinations, transcribed from the Sketch "Desktop HD" sidebar comp:
 * Home · Explore · Add Food · Notifications · Profile · Inbox · Settings.
 * The five `inMobileBar` items are the mobile bottom tab bar.
 */
export const APP_NAV: AppShellNavItem[] = [
  { key: "home", label: "Home", icon: House, href: "/", inMobileBar: true },
  {
    key: "explore",
    label: "Explore",
    icon: Search,
    href: "/recipes",
    inMobileBar: true,
  },
  {
    key: "add",
    label: "Add Food",
    icon: Carrot,
    href: "/ingredients/new",
    inMobileBar: true,
  },
  {
    key: "notifications",
    label: "Notifications",
    icon: Bell,
    href: "/notifications",
    inMobileBar: true,
  },
  { key: "profile", label: "Profile", icon: User, inMobileBar: true },
  { key: "inbox", label: "Inbox", icon: Mail, href: "/messages" },
  { key: "settings", label: "Settings", icon: Settings, href: "/settings" },
];

function pathIsActive(currentPath: string, href?: string) {
  if (!href) return false;
  if (href === "/") return currentPath === "/";
  return currentPath === href || currentPath.startsWith(`${href}/`);
}

export function AppShell({
  currentPath,
  LinkComponent,
  children,
  ingredientsNav,
  searchValue,
  onSearchChange,
  user,
  onSignOut,
  unreadMessages = 0,
  unreadNotifications = 0,
}: {
  currentPath: string;
  LinkComponent: ComponentType<AppShellLinkProps>;
  children: ReactNode;
  /** Desktop-only: inject a first-class "Ingredients" destination into the nav (web shells reach ingredients via recipe links). */
  ingredientsNav?: boolean;
  /** When provided, the chrome search becomes a controlled input (e.g. the desktop filters the active list). */
  searchValue?: string;
  onSearchChange?: (value: string) => void;
  /** The signed-in user — renders an account block + sign-out in the sidebar and mobile bar. */
  user?: { name: string; email: string; username?: string } | null;
  onSignOut?: () => void;
  /** Unread DM count — badges the Inbox nav item (sidebar) and the mobile header's Mail link. */
  unreadMessages?: number;
  /** Unread notification count — badges the Bell (sidebar + mobile tab bar). */
  unreadNotifications?: number;
}) {
  // The Profile destination is per-user (/<username>): filled in from the signed-in user. Logged
  // out it points at /login instead of sitting disabled — every tap on the person icon has a
  // destination, and both shells route /login as a plain screen ("a destination, not a gate").
  // Applied to the sidebar + the mobile bar.
  const profileHref = user?.username ? `/${user.username}` : "/login";
  const withProfile = (items: AppShellNavItem[]) =>
    items.map((it) => {
      if (it.key === "profile") return { ...it, href: profileHref };
      if (it.key === "inbox") return { ...it, badge: unreadMessages };
      if (it.key === "notifications")
        return { ...it, badge: unreadNotifications };
      return it;
    });
  const navItems = withProfile(
    ingredientsNav
      ? [
          ...APP_NAV.slice(0, 2),
          {
            key: "ingredients",
            label: "Ingredients",
            icon: Salad,
            href: "/ingredients",
          },
          ...APP_NAV.slice(2),
        ]
      : APP_NAV,
  );
  const mobileItems = withProfile(APP_NAV).filter((item) => item.inMobileBar);
  return (
    <div className="flex h-screen flex-col overflow-hidden bg-background text-foreground lg:flex-row">
      {/* ===== Desktop sidebar ===== */}
      <aside className="hidden bg-green-dark text-white lg:flex lg:h-screen lg:w-72 lg:shrink-0 lg:flex-col lg:overflow-y-auto">
        <LinkComponent href="/" className="flex items-center px-6 py-8">
          <VegifyLogo className="h-auto w-full" />
        </LinkComponent>
        <nav className="flex flex-1 flex-col gap-1 px-3 py-4">
          {navItems.map((item) => (
            <NavRow
              key={item.key}
              item={item}
              active={pathIsActive(currentPath, item.href)}
              LinkComponent={LinkComponent}
            />
          ))}
        </nav>
        <div className="mt-auto space-y-3 px-3 pb-5">
          {user ? (
            <div className="flex items-center gap-3 rounded-lg bg-white/5 px-3 py-2">
              <div className="flex size-9 shrink-0 items-center justify-center rounded-full bg-white/20 text-sm font-bold uppercase">
                {user.name.trim().charAt(0) || "?"}
              </div>
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-semibold leading-tight">
                  {user.name}
                </p>
                <p className="truncate text-xs leading-tight text-white/70">
                  {user.email}
                </p>
              </div>
              {onSignOut ? (
                <button
                  type="button"
                  onClick={onSignOut}
                  aria-label="Sign out"
                  className="flex size-8 shrink-0 items-center justify-center rounded-lg text-white/80 transition hover:bg-white/10 hover:text-white"
                >
                  <LogOut className="size-5" />
                </button>
              ) : null}
            </div>
          ) : (
            <LinkComponent
              href="/login"
              className="flex items-center justify-center gap-2 rounded-lg bg-white/10 px-3 py-2 text-sm font-semibold text-white transition hover:bg-white/20"
            >
              <User className="size-5" />
              Sign in
            </LinkComponent>
          )}
        </div>
      </aside>

      {/* ===== Mobile top bar ===== */}
      <header className="flex h-14 shrink-0 items-center justify-between bg-green-dark px-4 text-white lg:hidden">
        <div className="flex items-center gap-4">
          <LinkComponent href="/settings" aria-label="Settings">
            <Settings className="size-6" />
          </LinkComponent>
          <LinkComponent
            href="/messages"
            aria-label="Messages"
            className="relative"
          >
            <Mail className="size-6" />
            {unreadMessages > 0 ? (
              <span className="absolute -right-1.5 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-orange px-1 text-[0.6rem] font-bold leading-none">
                {unreadMessages > 99 ? "99+" : unreadMessages}
              </span>
            ) : null}
          </LinkComponent>
        </div>
        <VegifyLogo className="h-6 w-auto" />
        {user ? (
          <button type="button" onClick={onSignOut} aria-label="Sign out">
            <LogOut className="size-6" />
          </button>
        ) : (
          <LinkComponent href="/login" aria-label="Sign in">
            <User className="size-6" />
          </LinkComponent>
        )}
      </header>

      {/* ===== Content ===== */}
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <div className="hidden shrink-0 items-center px-8 py-4 lg:flex">
          <div className="relative mx-auto w-full max-w-xl">
            <input
              type="search"
              aria-label="Search"
              placeholder="Search…"
              className="h-11 w-full rounded-full border border-input bg-card pl-5 pr-12 text-base text-foreground outline-none placeholder:text-muted-foreground focus:border-primary"
              {...(onSearchChange
                ? {
                    value: searchValue ?? "",
                    onChange: (e) => onSearchChange(e.target.value),
                  }
                : {})}
            />
            <span className="absolute right-1.5 top-1/2 flex size-8 -translate-y-1/2 items-center justify-center rounded-full bg-primary text-primary-foreground">
              <Search className="size-4" />
            </span>
          </div>
        </div>
        <main className="min-w-0 flex-1 overflow-y-auto pb-24 lg:pb-8">
          {children}
        </main>
      </div>

      {/* ===== Mobile bottom tab bar ===== */}
      <nav className="fixed inset-x-0 bottom-0 z-40 flex h-16 items-center justify-around bg-green-dark text-white lg:hidden">
        {mobileItems.map((item) => (
          <TabItem
            key={item.key}
            item={item}
            active={pathIsActive(currentPath, item.href)}
            LinkComponent={LinkComponent}
          />
        ))}
      </nav>
    </div>
  );
}

function NavRow({
  item,
  active,
  LinkComponent,
}: {
  item: AppShellNavItem;
  active: boolean;
  LinkComponent: ComponentType<AppShellLinkProps>;
}) {
  const Icon = item.icon;
  const base =
    "flex items-center gap-3 rounded-lg px-3 py-2.5 text-2xl font-semibold transition";
  const inner = (
    <>
      <Icon className="size-6 shrink-0" strokeWidth={2} />
      <span>{item.label}</span>
      {(item.badge ?? 0) > 0 ? (
        <span className="ml-auto flex h-6 min-w-6 items-center justify-center rounded-full bg-orange px-1.5 text-sm font-bold leading-none text-white">
          {(item.badge ?? 0) > 99 ? "99+" : item.badge}
        </span>
      ) : !item.href ? (
        <span className="ml-auto rounded-full bg-white/15 px-2 py-0.5 text-xs font-semibold uppercase tracking-wide">
          soon
        </span>
      ) : null}
    </>
  );
  if (!item.href) {
    return (
      <span
        aria-disabled
        className={cn(base, "cursor-not-allowed text-white/55")}
      >
        {inner}
      </span>
    );
  }
  return (
    <LinkComponent
      href={item.href}
      aria-current={active ? "page" : undefined}
      className={cn(
        base,
        active
          ? "bg-white/15 text-white"
          : "text-white/85 hover:bg-white/10 hover:text-white",
      )}
    >
      {inner}
    </LinkComponent>
  );
}

function TabItem({
  item,
  active,
  LinkComponent,
}: {
  item: AppShellNavItem;
  active: boolean;
  LinkComponent: ComponentType<AppShellLinkProps>;
}) {
  const Icon = item.icon;
  const inner = (
    <span
      className={cn(
        "relative flex size-11 items-center justify-center rounded-2xl transition",
        active ? "bg-orange text-white" : "text-white/85",
      )}
    >
      <Icon className="size-6" />
      {(item.badge ?? 0) > 0 ? (
        <span className="absolute right-0.5 top-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-orange px-1 text-[0.6rem] font-bold leading-none text-white ring-2 ring-green-dark">
          {(item.badge ?? 0) > 99 ? "99+" : item.badge}
        </span>
      ) : null}
    </span>
  );
  if (!item.href) {
    return (
      <span
        aria-disabled
        className="flex items-center justify-center opacity-55"
      >
        {inner}
      </span>
    );
  }
  return (
    <LinkComponent
      href={item.href}
      aria-current={active ? "page" : undefined}
      className="flex items-center justify-center"
    >
      {inner}
    </LinkComponent>
  );
}
