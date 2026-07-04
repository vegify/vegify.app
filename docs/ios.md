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

## Known gaps (deliberate, scaffold-scope)

- **Keychain**: `keyring` falls back to its **mock** store on iOS (log shows `MockCredential`) — sign-in works but the session token does not persist across app launches. Follow-up: real iOS keychain (and note the keyring service name is hard-coded `app.vegify.desktop` in `data.rs` — revisit both together).
- **Debug DB path**: debug builds point at the repo's seeded `.data/vegify.db`, which works on the **simulator** (sim processes see host paths) but would fail on a physical device — set `DATABASE_PATH` or use a release build there.
- **Device builds/signing**: need a development team (`APPLE_DEVELOPMENT_TEAM` env or `bundle.iOS.developmentTeam`); keep the value out of the repo like every other account identifier.
- **No iOS CI** — the sim build is local-only for now.

## Icons

Brand master `packages/ui/brand/app-icon.png` → `pnpm exec tauri icon ../../packages/ui/brand/app-icon.png` fills `gen/apple/Assets.xcassets/AppIcon.appiconset` (and `icons/android/` for later). Never hand-edit the generated sets.
