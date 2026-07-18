# Mac App Store (+ the unified `app.vegify` bundle id)

The desktop app ships to the Mac App Store from the same Tauri crate as the `.dmg` and the iOS app — one Rust core, one set of shared screens, now THREE Apple artifacts per released merge: the Developer ID `.dmg` (direct download, unsandboxed), the Mac App Store `.pkg` (sandboxed), and the iOS `.ipa`. iOS and macOS share the single bundle identifier **`app.vegify`** on ONE App Store Connect app record, which is what makes the listing a **universal purchase** (one product page; a user gets both platforms).

## Why `app.vegify` (and what it cost)

The identifiers used to be split (`app.vegify.desktop` for the mac builds, `app.vegify.ios` for iOS). Universal purchase requires the macOS and iOS binaries to share one bundle id on one app record — and App Store Connect **locks a bundle id permanently once a build has been uploaded**, so the live iOS record under `app.vegify.ios` could never be renamed. Unifying therefore meant a NEW app record under `app.vegify` (the reverse-DNS of vegify.app) and retiring the old iOS record. That trade was taken deliberately days after the iOS launch, when the old record had ~nothing to lose. Two literals deliberately did NOT change: the keychain service name and the app-data dir name both stay `app.vegify.desktop` — they are opaque stable keys, and renaming them would log out / strand the cache of every existing install for zero functional gain (see `data.rs` / `crates/vegify-config`).

## How the lane works (`deploy.yml`: build-mas → publish-mas)

- **build-mas** (released pushes only) builds the SAME app with a config overlay: `tauri.version.json` (tag-derived version) deep-merged with `src-tauri/tauri.mas.json`, which swaps in `Entitlements.mas.plist` (App Sandbox + network client + user-selected files + the same associated domains as the Developer ID build, plus the MAS identity trio) and embeds the Mac App Store provisioning profile at `Contents/embedded.provisionprofile`.
- Signing has no cloud path outside xcodebuild, so unlike iOS the certs are real files: the **Apple Distribution** + **Mac Installer Distribution** certs come from the shared MAS signing secret (Secrets Manager, id from the `/vegify/deploy/mas-secret-id` decision) and are imported into an ephemeral keychain (`security import` + `set-key-partition-list` — the incantation that lets codesign/productbuild use the keys without a UI prompt no runner could answer).
- The job **refuses to stage an unsandboxed build** (`codesign -d --entitlements - | grep app-sandbox`), wraps the `.app` with `xcrun productbuild --sign <installer identity>`, and stages `vegify-mas.pkg` as a workflow artifact. MAS builds are deliberately **NOT notarized** (App Store ingestion runs its own checks) and **NOT attached to the GitHub Release** (a MAS-signed pkg only runs with a store receipt — it is submission material, not a download).
- **publish-mas** mirrors publish-ios: after deploy-server is live, `xcrun altool --upload-app --type macos` pushes the pkg to App Store Connect with the shared ASC API key, duplicate-tolerant for `just redeploy`.
- Putting a build ON THE STORE stays a deliberate ASC-side step (create the version, attach the build, submit for review) — uploads land in TestFlight/processing only. lux's `appstore-submit.ts` (ASC REST API, submits both platforms in one run) is the model for automating that later.

## Sandbox surface (what the entitlements cover)

HTTPS to api.vegify.app (`network.client`), the WKWebView file input for recipe photos (`files.user-selected.read-only` → Powerbox), keyring's session item (`keychain-access-groups`), Password AutoFill + universal links (`associated-domains`, same as the dmg). The tray icon, deep links (`vegify://`), and the container-relative SQLite cache need nothing extra. `ITSAppUsesNonExemptEncryption=false` + `LSApplicationCategoryType` ride in via `src-tauri/Info.plist` (tauri merges it into both mac bundles; a MAS upload without a category is rejected).

## One-time cutover runbook (John, ordered — the middle is irreversible)

**A. Apple Developer portal + account plumbing (safe any time before the merge):**

