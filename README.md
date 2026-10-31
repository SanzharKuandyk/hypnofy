# Hypnofy

Windows desktop application for writing, organizing, generating and narrating self-guided relaxation scripts. The rewritten interface follows the approved [Figma design](https://www.figma.com/design/gOLulpHyzRsG42fofiLCiC), with Dark as the default and Light/System options.

## Development

Requires Node.js 22.13+, Rust 1.98+, WebView2, Visual Studio C++ tools, the Vulkan SDK and Ninja at C:/DevTools/ninja.exe. Native UI fixtures use Node's built-in SQLite support. The launcher discovers Visual Studio and the installed Vulkan SDK, initializes the x64 compiler environment, and uses the ignored short .b directory to avoid Windows native compiler path limits.

```powershell
npm ci --ignore-scripts
npm run tauri dev
```

The Tauri wrapper builds the inference worker first. To produce a runnable development EXE with embedded UI:

```powershell
npm run dev:exe
```

Run .b/debug/Hypnofy-dev.exe from its build folder, keeping hypnofy-inference-worker.exe and native DLLs beside it. This is a debug build, with no installer or updater. Release packaging is intentionally disabled.

## Features and storage

- Collections, favorites, search, bulk management, Trash/restore, permanent deletion and revisions.
- TipTap writing with formatting, stable passage IDs, inline voice/pace overrides, explicit pauses and continuous passage groups. Serialized autosave preserves edits made during an in-flight save; conflicts offer recovery choices.
- NobodyWho local GGUF text generation and Kokoro/Supertonic speech. OpenRouter text generation is optional and explicit. Suggestions are reviewed before an undoable insertion.
- Managed model installation, verification, cancellation, folder scans and linking. Default model root is %USERPROFILE%/Models/Hypnofy. Linked models stay in place and Unlink never deletes them.
- Passage previews, saved narration, native playback/seek, ambience, sleep timer, mono narration WAV and stereo mixed WAV export at 48 kHz/16-bit PCM.
- UTF-8 TXT, Markdown and lossless versioned Hypnofy JSON import/export; bulk export publishes one new folder after every file succeeds.
- Required launch acknowledgment, with the same information accessible from Settings/Help.
- Shared dark/light selectors and popup lists, text/password fields, bounded steppers, sliders, checkboxes, progress and tooltips. Keyboard interaction and narrow-window popup placement are covered by component and native UI tests.

The application identifier remains app.hypnofy.desktop and Tauri resolves the existing application-data directory. SQLite migrations back up older databases before upgrading to schema 7. Unsupported legacy documents remain preserved and can be exported for recovery. Migration backups, scripts, revisions, pending suggestions, saved renders and user exports are persistent work.

Windows Credential Manager stores the optional OpenRouter key. IPC only accepts credential writes and returns connection status. The frontend has no general filesystem or shell capability. Development tests use disposable application/WebView profiles and synthetic migration data; they do not inspect existing credentials or personal libraries.

## Checks

```powershell
npm run check
npm run lint
npm run test
npm run test:e2e
npm run rpc:check
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
npm run native:lint
npm run native:test
npm run native:doc
```

The native wrappers run workspace Clippy with all targets/features and warnings denied, workspace tests, and shared-protocol documentation in the required VS/Ninja environment. Bindings are generated from Rust; rpc:check compares them without retaining temporary output.

Routine tests use fake network/audio inputs and never download models or call paid providers. Explicit model smoke instructions and the architecture map are in [docs/rewrite-status.md](docs/rewrite-status.md).
