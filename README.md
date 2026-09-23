# Hypnofy

Windows desktop app for writing, organizing, and drafting hypnosis scripts. The library and editor work offline; narration is not configured in this release.

## Development

Requirements: Node.js 22+, Rust 1.98+, WebView2, and Windows build tools.

```powershell
npm ci --ignore-scripts
npm run check
cargo check --locked --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

The app stores user data under `%APPDATA%\app.hypnofy.desktop`. Ollama is optional and is reached only on `127.0.0.1:11434`. To build an installer, run `npm run tauri build` after the checks above.

## Configuration

- The reviewed llama.cpp runtime asset lives in `src-tauri/assets/asset-manifest.json`. No speech engine or voice pack is installed by Hypnofy.
- Linked GGUF files remain in their original locations. After linking, use **Verify** to load the file through the pinned llama.cpp runtime before it becomes selectable. The Models page also manages an optional OpenRouter key. The key is stored in Windows Credential Manager; scripts and model preferences stay in SQLite.
- Tauri permissions live in `src-tauri/capabilities/default.json`; the frontend has no shell or general filesystem capability.

## Code map

- `src/App.svelte`: desktop shell, navigation, and save coordination.
- `src/design/` and `src/components/`: palette and reusable controls.
- `src/features/`: library, document editor, AI draft, models, and session views.
- `src/bridge/`: typed frontend commands and data contracts.
- `src-tauri/src/commands.rs`: typed Tauri command surface.
- `src-tauri/src/config.rs`: reviewed asset manifest and user settings.
- `src-tauri/src/storage/` and `document.rs`: SQLite migrations, revisions, linked models, and versioned script documents.
- `src-tauri/src/models/`: Ollama, GGUF, OpenRouter, and the guarded OpenCode status.
- `src-tauri/src/jobs.rs`: generation cancellation registry.
- `src-tauri/src/speech.rs`: timing plan and future speech-provider interface.

OpenCode generation remains disabled until an installed version passes the isolated-profile and deny-all-tool checks. Cloud generation through OpenRouter is explicitly opt-in. Existing render records remain in storage, but this release does not create new narration audio.

No automated tests were added for this refactor. Use `npm run check`, `cargo clippy --locked`, audits, and the manual acceptance flow before packaging.

