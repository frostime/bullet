# bullet

Windows-first Tauri 2 + Svelte 5 desktop application. Entry points: `/src/App.svelte`, `/src-tauri/src/desktop/mod.rs`.

## Commands

- `npm ci`; `npm run check`; `npm test`; `npm run build`
- `npm run desktop`: real desktop development, requires the Tauri platform prerequisites.
- `npm run dev`: browser interaction preview, always simulated execution.
- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`: real execution/core tests without WebView dependencies.
- `npm run desktop:build`: Windows NSIS installer; use Windows/MSVC for release.

## Routing

- Boundary decisions and why: [architecture](.dev/docs/architecture.md).
- Async races, detach, cancellation: [task lifecycle](.dev/docs/task-lifecycle.md), [execution contract](src-tauri/src/execution/execution.SPEC.md).
- Configuration/path/script changes: [config contract](src-tauri/src/config/config.SPEC.md).
- Window geometry/focus: [desktop contract](src-tauri/src/desktop/desktop.SPEC.md).
- Build and native acceptance: [Windows validation](.dev/docs/windows-validation.md).

## Constraints

- Rust owns all tasks and persistent configuration. UI detachment never cancels a task.
- uv and pi are ordinary CLI programs, not providers or special runners.
- Managed Script adds ownership/versioned source, not an execution engine.
- Set each child's cwd. Never mutate the application's global current directory.
- Executable arguments remain an array. Text is stdin or data in one argument, never interpolated into a shell command.
- No automatic execution retries; a timed-out write can already have happened.
- Keep all output bounded while continuing to drain pipes.
- Module contracts live beside code; cross-module docs live under `.dev/docs` with frontmatter. Types/code are authority for local implementation.
- Do not copy personal skills into this repository. Update contracts/docs in the same change as behavior.
