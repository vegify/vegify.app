# Changelog

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
