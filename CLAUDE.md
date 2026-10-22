# Project

Kuznya Mirov ("Кузня Миров") is a data-driven 2D game engine: a Rust core (`engine/`) that reads
a game's four JSON files (`game.json`, `properties.json`, `scene.json`, `rules.json`), runs a
fixed nine-stage simulation step, and renders via `wgpu`; it compiles to WebAssembly and is driven
by a browser page (`web/`, built separately). `games/snake/` and `games/arkanoid/` are demo games
made entirely of data — no engine code exists per-game.

The design documentation that governs this code lives outside this repo, in the Obsidian vault at
`C:\projects\obsidian\Проекты\Кузня Миров\`: `Архитектура Кузни\` (how the engine is built),
`Фазы Кузни\` (one note per feature — this is where feature plans go, not `docs/plans/`) and
`Журнал Кузни\` (decisions). The folder follows `Проекты\Шаблон проекта.md`. Any mismatch between the
code and those notes is a bug in the code, not in the docs — do not "fix" the docs to match the code.

# Stack & Build

`engine/` is a plain Cargo crate (edition 2024); `wgpu`, `wasm-bindgen`, `web-sys`, and `bytemuck`
are `[target.wasm32-unknown-unknown.dependencies]` only — they are not compiled for the native
target, so `cargo check`/`test`/`clippy` without `--target` stay fast and do not need a GPU.

`luars/` is the project's own copy of the `luars` 0.26.3 Lua runtime (MIT, author's `LICENSE`
kept); `engine/` depends on it by path, without a Cargo workspace. Upstream releases are not pulled
in: fix a runtime defect in `luars/` itself, with a test in `luars/src/test/`, instead of adding a
workaround in `engine/`. `cargo test` in `engine/` does not run the library's tests — run
`cargo test` in `luars/` as well after changing it.

`cosmic-text/` is the project's own copy of the `cosmic-text` 0.18.2 text layout library (MIT or
Apache-2.0, the author's `LICENSE-MIT` and `LICENSE-APACHE` kept); `engine/Cargo.toml` substitutes
it for the crates.io release through `[patch.crates-io]`, so `glyphon` shapes and rasterises text
with it. The copy adds the `wdth` font axis (`Attrs::stretch`) next to the library's own `wght`.
Upstream releases are not pulled in: fix a defect, or add what the engine needs, in `cosmic-text/`
itself, with a test in `cosmic-text/tests/`, instead of a workaround in `engine/`; run `cargo test`
in `cosmic-text/` after changing it.

This machine has no MSVC toolchain, so the default `x86_64-pc-windows-msvc` Rust host cannot link.
The installed default toolchain is `stable-x86_64-pc-windows-gnu` (MinGW-w64 from WinLibs), which
does link natively — `rustup default` should stay on the gnu toolchain here. `wasm32-unknown-unknown`
itself links fine with either host toolchain (it uses `rust-lld`, not the host linker).

Build/package for the browser: `cargo build --target wasm32-unknown-unknown` from `engine/`, then
`wasm-pack build --target web --out-dir pkg`.

`web/` depends on that `pkg` output as a file dependency, and `engine/pkg` is gitignored, so it does
not exist on a fresh clone. `web/package.json` handles this itself: `predev` and `prebuild` scripts
run `node scripts/ensureEnginePkg.mjs`, which invokes the `wasm-pack` command above when
`engine/pkg` is missing or when `engine/src`, `engine/shaders`, `engine/Cargo.toml`,
`engine/Cargo.lock`, `luars/src`, `luars/Cargo.toml`, `luars/Cargo.lock`, `cosmic-text/src` or
`cosmic-text/Cargo.toml` were modified more recently than `engine/pkg` was built, then `npm run dev`
or `npm run build` in `web/` continues as normal. So on a clean clone, just `npm install && npm run
build` (or `npm run dev`) in `web/` builds the engine first and the page second, with no manual
step, and editing engine, `luars` or `cosmic-text` source gets picked up by the next `npm run
dev`/`build` without deleting `pkg` by hand. In the container build (`Dockerfile`) the
`wasm-builder` stage copies `engine/`, `luars/` and `cosmic-text/` sources and builds `pkg`; the
`web-builder` stage only copies in that already-built `engine/pkg`, never the sources, so the mtime
check finds no source files to compare and never re-triggers `wasm-pack` in a stage that has no
`cargo`.

# Common Mistakes

[Empty]
