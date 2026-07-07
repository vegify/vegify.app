// The client-side auth gate's PATH POLICY, factored out of routes/__root.tsx so it can be unit-tested in
// isolation and so "is this path reachable logged-out?" has exactly one home. This is the UX layer only:
// the backend is the real access control — Axum scopes every /api/content read to the session and 401s
// without one (see api.ts) — so a miss here degrades UX (wrong redirect) but never leaks data.

// Auth and token pages render bare (no app shell): the auth forms plus the email-link actions.
export const BARE_PATHS = new Set([
  "/login",
  "/signup",
  "/forgot",
  "/reset",
  "/verify",
]);

// Editorial/marketing sections that render bare with their OWN chrome regardless of session — the
// blog carries its own header/footer (like the landing at "/", which is special-cased on `user` in
// __root). Without this, a signed-in visitor gets double chrome: the AppShell sidebar + search bar
// wrapped around the blog's header — two logos on screen (caught on the live page, 2026-07-01).
export const BARE_PREFIXES: readonly string[] = ["/blog", "/terms", "/privacy"];
export const isBarePath = (pathname: string): boolean =>
  BARE_PATHS.has(pathname) ||
  BARE_PREFIXES.some((p) => pathname === p || pathname.startsWith(`${p}/`));

// Pages reachable without a session. "/" is the public marketing landing for logged-out visitors (and
// crawlers) AND the app home for signed-in users — the index route branches on `user` — so it is public
// but never redirected in either direction. Every other STATIC route is gated (see STATIC_TOP_LEVEL).
export const PUBLIC_PATHS = new Set([
  "/",
  "/terms",
  "/privacy",
  "/download",
  ...BARE_PATHS,
]);

// Auth FORMS a signed-in user has no use for → bounce them to "/". Deliberately NOT /verify or /reset:
// those consume a one-time token from an email link and must work even while signed in. (A signed-in user
// who clicks a verification link would otherwise be redirected to "/" before the token is consumed, so
// the email never gets marked verified and the banner never clears.)
export const BOUNCE_WHEN_AUTHED = new Set(["/login", "/signup", "/forgot"]);

// Fail-closed profile detection. Public profiles live at the apex: "/<username>". `$username` is the ONLY
// dynamic top-level route, so every other top-level path is a STATIC route with a file in ./routes. We
// derive that static set straight from the route files rather than hand-listing the gated ones: drop in
// routes/dashboard.tsx and "/dashboard" is gated automatically — there is no denylist to forget. (The
// backend's handles.rs reservation is the OTHER half of the contract: it stops a user from claiming a
// username that shadows one of these static routes. It does not gate anything here.)
//
// __VEGIFY_ROUTE_FILES__ is the literal list of files in ./routes, injected at build time by vite.config
// (define). Reading filenames — never importing the route modules — keeps this off the route-tree import
// graph (importing the tree is circular: it imports __root, which imports this) and emits no chunk warnings.
declare const __VEGIFY_ROUTE_FILES__: string[];
export const STATIC_TOP_LEVEL: ReadonlySet<string> = new Set(
  __VEGIFY_ROUTE_FILES__
    // "recipes.$recipeId.edit.tsx" → "recipes": keep route files, drop the extension, take the first segment.
    .filter((file) => /\.(ts|tsx)$/.test(file))
    .map((file) => {
      const cleaned = file.replace(/\.(ts|tsx)$/, "");
      return cleaned.split(".")[0] ?? cleaned;
    })
    // Drop the layout root, the "/" index, and dynamic ($) / pathless (_) segments — none is a gated
    // top-level section a logged-out visitor could mistake for a profile.
    .filter(
      (seg) =>
        seg !== "__root" &&
        seg !== "index" &&
        !seg.startsWith("$") &&
        !seg.startsWith("_"),
    )
    .map((seg) => `/${seg}`),
);

// App sections a logged-out visitor may browse READ-ONLY: the recipe + ingredient catalog and the detail
// pages under them, plus the blog (pure public content — the SEO/GEO writing surface). Create/edit children
// (/…/new, /…/<id>/edit) and every other gated section (e.g. /settings) still require a session — signing
// in is what unlocks writing. Mirrors the desktop, which hides the same New/Edit affordances logged-out;
// the shared screens gate those on the session in both shells.
export const PUBLIC_SECTIONS: readonly string[] = [
  "/recipes",
  "/ingredients",
  "/blog",
];

// A create/edit leaf under a public section — gated even though its section is public.
const isWritePath = (pathname: string): boolean =>
  /\/(new|edit)$/.test(pathname);

// True for a public section's list ("/recipes") or a detail page ("/recipes/<id>"), but never its write leaves.
const inPublicSection = (pathname: string): boolean =>
  PUBLIC_SECTIONS.some((s) => pathname === s || pathname.startsWith(`${s}/`)) &&
  !isWritePath(pathname);

// A handle is one [a-z0-9-] segment leading with an alphanumeric — a superset of the backend's username
// rules (handles.rs), which is fine: a non-handle single segment that isn't a static route just renders
// "profile not found".
const HANDLE_RE = /^\/[a-z0-9][a-z0-9-]*$/;
// A canonical recipe URL: /<handle>/<recipe-slug> (two [a-z0-9-] segments). Public read, like the
// profile; the leading segment must be a handle, not a static section (so /recipes/<id> is excluded
// here — it's covered by inPublicSection, which also gates the /new and /edit write leaves).
const PROFILE_RECIPE_RE = /^\/[a-z0-9][a-z0-9-]*\/[a-z0-9][a-z0-9-]*$/;
// Reachable logged-out iff: an explicit public page, OR inside a public catalog section (read-only), OR a
// "/<username>" profile, OR a "/<username>/<recipe-slug>" recipe — where the handle isn't a static route.
export const isPublicPath = (pathname: string): boolean =>
  PUBLIC_PATHS.has(pathname) ||
  inPublicSection(pathname) ||
  (HANDLE_RE.test(pathname) && !STATIC_TOP_LEVEL.has(pathname)) ||
  (PROFILE_RECIPE_RE.test(pathname) &&
    !STATIC_TOP_LEVEL.has(`/${pathname.split("/")[1]}`));
