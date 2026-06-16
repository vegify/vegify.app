# web-leptos — React-like DX in Rust (Leptos SSR)

A spike answering "can we keep React/shadcn DX *and* be fastest?" — built from real components
(`#[component]` + the `view!` macro) and **the same design system as the React apps**: Tailwind v4 +
`@vegify/tokens`, scanned from the `.rs` source. Same recursive-CTE nutrition over the shared
`.data/vegify.db`. Throughput ≈ raw Rust (`web-fast`), ~12–25× the JS shells — see `docs/benchmark.md`.

## Build & run

```sh
# 1. compile the design system (Tailwind scans src/*.rs for class names → style/out.css,
#    which main.rs embeds via include_str!). Re-run when the markup/classes change.
node_modules/.bin/tailwindcss -i apps/web-leptos/style/app.css -o apps/web-leptos/style/out.css --minify

# 2. build + run (use a high port, not 3000/3001)
cargo build --release --manifest-path apps/web-leptos/Cargo.toml
DATABASE_PATH="$(pwd)/.data/vegify.db" PORT=39060 ./apps/web-leptos/target/release/web-leptos
# → http://127.0.0.1:39060/recipes/<id>
```

## Status / notes

- **SSR-only.** Full hydration/islands (for interactivity) would add `cargo-leptos` + a wasm client
  build — a follow-up; reads are fine SSR-only.
- `style/out.css` is generated but committed (it's an `include_str!` build input). Regenerate with
  step 1 above after changing markup.
- IDE/lint: rust-analyzer expands `view!` (errors/hover/goto/autocomplete inside it), clippy lints it,
  `leptosfmt` formats it. Not unlintable.
- Leptos 0.8 SSR: render with `view!{…}.to_html()` (RenderHtml trait) inside `Owner::new().with(…)`;
  there is no `render_to_string`.
