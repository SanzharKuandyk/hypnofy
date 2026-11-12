# Rewrite architecture and validation

## Implementation map

| Location                            | Responsibility                                                                                    |
| ----------------------------------- | ------------------------------------------------------------------------------------------------- |
| src/app                             | Bootstrap, one Effect ManagedRuntime, typed routes, cross-module workflows and shell              |
| src/pages                           | Library, script, models and settings composition                                                  |
| src/domain                          | Validated frontend document model, branded identifiers and generated-contract aliases             |
| src/modules                         | Entry, editor, generation, models, narration and playback presentation/application boundaries                        |
| src/shared                          | Accessible UI primitives, theme/layout, platform dialogs, Effect/RPC infrastructure and testing   |
| src/shared/rpc/generated            | Rust-generated TauRPC declarations; never edit manually                                           |
| src-tauri/src/api                   | TauRPC namespace resolvers and request validation                                                 |
| src-tauri/src/application           | Generation, model management, narration compiler/rendering, export and cleanup                    |
| src-tauri/src/infrastructure        | Dedicated database/audio/inference workers, filesystem/download ownership and providers           |
| src-tauri/src/storage               | Schema migrations, transactional library/revisions and persistent models/jobs/renders/suggestions |
| src-tauri/crates/inference-protocol | Versioned framed worker messages                                                                  |
| src-tauri/crates/inference-worker   | Isolated NobodyWho native model ownership                                                         |

Pure library/settings/model views are page-sized modules; they share the application workflow rather than importing private internals. The import/cycle check enforces current module boundaries. One generated RPC facade owns native calls. Rust owns persistent rules, native resources and filesystem/network access. Effect owns frontend dependency lifetimes, subscriptions, cancellation and typed workflow failures. TipTap is lazy loaded and remains stable through ordinary updates.

The native worker pins NobodyWho to d045f3408e3dc9ffc0403220fe533a8fd29ec218. It reuses an idle loaded model for five minutes, releases it on model switches, uses a fresh conversation for each request, and registers no tools. Requests are framed on stdin/stdout; audio stays on disk. Cooperative cancellation has a short termination fallback. Windows Job Objects prevent orphan workers. Text context is bounded by verified model capacity; speech uses its own backend selection rather than being labeled Vulkan.

## Safety of persistent work

Schema 7 uses consistent SQLite backups, ordered transactional migrations, version-2 documents and millisecond narration settings. Tests cover legacy schemas, rollback, newer-version rejection, unsupported JSON preservation, revisions, collection moves and transactional bulk actions. Credentials are never read during migration. Retired-provider preferences are archived, not used to start old integrations.

Every job has an idempotent UUID, typed result/error and sequenced events. The frontend subscribes before starting work and reconciles authoritative snapshots. Completed suggestions persist until accepted/discarded. A rendering job captures an immutable script snapshot, publishes validated audio/manifest atomically, and never replaces previous audio on failure. Speech-unit hashes include model identity, voice, pace, text and engine revision. Saved render history remains user work.

Cleanup only removes proven owned staging or unreferenced internal audio. Managed model manifests prove the original root and installation identity. Linked files, exports, backups and arbitrary files are excluded. Maintenance excludes new jobs while resources are released. Locked files retain ownership records or report retryable cleanup failure. A process-wide workspace lock prevents a second application from recovering the first process's active staging.

## Design and entry notice

