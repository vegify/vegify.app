# iOS app (Tauri) — scaffold

The iOS app is the same Tauri crate as the desktop app (`apps/desktop`), not a separate shell: one Rust core (`vegify_lib`), one set of shared `@vegify/ui` screens, with `gen/apple/` holding the generated Xcode project. Bundle identifier is **`app.vegify.ios`** (set in `src-tauri/tauri.ios.conf.json`, merged over `tauri.conf.json` — the desktop keeps `app.vegify.desktop`), and the AASA already authorizes it for webcredentials + applinks.

## Running it

```sh
xcrun simctl list devices available          # pick a simulator
cd apps/desktop && pnpm exec tauri ios dev 'iPhone 17 Pro'
```

`tauri ios dev` starts the Vite dev server (port 1420, same as desktop dev), builds the Rust core for `aarch64-apple-ios-sim` inside the Xcode build, then boots the simulator and installs + launches the app. The simulator shares the Mac's loopback, so `devUrl: http://localhost:1420` works as-is. First build compiles the whole dependency tree for the sim target — expect minutes; after that it's incremental.

Prereqs (all present on this machine): full Xcode with an iOS runtime, CocoaPods, and the rustup targets `aarch64-apple-ios` + `aarch64-apple-ios-sim`.

## How the entry point works

`run()` in `src-tauri/src/lib.rs` is the whole app; `#[cfg_attr(mobile, tauri::mobile_entry_point)]` exposes it as the static-lib entry the Xcode project launches, and desktop's `main.rs` is a three-line shim calling it. Consequence: `tauri::generate_context!` now compiles with the lib, so `cargo run --example gen_bindings` needs `../dist` to exist first — any `pnpm --filter desktop build` provides it, and `just check` orders that automatically.

## Platform gates (why they exist)

- **Menu-bar tray** — `#[cfg(desktop)]` inside `run()`'s setup; iOS has no menu bar.
- **fltsci tauri-plugin-tracing** — desktop-only *by necessity*: 0.3.4 (latest) ships an iOS code path calling `swift_rs::swift!` without declaring the `swift-rs` dependency, so the crate cannot compile for iOS/Android. It's a target-gated dependency in `Cargo.toml`; mobile installs a plain `tracing_subscriber::fmt()` instead, so `info!`/`debug!` reach the simulator/device console (visible in the `tauri ios dev` log stream). The JS `takeoverConsole()` call rejects harmlessly on iOS (no console bridge there until upstream compiles for mobile).
- **Capabilities** — split into `capabilities/default.json` (all platforms: `core:default`, `deep-link:default`) and `capabilities/desktop.json` (`platforms: [macOS, windows, linux]`, carrying `tracing:default`), because a capability referencing a permission that doesn't exist on the target fails the build.

## Deep links on iOS

`plugins.deep-link.mobile: [{ "scheme": ["vegify"] }]` in `tauri.conf.json` registers the custom scheme: the plugin's build script injects `CFBundleURLTypes` into `gen/apple/vegify_iOS/Info.plist` during `tauri ios` builds (it only touches `gen/apple` — it never modifies the desktop `Entitlements.plist`, verified in the plugin source). The App.tsx handler (`getCurrent` + `onOpenUrl`) is platform-independent and works unchanged. Universal links on iOS are a follow-up: add an `appLink: true` entry with `host: vegify.app` to the `mobile` config (the plugin then manages the iOS associated-domains entitlement itself) — the AASA side is already in place.

## Versioning

`gen/apple/vegify_iOS/Info.plist` bakes the version at scaffold time; the release-please `extra-files` sync that used to keep it current is retired (versions are git-tag-derived now), so its value is frozen — inject the real version at archive time when device/App Store builds become real (the desktop already does this via `deploy.yml`'s `tauri.version.json`).

## App Store / TestFlight

iOS ships **on the release cascade** (`deploy.yml`, since the App Store Connect side proved out 2026-07-06): `build-ios` archives + exports the signed `.ipa` at build time and stages it as a workflow artifact, and `publish-ios` uploads it to TestFlight only after `deploy-server` is live — TestFlight has no draft state, so this sequencing is what keeps a failed deploy away from testers. It reuses the SAME Apple ASC API key the desktop notarization uses (Secrets Manager via SSM) — no new secret — and signs via Xcode **cloud (automatic) signing** keyed by the team id + ASC key, so there's no distribution cert/profile to manage in the repo. Builds skip the per-build export-compliance question via `ITSAppUsesNonExemptEncryption=false` in Info.plist (the app's only crypto is TLS + the OS keychain). `just redeploy vX.Y.Z` re-runs a tag end-to-end; a build already on ASC makes `publish-ios` a tolerated no-op.

**One-time App Store Connect setup (John):**
1. Create the app record for bundle id **`app.vegify.ios`**.
2. Confirm the ASC API key in Secrets Manager has the **App Manager** role (it signs + uploads).
3. Fill the App Store listing surface that lives outside code: **App Privacy** labels (email, user content, messages), screenshots, description, age rating, and set the **support/marketing URLs** to `https://vegify.app` (the /terms + /privacy pages are live). Provide a **demo account** or open signups for review.

Then every released merge uploads a TestFlight build automatically (the version is the release tag's). Device release builds use the per-user app-data DB dir and the real iOS keychain (the mock is debug-only), so no device-specific code path is needed for the shipped app.

## Resolved gaps

- **Keychain**: the mock is `#[cfg(any(test, debug_assertions))]` only — **release device builds use the real iOS keychain** (keyring 3's `apple-native` supports `aarch64-apple-ios`). The service name stays `app.vegify.desktop` (an opaque keychain key; renaming would log out existing desktop users for no functional gain).
- **DB path**: release builds use `dirs::data_dir()` (the app's sandbox on device) — the repo `.data/vegify.db` path is `#[cfg(debug_assertions)]`, i.e. simulator-only, and never ships.
- **Safe areas**: `viewport-fit=cover` + `env(safe-area-inset-*)` padding (index.html + styles.css) so content clears the notch/home indicator.
- **Version**: injected at build time by the workflow's `--config` merge (like the desktop), so the frozen Info.plist value is overridden.

## Still deferred (not review blockers)

- Universal links (custom-scheme deep links work; add an `appLink: true` mobile entry to finish — AASA is already in place).
- iOS-native photo capture (the WKWebView file input covers photo upload).

## Icons

Brand master `packages/ui/brand/app-icon.png` → `pnpm exec tauri icon ../../packages/ui/brand/app-icon.png` fills `gen/apple/Assets.xcassets/AppIcon.appiconset` (and `icons/android/` for later). Never hand-edit the generated sets.
