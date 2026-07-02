# Changelog

## [0.17.0](https://github.com/vegify/vegify.app/compare/v0.16.0...v0.17.0) (2026-07-02)


### Features

* **blog:** DB-backed blog CMS — posts are data, not code ([#49](https://github.com/vegify/vegify.app/issues/49)) ([84de560](https://github.com/vegify/vegify.app/commit/84de560936922b720d05bc0bbca45e384798ad19))

## [0.16.0](https://github.com/vegify/vegify.app/compare/v0.15.0...v0.16.0) (2026-07-02)


### Features

* **ui:** site footer under the nutrition label in the detail right-rail ([#47](https://github.com/vegify/vegify.app/issues/47)) ([ddc733d](https://github.com/vegify/vegify.app/commit/ddc733d85ecc0ce74debac8f797f0bebb721a176))

## [0.15.0](https://github.com/vegify/vegify.app/compare/v0.14.0...v0.15.0) (2026-07-02)


### Features

* **blog:** post [#2](https://github.com/vegify/vegify.app/issues/2) — "There's no such thing as 100%" (DRI explainer + charts) ([#44](https://github.com/vegify/vegify.app/issues/44)) ([73ce6e7](https://github.com/vegify/vegify.app/commit/73ce6e7dc94ee4f5d9faf6eac18bebc0e9fdfb96))

## [0.14.0](https://github.com/vegify/vegify.app/compare/v0.13.0...v0.14.0) (2026-07-02)


### Features

* **slugs:** Phase 4 — dynamic sitemap.xml enumerating public content ([#42](https://github.com/vegify/vegify.app/issues/42)) ([abb2678](https://github.com/vegify/vegify.app/commit/abb26782226b40dd840217e8974de36e3618350b))

## [0.13.0](https://github.com/vegify/vegify.app/compare/v0.12.0...v0.13.0) (2026-07-02)


### Features

* **slugs:** canonical /&lt;username&gt;/&lt;recipe-slug&gt; + /ingredients/&lt;slug&gt; URLs (recipes recovered + ingredients) ([#40](https://github.com/vegify/vegify.app/issues/40)) ([388ba13](https://github.com/vegify/vegify.app/commit/388ba13b9d11ad5de03e5ad97767cfa8ffec9065))

## [0.12.0](https://github.com/vegify/vegify.app/compare/v0.11.0...v0.12.0) (2026-07-02)


### Features

* **slugs:** data foundation — slug column, history, generation, backfill ([#38](https://github.com/vegify/vegify.app/issues/38)) ([fe1f519](https://github.com/vegify/vegify.app/commit/fe1f519c95b3fe58635cc5b0854f981b9a79f8d6))

## [0.11.0](https://github.com/vegify/vegify.app/compare/v0.10.1...v0.11.0) (2026-07-02)


### Features

* **ui:** inline (Linear-style) editing on recipe & ingredient detail ([#36](https://github.com/vegify/vegify.app/issues/36)) ([56d13db](https://github.com/vegify/vegify.app/commit/56d13db1584f5af5672034c1ee884a47c68a9322))

## [0.10.1](https://github.com/vegify/vegify.app/compare/v0.10.0...v0.10.1) (2026-07-02)


### Bug Fixes

* **web:** render the blog bare and fix its dark-mode and light-mode chrome ([#34](https://github.com/vegify/vegify.app/issues/34)) ([10efaf2](https://github.com/vegify/vegify.app/commit/10efaf22b6f7b2dc929db9ea486b81c9f347b855))

## [0.10.0](https://github.com/vegify/vegify.app/compare/v0.9.0...v0.10.0) (2026-07-01)


### Features

* **web:** stand up the blog with the first post ([#32](https://github.com/vegify/vegify.app/issues/32)) ([8c9762a](https://github.com/vegify/vegify.app/commit/8c9762a536dd6e2c1ece0b5d013e8af54c0ca190))

## [0.9.0](https://github.com/vegify/vegify.app/compare/v0.8.0...v0.9.0) (2026-07-01)


### Features

* **ios:** scaffold the iOS app on the shared Tauri crate ([#27](https://github.com/vegify/vegify.app/issues/27)) ([fab7965](https://github.com/vegify/vegify.app/commit/fab7965d6d6241b294029476c46fbb5d47effd48))

## [0.8.0](https://github.com/vegify/vegify.app/compare/v0.7.0...v0.8.0) (2026-07-01)


### Features

* **desktop:** open vegify:// and vegify.app links as OS deep links ([#25](https://github.com/vegify/vegify.app/issues/25)) ([0035a7a](https://github.com/vegify/vegify.app/commit/0035a7a60cb19100de678424eec932e9023c1d02))

## [0.7.0](https://github.com/vegify/vegify.app/compare/v0.6.0...v0.7.0) (2026-06-30)


### Features

* crisp app icons, vector brand mark, and a macOS menu-bar tray ([#22](https://github.com/vegify/vegify.app/issues/22)) ([a10812f](https://github.com/vegify/vegify.app/commit/a10812f72c07ccd5a35e603c0c2b39d7617c8a58))

## [0.6.0](https://github.com/vegify/vegify.app/compare/v0.5.2...v0.6.0) (2026-06-30)


### Features

* public browsing without sign-in (desktop + web) ([#16](https://github.com/vegify/vegify.app/issues/16)) ([4b0aefb](https://github.com/vegify/vegify.app/commit/4b0aefb4ad2b1b2a426c652a06f11053d360dad7))

## [0.5.2](https://github.com/vegify/vegify.app/compare/v0.5.1...v0.5.2) (2026-06-29)


### Bug Fixes

* make /&lt;username&gt; profile pages publicly viewable ([ae850bb](https://github.com/vegify/vegify.app/commit/ae850bbd14f980ae55fc08236abd94bb009adb53))

## [0.5.1](https://github.com/vegify/vegify.app/compare/v0.5.0...v0.5.1) (2026-06-29)


### Bug Fixes

* set username in db createUser, and run CI via a shared justfile ([97ff927](https://github.com/vegify/vegify.app/commit/97ff92783d6d4fcb5832e05d4a5211391903e593))

## [0.5.0](https://github.com/vegify/vegify.app/compare/v0.4.0...v0.5.0) (2026-06-29)


### Features

* usernames, profile pages, and login by email or username ([f8c7988](https://github.com/vegify/vegify.app/commit/f8c79888158dcbf2fec5c3bc4456178f1aa13fd3))

## [0.4.0](https://github.com/vegify/vegify.app/compare/v0.3.5...v0.4.0) (2026-06-29)


### Features

* add Settings page with light/dark/system theme control ([009b8b8](https://github.com/vegify/vegify.app/commit/009b8b8b042a34ed3d2dc85921c151e8548ee3bf))

## [0.3.5](https://github.com/vegify/vegify.app/compare/v0.3.4...v0.3.5) (2026-06-29)


### Bug Fixes

* **desktop:** persist the current route across a webview reload ([c2133e9](https://github.com/vegify/vegify.app/commit/c2133e9cd883a2b697053d48f04b1914493aad98))

## [0.3.4](https://github.com/vegify/vegify.app/compare/v0.3.3...v0.3.4) (2026-06-28)


### Bug Fixes

* **server:** send transactional email from hello@ instead of no-reply@ ([93b10fe](https://github.com/vegify/vegify.app/commit/93b10fe99d652d82f811da0bb9b04798aecab89e))

## [0.3.3](https://github.com/vegify/vegify.app/compare/v0.3.2...v0.3.3) (2026-06-28)


### Bug Fixes

* **ci:** exclude the release version-bump commit from the server-change diff ([e644ed1](https://github.com/vegify/vegify.app/commit/e644ed156f684a13ffcc4a39edb37b8f85f09359))
* **web:** stop mirroring client logs to the production console ([1352991](https://github.com/vegify/vegify.app/commit/1352991860fc339060f0c6e305dd9ea631a1baa3))

## [0.3.2](https://github.com/vegify/vegify.app/compare/v0.3.1...v0.3.2) (2026-06-28)


### Bug Fixes

* **web:** keep public pages up when the backend is briefly unreachable ([f6b4260](https://github.com/vegify/vegify.app/commit/f6b4260cf67e13319f1bcd821646a110c9d04969))
* **web:** let signed-in users complete email verification and password reset ([4447f56](https://github.com/vegify/vegify.app/commit/4447f5683bc2b2d4ec9153d0a92c41a5036eae21))

## [0.3.1](https://github.com/vegify/vegify.app/compare/v0.3.0...v0.3.1) (2026-06-28)


### Bug Fixes

* **web:** serve robots.txt and sitemap.xml from S3, not the gated SSR Lambda ([bb3d01b](https://github.com/vegify/vegify.app/commit/bb3d01bd2d87c24d73f3e80f3220ce3d8c2781fd))

## [0.3.0](https://github.com/vegify/vegify.app/compare/v0.2.4...v0.3.0) (2026-06-28)


### Features

* **web:** public marketing landing at / with SEO and GEO surface ([b283c59](https://github.com/vegify/vegify.app/commit/b283c59c4219b1ed9b32a8ce3f995039151accc5))

## [0.2.4](https://github.com/vegify/vegify.app/compare/v0.2.3...v0.2.4) (2026-06-28)


### Bug Fixes

* **desktop:** bootstrap the content schema on a fresh on-device DB ([b765f49](https://github.com/vegify/vegify.app/commit/b765f4982925c775163f1b20dabb8a8754f18d2e))

## [0.2.3](https://github.com/vegify/vegify.app/compare/v0.2.2...v0.2.3) (2026-06-28)


### Bug Fixes

* **web:** serve apple-app-site-association as application/json ([2d810dd](https://github.com/vegify/vegify.app/commit/2d810dd373d2383057f2efd355b769e1ec60cf7f))

## [0.2.2](https://github.com/vegify/vegify.app/compare/v0.2.1...v0.2.2) (2026-06-27)


### Bug Fixes

* **desktop:** resolve the on-device DB path at runtime so shipped builds launch ([159ea15](https://github.com/vegify/vegify.app/commit/159ea15facd4061cd1f6197f6deff2da85d13e76))

## [0.2.1](https://github.com/vegify/vegify.app/compare/v0.2.0...v0.2.1) (2026-06-27)


### Bug Fixes

* **desktop:** embed Developer ID provisioning profile to authorize associated-domains ([15f9a04](https://github.com/vegify/vegify.app/commit/15f9a04dbb9bb8ffcbf24c44a5a1f4e0aec0168c))

## [0.2.0](https://github.com/vegify/vegify.app/compare/v0.1.3...v0.2.0) (2026-06-27)


### Features

* **desktop:** email-verification status and resend banner ([c5a7972](https://github.com/vegify/vegify.app/commit/c5a79724aafc4bf848d64cb7f56dafac485c8055))
* **desktop:** macOS Password AutoFill — associated-domains entitlement + AASA ([a5fb06b](https://github.com/vegify/vegify.app/commit/a5fb06b18e57e725178d2a375f48cd5712bfebd2))
* **server:** email verification — mint/confirm tokens, send on signup, resend endpoint ([d410a84](https://github.com/vegify/vegify.app/commit/d410a844f46a2a3131b32b4963301f237e38481e))
* **web:** email-verification route, verify screen, and resend banner ([b2bab3b](https://github.com/vegify/vegify.app/commit/b2bab3b4c808a5b0226bd7d37fa0ec88e37e0a07))
* **web:** serve the AASA from S3 and inject the Apple Team ID at build ([d1089f1](https://github.com/vegify/vegify.app/commit/d1089f1e5865b90824ad78a1d234c25a7a6d5cb0))

## [0.1.3](https://github.com/vegify/vegify.app/compare/v0.1.2...v0.1.3) (2026-06-26)


### Bug Fixes

* **desktop:** move gen_bindings codegen to an example so tauri never bundles it ([311b210](https://github.com/vegify/vegify.app/commit/311b210e92aec2d216192439b18cc260e0bbee45))

## [0.1.2](https://github.com/vegify/vegify.app/compare/v0.1.1...v0.1.2) (2026-06-26)


### Bug Fixes

* **desktop:** gate the gen-bindings binary behind a feature so the macOS bundle succeeds ([1ada4b5](https://github.com/vegify/vegify.app/commit/1ada4b5dd892c1286a866bd0de804abf5b93eade))

## [0.1.1](https://github.com/vegify/vegify.app/compare/v0.1.0...v0.1.1) (2026-06-26)


### Bug Fixes

* get VegifyServer actually serving (bind 0.0.0.0; fix user-data disk + seed) ([75067d9](https://github.com/vegify/vegify.app/commit/75067d9404bad0b7e1d3c4fde85d8c9b374d9674))
