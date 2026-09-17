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

This machine has no MSVC toolchain, so the default `x86_64-pc-windows-msvc` Rust host cannot link.
The installed default toolchain is `stable-x86_64-pc-windows-gnu` (MinGW-w64 from WinLibs), which
does link natively — `rustup default` should stay on the gnu toolchain here. `wasm32-unknown-unknown`
itself links fine with either host toolchain (it uses `rust-lld`, not the host linker).

Build/package for the browser: `cargo build --target wasm32-unknown-unknown` from `engine/`, then
`wasm-pack build --target web --out-dir pkg`.

`web/` depends on that `pkg` output as a file dependency, and `engine/pkg` is gitignored, so it does
not exist on a fresh clone. `web/package.json` handles this itself: `predev` and `prebuild` scripts
run `node scripts/ensureEnginePkg.mjs`, which invokes the `wasm-pack` command above when `engine/pkg`
is missing or when `engine/src`, `engine/shaders`, `engine/Cargo.toml` or `engine/Cargo.lock` were
modified more recently than `engine/pkg` was built, then `npm run dev` or `npm run build` in `web/`
continues as normal. So on a clean clone, just `npm install && npm run build` (or `npm run dev`) in
`web/` builds the engine first and the page second, with no manual step, and editing engine source
gets picked up by the next `npm run dev`/`build` without deleting `pkg` by hand. The container build
(`Dockerfile`) is unaffected: its `web-builder` stage only copies in the already-built `engine/pkg`,
never the engine sources, so the mtime check finds no source files to compare and never re-triggers
`wasm-pack` in a stage that has no `cargo`.

# Common Mistakes

[Empty]