The [current writing and sound workflows](https://www.figma.com/design/gOLulpHyzRsG42fofiLCiC?node-id=21-4422) replace the old AI/delivery sidebars. Neutral charcoal surfaces use a bright lime accent; Light remains available. Figma contains desktop writing, AI review, delivery and sound-library views plus narrow AI/delivery layouts in both themes. Shared colour variables update earlier screens, and the revised controls reuse the component library.

AI review uses a wide workspace with an independent reading viewport and pinned review actions. Narrow windows switch between Compose and Preview. Only one saved suggestion is shown at a time; selection rewrites retain their snapshot guards. Delivery separates whole-script defaults from passage changes and discloses fine timing only when requested. The editor exposes paragraph style, bold, italic and pause directly; lists, history and continuous grouping are in More formatting. Narrow headers move Sounds and favorite controls into Script actions. No permanent inspector consumes writing width. Completed narration becomes the selected render; previews play automatically and full renders load ready to play.

The narration compiler now normalizes explicit written pause directions before voice/speed resolution and synthesis. It accepts `[pause:5s]`, `(pause for 5 seconds)` and a standalone `pause 5 seconds` line, including Windows CRLF and cues split across formatting marks. Supported pauses range from 0.5 to 120 seconds. Unsupported or invalid recognizable directions block synthesis with a correction message. Normal prose such as “You can pause 5 seconds before answering” remains speech. Frontend and Rust use shared conformance fixtures; plain-text paste and AI insertion convert cues into timing chips without discarding rich HTML clipboard formatting. Render manifests record pipeline version 2; older audio stays playable but is marked for re-rendering.

The sound library imports WAV/MP3/FLAC/Ogg into owned copies, supports search, rename, independent preview, script assignment and background volume. Preview pauses narration, and ordinary narration playback resumes the selected looping background. Volume is persisted with the active script. Removal refuses assets referenced by scripts or revisions and validates ownership before deleting a library copy. Original files remain user-owned. Routine native tests create silent PCM fixtures and seed a saved AI draft in a disposable database; they never open personal scripts or models.

Expressive-model research: [Qwen3-TTS 1.7B CustomVoice](https://github.com/QwenLM/Qwen3-TTS) supports natural-language prosody/style instructions; [Chatterbox](https://github.com/resemble-ai/chatterbox) exposes expression controls. These need a separate verified speech adapter and are not selectable as Kokoro/Supertonic bundles. The current UI explains this capability boundary instead of exposing unsupported emotion controls.

The [form-control design](https://www.figma.com/design/gOLulpHyzRsG42fofiLCiC?node-id=18-4242) adds nine reusable families with 62 dark/light states: selectors, popup options, checkboxes, sliders, number fields, text fields, textareas, progress and tooltips. Semantic control-border variables preserve contrast in both themes. Option labels are exposed as component properties.

Shared Svelte controls in `src/shared/ui` replace page-level native selects and inconsistent field markup. Bits UI owns selection, checkbox, slider, progress and tooltip interaction; `controls.css` owns their appearance. Select values preserve strings, numbers, booleans and null without coercion. Popup lists are portaled, collision-aware and scroll independently. Number fields expose bounded steppers and retain invalid drafts until blur, then restore the last valid value. Password fields have an explicit reveal action. Native file/folder choosers remain Windows dialogs.

The component regression suite uses headless Edge against the development showcase to exercise keyboard selection, disabled options, focus restoration, boolean export choices, validation, slider keys, mixed checkboxes, tooltips and selectors inside dialogs. Headless scrollbar hiding is disabled so tests can drag the actual themed thumb. Select viewports explicitly restore the app scrollbar and return focus to the selection trigger when scrolling ends. Native tests seed synthetic speech inventory in their disposable database without reading model files, then verify the Models picker in both themes at desktop/narrow widths and persist voice, pace and volume through the actual settings API. No inference or downloads run in this UI fixture.

The [scrollbar and resize update](https://www.figma.com/design/gOLulpHyzRsG42fofiLCiC?node-id=16-3937) adds 12 reusable variants: Dark/Light × Idle/Hover/Dragging × Vertical/Horizontal. Theme-bound thumbs use a transparent 10px gutter, a 6px rounded grip and a 40px minimum grip length, without arrow buttons. The main frames in both themes now fill their available width/height; the editor frame was checked at 1600×1000 and restored to its reference size.

The application fills the live client area and styles native scrollbars, preserving native wheel, keyboard and drag behavior. Automated test windows stay hidden. Visual viewport emulation and actual Windows resizing run in separate app processes: a fixed CDP viewport must never be used as evidence that native resize works. The native test exercises 900×700, 1600×1000, 1024×768, 390×844 and 1440×960, then checks maximized client bounds at the current Windows DPI. It also verifies editor content, status visibility and the scrollbar gutter. A script view and the global shell share one player at a time.

The approved dark/light palette, Inter/Lora, 240px sidebar, 72px rail, drawer navigation, writing canvas, delivery inspector, model/provider/storage pages and entry notice are implemented in one responsive tree. Main visual baselines are 1440×960, 1024×768, 768×1024 and 390×844. Minimum client size is 390×620. The development-only component showcase is available at /?showcase through Vite.

The notice requires an unchecked acknowledgment before acceptance. Native AI/playback gates require session acceptance; a frontend reload retains acceptance, a new process does not. Help reopens the same content with normal dismissal.

**Draft product wording:** the responsibility/liability paragraph is product copy, not a legal opinion. It applies only to the extent permitted by law and does not exclude liability that cannot legally be excluded. Professional-care guidance is linked to the NHS. The notice explains that Hypnofy does not replace qualified care, promise results or justify delaying treatment.

## Validation workflow

Use the commands in README. Native Playwright tests launch the embedded development EXE with temporary workspace/WebView profiles and control the WebView through CDP. This uses no Computer Use and does not open the personal library. The test profile disables cloud credentials.

For explicit real-model checks, prepare owned synthetic fixtures:

```powershell
node scripts/fetch-smoke-models.mjs
npm run worker:smoke -- --text-model C:/Projects/hypnofy/.b/smoke-models/text/model.gguf --speech-model C:/Projects/hypnofy/.b/smoke-models/speech --device cpu --test-cancellation
npm run worker:smoke -- --text-model C:/Projects/hypnofy/.b/smoke-models/text/model.gguf --speech-model C:/Projects/hypnofy/.b/smoke-models/speech --device auto --test-cancellation
$env:HYPNOFY_REAL_MODELS='1'
npm run test:e2e
```

The fixture downloader uses immutable revisions and SHA-256 verification. It creates .hypnofy-test-fixtures as an ownership marker. Run `node scripts/fetch-smoke-models.mjs --clean` after the explicit test session. Cleanup verifies the marker, exact file inventory, canonical paths and model checksums before deleting individual files; it refuses unknown files or links and never recursively deletes a model folder. Routine tests do not download models. A worker handshake alone does not satisfy real text/speech integration.

Fixture provenance: Qwen_Qwen3-0.6B-GGUF revision 8cb2bae78af48e5727740bd865d01ce291ea6d33, Q4_K_M SHA-256 168a5fd6091254189e2594764db1c66dd6b2984f0091f92a86f388bddd1d9bb2. Kokoro-82M revision 0c5985572257cf2f1bce418b2cf47ed5a9580b14, model.onnx SHA-256 a287341900ae88fd8c049af4560a6d270aecba6b8859f59c307ba50dd0d0afb6, bf_emma.safetensors SHA-256 2145949882a4bf36af1b2f7254a068ec765fc5e731ad7e6f2db5ecd1029d09de; config.json comes from the same revision.

Kokoro and Qwen are exercised with actual local inference. Supertonic uses the verified architecture adapter and bundle contract, but requires its own larger explicit model smoke when validating a new variant. OpenRouter is tested with synthetic SSE/error streams; no paid live generation is part of routine validation. GPU/CUDA availability is machine-dependent and speech can fall back to CPU.

## Recording ownership and public discovery

The current [model discovery and recording views](https://www.figma.com/design/gOLulpHyzRsG42fofiLCiC?node-id=25-5484) have dark/light and narrow variants. Model search is performed by Rust against the public Hugging Face API, independently of the verified installation catalog. Results include task, library, downloads, likes, license, gating and compatibility. Details include escaped readable model-card text (not embedded third-party HTML), language metadata, file sizes and revision-specific source links. Search supports sorting and pagination. Model source/file links use the typed native browser launcher; WebView popup blocking cannot swallow them. The launcher accepts only public HTTPS Hugging Face URLs and never runs a shell. Unsupported models, including Qwen3-TTS, remain browse-only. No new inference engine or model download is introduced by discovery. Routine native tests use a loopback HTTP fixture enabled only for an isolated debug test profile.

The Recordings tab includes saved narration, optional passage previews and recordings retained in Trash. Close player unloads the native sinks and path; Delete recording removes only that owned recording and unreferenced cached speech. Trash unloads playback but retains audio for restore. Permanent deletion queues audio ownership before cascading script rows. Queued metadata survives a Windows file lock and supports visible retry, startup recovery and Storage cleanup; removal is reported complete only after the files are gone. Startup also reclaims recordings orphaned by older deletion only when the internal ownership manifest matches the file ID. Canonical directory checks prevent traversal through relocated/symlinked audio roots. Shared units, background originals and user exports stay protected.
