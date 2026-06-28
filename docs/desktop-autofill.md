# Desktop sign-in & password autofill

How password autofill works (and doesn't) in the macOS desktop app, why it's that way, and what's already wired on our side. The desktop shell is a Tauri app — a macOS **WKWebView** rendering the same shared `@vegify/ui` sign-in screen the web uses.

## The short version (how to use it)

On the desktop sign-in screen, fill your saved credential with **1Password's Universal Autofill**: focus a field and press **⌘\\**. The first time, 1Password shows a search box (the app has no URL for it to match on) — type "vegify", pick the login, authenticate with Touch ID, and both fields fill. 1Password remembers the app↔login association, so later ⌘\\ presses suggest it directly.

That keystroke is as close to "autofill" as macOS allows for a non-browser app. There is no in-field key icon / automatic popup the way there is in a browser — see the next section for why.

## Why it's ⌘\\ and not in-field autofill

Two independent platform limits, both confirmed empirically (2026-06-28), cap this — neither is a vegify bug, and neither is fixable in our code:

- **macOS WKWebView does not surface native Password AutoFill for HTML forms.** Apple reserves in-field autofill for Safari/SFSafariViewController; an *embedded* WKWebView (Tauri, Electron-on-wry, any host) gets no autofill UI for web content on macOS. (On **iOS** WKWebView *does* — that difference matters below.) Proof: with our entitlement and AASA perfect, `swcd` was never even invoked when interacting with the form — nothing in the WKWebView asks the OS to autofill.
- **1Password on macOS only inline-fills browsers.** Its sole mechanism for every other app — native, WKWebView, Electron — is Universal Autofill (⌘\\). It is not a system credential provider for native fields, so even a native AppKit screen wouldn't get 1Password to offer inline; it would only see iCloud Keychain.

So forking Tauri to "modernize" the webview wouldn't help (wrong layer — it's already modern WKWebView), and a native sign-in screen would only tap iCloud Keychain, not 1Password.

## Setup (one-time, in 1Password)

If ⌘\\ does nothing, the most common cause is that **1Password's autofill shortcut is unbound** (it ships unset on some installs — this bit us). Bind it:

1. 1Password → **Settings** (⌘,) → **General**.
2. Find the keyboard shortcut for autofill ("Show autofill in the active app" / Universal Autofill). If it's blank, click it and press **⌘\\**.
3. Back in the desktop app, focus a field and press it.

Universal Autofill also requires the 1Password desktop app to be running and unlocked, and to have **Accessibility** permission (System Settings → Privacy & Security → Accessibility → 1Password enabled) — it's accessibility-based, so without that permission the shortcut silently does nothing.

## What's already correct on our side

The desktop carries the pieces that make autofill work *where the platform allows it* — these are done and shouldn't be re-investigated:

- **Associated Domains entitlement** `webcredentials:vegify.app` on the signed, notarized build (authorized by an embedded Developer ID provisioning profile).
- **AASA** served at `https://vegify.app/.well-known/apple-app-site-association` as `application/json` (a CloudFront response-headers policy forces the content-type; Apple's CDN serves it correctly), authorizing `T3UN6N5K6Z.app.vegify.desktop`.
- The shared sign-in form carries proper `autocomplete` attributes (`email` + `current-password`).

This config is also exactly what a future **native iOS app** needs, where WKWebView *does* surface autofill — so it pays forward rather than being macOS-only effort.

## If we ever want zero-keystroke automatic autofill

The only way to get a credential offered inline (no ⌘\\) on the desktop is to **store the login in iCloud Keychain** and **build a native `NSSecureTextField` sign-in screen** so Apple's Passwords can fill it. That abandons 1Password for this one screen and diverges from the shared-screens architecture, so it's deliberately not done — revisit only if "automatic" outweighs "uses your existing 1Password vault".
