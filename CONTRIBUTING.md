# Contributing to vegify.app

Thanks for your interest. A few things specific to this project before you start.

## License — of the project and of your contributions

vegify.app is **source-available, not open source**. It is released under the [PolyForm Noncommercial License 1.0.0](LICENSE): anyone may read, run, modify, and share the code for **noncommercial** purposes, but commercial use is reserved to the copyright holder.

Because of that, contributions need one extra thing beyond the usual. **By submitting a contribution — a pull request, patch, or any other change — you represent that you wrote it (or otherwise have the right to submit it), and you grant the copyright holder (John Carmack) a perpetual, worldwide, irrevocable, royalty-free license to use, reproduce, modify, sublicense, and distribute your contribution under any terms, including commercial terms — not only the PolyForm Noncommercial License.**

This keeps the project's commercial rights with a single owner, which is the entire point of the license. If you can't agree to that, please don't submit code — but bug reports, ideas, and design feedback are always welcome and carry no such terms. (This is a lightweight inbound grant, not legal advice; for a substantial contribution you may be asked to sign a short CLA confirming the same thing.)

## Getting set up

See the [README quickstart](README.md#quickstart) for prerequisites and the full bootstrap. The short version:

```sh
pnpm install
pnpm --filter @vegify/tokens build
pnpm db:push && pnpm db:seed
cargo run -p vegify-server   # backend (auth + content API)
pnpm dev                     # web + desktop, against that backend
```

## How the codebase fits together

vegify runs two clients over one backend, all rendering **one shared set of screens** in `packages/ui`. The web (`apps/web`) and desktop (`apps/desktop`) are thin data/routing adapters; the backend (`services/api`) is the source of truth. A feature almost always lands in the shared screen plus *both* adapters — please don't re-implement a screen per client. The README's Architecture section has the full picture.

## Commits and pull requests

- **Write clear PR titles — they ARE the changelog.** Merging to main ships and auto-cuts a patch release whose notes are generated from PR titles (grouped by label — see `.github/release.yml`). Conventional-commit prefixes (`feat:`, `fix:`, …) remain welcome as style, but nothing parses them anymore; put `[skip release]` in the PR title to ship without cutting a version.
- **Keep each PR to one logical change** — it is far easier to review and to revert.
- **Verify before you push:** `pnpm build` (both apps build), `cargo test` (server + core), `cargo clippy --all-targets`, and `cargo fmt --check`. For a web change, smoke the built app, not just the dev server.

## Bugs, features, and security

Open an issue for bugs and feature ideas. For anything security-sensitive, please **don't** open a public issue — contact the maintainer directly so it can be addressed before disclosure.

## Conduct

Be decent; harassment or abuse isn't welcome here.
