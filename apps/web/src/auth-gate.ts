// The client-side auth gate's PATH POLICY, factored out of routes/__root.tsx so it can be unit-tested in
// isolation and so "is this path reachable logged-out?" has exactly one home. This is the UX layer only:
// the backend is the real access control — Axum scopes every /api/content read to the session and 401s
// without one (see api.ts) — so a miss here degrades UX (wrong redirect) but never leaks data.

// Auth and token pages render bare (no app shell): the auth forms plus the email-link actions.
export const BARE_PATHS = new Set(['/login', '/signup', '/forgot', '/reset', '/verify'])

// Pages reachable without a session. "/" is the public marketing landing for logged-out visitors (and
// crawlers) AND the app home for signed-in users — the index route branches on `user` — so it is public
// but never redirected in either direction. Every other STATIC route is gated (see STATIC_TOP_LEVEL).
export const PUBLIC_PATHS = new Set(['/', ...BARE_PATHS])

// Auth FORMS a signed-in user has no use for → bounce them to "/". Deliberately NOT /verify or /reset:
// those consume a one-time token from an email link and must work even while signed in. (A signed-in user
// who clicks a verification link would otherwise be redirected to "/" before the token is consumed, so
// the email never gets marked verified and the banner never clears.)
export const BOUNCE_WHEN_AUTHED = new Set(['/login', '/signup', '/forgot'])

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
declare const __VEGIFY_ROUTE_FILES__: string[]
export const STATIC_TOP_LEVEL: ReadonlySet<string> = new Set(
  __VEGIFY_ROUTE_FILES__
    // "recipes.$recipeId.edit.tsx" → "recipes": keep route files, drop the extension, take the first segment.
    .filter((file) => /\.(ts|tsx)$/.test(file))
    .map((file) => file.replace(/\.(ts|tsx)$/, '').split('.')[0])
    // Drop the layout root, the "/" index, and dynamic ($) / pathless (_) segments — none is a gated
    // top-level section a logged-out visitor could mistake for a profile.
    .filter((seg) => seg !== '__root' && seg !== 'index' && !seg.startsWith('$') && !seg.startsWith('_'))
    .map((seg) => `/${seg}`),
)

// A handle is one [a-z0-9-] segment leading with an alphanumeric — a superset of the backend's username
// rules (handles.rs), which is fine: a non-handle single segment that isn't a static route just renders
// "profile not found". A path is reachable logged-out iff it is an explicit public page, OR it looks like a
// handle AND is not one of our static routes (i.e. it is a "/<username>" profile).
const HANDLE_RE = /^\/[a-z0-9][a-z0-9-]*$/
export const isPublicPath = (pathname: string): boolean =>
  PUBLIC_PATHS.has(pathname) || (HANDLE_RE.test(pathname) && !STATIC_TOP_LEVEL.has(pathname))