1. Register the explicit App ID `app.vegify` (Identifiers → +) with **Associated Domains** enabled. Registration failing = the id is somehow taken (near-impossible for a domain you own) — stop and reconsider the name BEFORE anything uploads under it.
2. Create two provisioning profiles for `app.vegify`: a **Developer ID** profile (against the Developer ID Application cert in `lux/apple-signing`) — hold it for step C1 — and a **Mac App Store** distribution profile (against the Apple Distribution cert in `lux/mas-signing`) → base64 → new repo secret `VEGIFY_MAS_PROVISION_PROFILE_B64` (safe to add early; nothing reads it until the merged workflow runs).
3. Point the pipeline at the MAS certs: `aws secretsmanager describe-secret --secret-id lux/mas-signing --region us-west-1` to confirm it exists, then `just config-set mas-secret-id lux/mas-signing` and `gh variable set MAS_SIGNING_SECRET_ID -b lux/mas-signing` (the deploy-ci bootstrap variable). No new certs needed — they're Apple-account-wide and lux already ships MAS with them.
4. Widen the signing role's grant BEFORE the first cascade needs it: from this branch, `cd infra && APPLE_SIGNING_SECRET_ID=lux/apple-signing MAS_SIGNING_SECRET_ID=lux/mas-signing pnpm exec cdk --app 'tsx bin/ci.ts' deploy VegifyCi` (the documented VegifyCi bootstrap exception to "never hand-run cdk deploy"). Skipping this makes the merge's build-mas race deploy-ci for the grant — the exact AccessDenied race lux hit — recoverable with `just redeploy vX.Y.Z`, but why race.

**B. App Store Connect — the irreversible record swap (one sitting):**

5. Old app (bundle `app.vegify.ios`): Pricing & Availability → **Remove from Sale**, then App Information → **Delete App**. Deleting is what releases the NAME "Vegify" (mere removal holds it). Stated plainly: the old record's store URL and any reviews/analytics are gone forever, installed copies keep working but can never update, and `app.vegify.ios` is burned as a bundle id. Four days post-launch this is the cheapest it will ever be.
6. Immediately (the freed name is world-claimable in the gap) create the NEW record: platform iOS, name **Vegify**, bundle id **app.vegify**, a FRESH SKU (deleted SKUs can't be reused). Then App Information → **Add Platform → macOS** on the same record — same record + same bundle id + same SKU IS universal purchase. If the name is somehow still held, create under a temp name and rename with the first version submission.
7. Re-enter the listing surface on the new record: description, keywords, privacy labels (email, user content, messages), age rating, support/marketing URL `https://vegify.app`, the review demo account; re-upload the iPhone 6.5"/6.9" + iPad 13" screenshot sets (the 2026-07-10 in-use sets). macOS needs its OWN screenshots (2560×1600 or 2880×1800 work) — shoot the desktop app in use, same majority-in-use rule that got iOS through review.

**C. The repo cutover (merge day — keep the window tight):**

8. Replace `VEGIFY_PROVISION_PROFILE_B64` with the NEW Developer ID profile from step 2, then **merge this PR immediately after**. The profile and the identifier the tree builds must flip together: between the swap and the merge, any other released merge would embed a profile that doesn't match its bundle id — a dmg amfid refuses to launch. (Same hazard mirrored: merging BEFORE swapping breaks the new-id dmg.)
9. The merge's cascade cuts vX.Y.Z and ships everything under `app.vegify`: build-desktop (dmg, new Developer ID profile), build-ios (cloud signing mints the App Store profile for the new App ID on the fly), build-mas (first store pkg) → deploy-server → publish-web ∥ publish-desktop ∥ publish-ios ∥ publish-mas. All 13 jobs green = dmg on the Release, `.ipa` + `.pkg` processing on the new record.
10. Verify: both builds appear under the record's TestFlight (iOS + macOS tabs); install the macOS TestFlight build and smoke the sandbox — sign in (keychain), browse/pull, upload a recipe photo (file picker), a `vegify://` deep link, autofill on the login form.
11. Submit BOTH platform versions for review. Universal purchase is live once both are approved (they can be submitted/approved independently — iOS first restores the store presence fastest).

**Post-cutover notes:** the AASA now authorizes `app.vegify` plus both legacy ids (existing installs keep webcredentials/applinks; Apple's CDN caches the AASA up to ~a day, so associated-domain features on brand-new installs can lag briefly). Existing dmg users who download a new-id build keep their local cache and session (stable literals, above) — at most macOS shows one keychain consent prompt because the accessing app's identity changed. MAS and dmg builds share the bundle id and can't coexist in /Applications; a user picks a channel.

## Deferred

- Port lux's `appstore-submit.ts` + store-notes flow (ASC REST review submission for both platforms as a dispatchable workflow) — today the version/submit step is deliberate ASC UI work.
- iOS universal links (`appLink: true` mobile config) — unchanged from docs/ios.md, AASA side ready.
