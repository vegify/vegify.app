# Changelog

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
