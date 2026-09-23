# Hypnofy

Fast, private Windows desktop app for writing, organizing, generating, and narrating hypnosis scripts.

## Development

Requirements: Node.js 22+, Rust 1.98+, WebView2, and Windows build tools.

```powershell
npm install --ignore-scripts
npm run check
cargo check --locked --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

The app stores user data under `%APPDATA%\app.hypnofy.desktop`. Ollama is optional and is reached only on `127.0.0.1:11434`.

## Configuration

- Reviewed downloadable voice assets live in `src-tauri/assets/asset-manifest.json`.
- User model preferences live in SQLite and are edited from the in-app Settings page.
- Tauri permissions live in `src-tauri/capabilities/default.json`; the frontend has no shell or general filesystem capability.

## Code map

- `src/App.svelte`: desktop interaction and presentation.
- `src-tauri/src/commands.rs`: typed Tauri command surface.
- `src-tauri/src/config.rs`: reviewed asset manifest and user settings.
- `src-tauri/src/storage.rs`: SQLite migrations and script persistence.
- `src-tauri/src/providers.rs`: Ollama and managed llama.cpp adapters.
- `src-tauri/src/speech.rs`: verified voice setup and cached chunk rendering.
- `src-tauri/src/audio.rs`: native narration and ambience mixer.

No automated tests are included in v1 by design. Use `npm run check`, `cargo clippy --locked`, and the manual acceptance flow before packaging.

