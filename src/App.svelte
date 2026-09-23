<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { api, type Ambience, type Collection, type PlayerState, type Script, type ScriptSummary, type SettingsView, type SpeechStatus, type TextModel, type Voice } from "./api";
  import AppSelect from "./components/AppSelect.svelte";

  let collections: Collection[] = [];
  let scripts: ScriptSummary[] = [];
  let current: Script | null = null;
  let selectedView = "all";
  let search = "";
  let sidebarOpen = true;
  let sessionOpen = false;
  let focusMode = false;
  let loading = true;
  let saveState = "Saved";
  let error = "";
  let toast = "";
  let editVersion = 0;
  let savedVersion = 0;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let saving: Promise<void> | null = null;
  let generationOpen = false;
  let generationJob = "";
  let generationBusy = false;
  let generationError = "";
  let draft = "";
  let intention = "Relax deeply and reinforce a helpful positive change";
  let minutes = 10;
  let tone = "gentle and reassuring";
  let instructions = "";
  let includeCurrent = false;
  let models: TextModel[] = [];
  let modelKey = "";
  let modelRuntimeInstalling = false;
  let voices: Voice[] = [];
  let speech: SpeechStatus = { installed: false, downloading: false, engineVersion: "", modelVersion: "", downloadBytes: 0 };
  let speechJob = "";
  let speechProgress = 0;
  let speechMessage = "";
  let latestManifest = "";
  let playbackUpdates: Promise<void> = Promise.resolve();
  let ambience: Ambience[] = [];
  let player: PlayerState = { playing: false, positionSeconds: 0, durationSeconds: 0, finished: true };
  let playerTimer: ReturnType<typeof setInterval> | null = null;
  let unlisteners: UnlistenFn[] = [];
  let settingsView: SettingsView | null = null;
  let settingsOpen = false;
  let settingsSaving = false;
  let closing = false;

  const viewCollection = () => selectedView.startsWith("collection:") ? selectedView.slice(11) : null;
  const displayDate = (iso: string) => new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }).format(new Date(iso));
  const formatTime = (seconds: number) => `${Math.floor(seconds / 60)}:${Math.floor(seconds % 60).toString().padStart(2, "0")}`;
  function message(text: string) { toast = text; setTimeout(() => { if (toast === text) toast = ""; }, 2600); }
  function fail(value: unknown) { error = typeof value === "string" ? value : value instanceof Error ? value.message : String(value); }

  async function refreshList() {
    try {
      scripts = await api.scripts(viewCollection(), search, selectedView === "favorites", selectedView === "trash");
      collections = await api.collections();
    } catch (e) { fail(e); }
  }
  function searchChanged() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(refreshList, 160);
  }
  async function loadScript(id: string) {
    await flushSave();
    try { current = await api.script(id); editVersion = 0; savedVersion = 0; saveState = "Saved"; latestManifest = ""; error = ""; }
    catch (e) { fail(e); }
  }
  async function chooseView(view: string) { await flushSave(); selectedView = view; current = null; if (view === "settings") { try { settingsView = await api.getSettings(); } catch (e) { fail(e); } } else { await refreshList(); } }
  async function newScript() {
    await flushSave();
    try { current = await api.createScript(viewCollection()); editVersion = 0; savedVersion = 0; selectedView = `collection:${current.collectionId}`; await refreshList(); setTimeout(() => document.querySelector<HTMLInputElement>(".title-input")?.select()); }
    catch (e) { fail(e); }
  }
  function changed() { editVersion++; saveState = "Unsaved"; if (saveTimer) clearTimeout(saveTimer); saveTimer = setTimeout(flushSave, 400); }
  async function flushSave() {
    if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; }
    if (!current || savedVersion === editVersion) return saving;
    if (saving) { await saving; if (savedVersion !== editVersion) return flushSave(); return; }
    const target = current; const version = editVersion; saveState = "Saving…";
    saving = (async () => {
      try {
        const saved = await api.updateScript(target);
        if (current?.id === target.id) {
          current.revision = saved.revision;
          current.updatedAt = saved.updatedAt;
        }
        const summary = scripts.find(script => script.id === target.id);
        if (summary) {
          summary.title = saved.title;
          summary.updatedAt = saved.updatedAt;
          summary.favorite = saved.favorite;
        }
        savedVersion = version;
        saveState = savedVersion === editVersion ? "Saved" : "Unsaved";
      }
      catch (e) { saveState = "Save failed"; fail(e); }
      finally { saving = null; }
    })();
    return saving;
  }
  async function createCollection() { const name = window.prompt("Collection name"); if (!name?.trim()) return; try { const item = await api.createCollection(name); await refreshList(); await chooseView(`collection:${item.id}`); } catch (e) { fail(e); } }
  async function duplicateCurrent() { if (!current) return; await flushSave(); try { current = await api.duplicateScript(current.id); selectedView = `collection:${current.collectionId}`; editVersion = savedVersion = 0; await refreshList(); message("Script duplicated"); } catch (e) { fail(e); } }
  async function moveToTrash() { if (!current) return; await flushSave(); try { await api.setDeleted(current.id, true); current = null; await refreshList(); message("Moved to Trash"); } catch (e) { fail(e); } }
  async function restoreCurrent() { if (!current) return; try { await api.setDeleted(current.id, false); current = null; await refreshList(); message("Script restored"); } catch (e) { fail(e); } }
  async function importScript() { const path = await open({ multiple: false, filters: [{ name: "Text", extensions: ["txt", "md"] }] }); if (!path) return; try { current = await api.importText(path, viewCollection()); selectedView = `collection:${current.collectionId}`; editVersion = savedVersion = 0; await refreshList(); message("Script imported"); } catch (e) { fail(e); } }
  async function exportScript() { if (!current) return; await flushSave(); const path = await save({ defaultPath: `${current.title.replace(/[<>:"/\\|?*]/g, "-")}.md`, filters: [{ name: "Markdown", extensions: ["md"] }, { name: "Text", extensions: ["txt"] }] }); if (!path) return; try { await api.exportText(current.id, path); message("Script exported"); } catch (e) { fail(e); } }
  function insertPause() { if (!current) return; const area = document.querySelector<HTMLTextAreaElement>(".script-editor"); if (!area) return; const start = area.selectionStart; const end = area.selectionEnd; current.body = `${current.body.slice(0, start)}[pause:5s]${current.body.slice(end)}`; changed(); setTimeout(() => { area.focus(); area.setSelectionRange(start + 10, start + 10); }); }

  async function refreshModels() {
    const previous = modelKey;
    models = await api.models();
    const preferred = models.find(model => `${model.provider}|${model.id}` === previous)
      ?? models.find(model => model.provider === settingsView?.preferences.defaultProvider && model.id === settingsView?.preferences.defaultModel)
      ?? models.find(model => model.available)
      ?? models[0];
    modelKey = preferred ? `${preferred.provider}|${preferred.id}` : "";
  }
  async function openGeneration() { generationOpen = true; generationError = ""; try { settingsView = await api.getSettings(); await refreshModels(); } catch (e) { generationError = String(e); } }
  async function installModelRuntime() {
    modelRuntimeInstalling = true; generationError = "";
    try { await api.installModelRuntime(); await refreshModels(); message("Local model runtime installed"); }
    catch (e) { generationError = String(e); }
    finally { modelRuntimeInstalling = false; }
  }
  async function generate() {
    const selected = models.find(model => `${model.provider}|${model.id}` === modelKey);
    if (!selected?.available) { generationError = selected?.reason ?? "The selected model is unavailable"; return; }
    const [provider, model] = modelKey.split("|");
    if (!provider || !model || !intention.trim()) return;
    draft = ""; generationError = ""; generationBusy = true;
    try { generationJob = await api.generate({ provider, model, intention, minutes, tone, instructions, includedText: includeCurrent ? current?.body ?? "" : null }); }
    catch (e) { generationBusy = false; generationError = String(e); }
  }
  async function cancelGeneration() { if (generationJob) await api.cancelGeneration(generationJob); generationBusy = false; }
  function acceptDraft() { if (!current || !draft.trim()) return; current.body = draft.trim(); changed(); generationOpen = false; draft = ""; }

  async function installVoices() { try { speech.downloading = true; speechJob = await api.installVoices(); } catch (e) { speech.downloading = false; fail(e); } }
  async function play() { if (!current) return; await flushSave(); if (!speech.installed) { sessionOpen = true; message("Install the local voice pack first"); return; } try { speechMessage = "Planning narration…"; speechProgress = 0; speechJob = await api.synthesize({ scriptId: current.id, revision: current.revision, text: current.body, voice: current.session.voice, speed: current.session.speed, paragraphPause: current.session.paragraphPause, preview: false }); } catch (e) { fail(e); } }
  async function togglePlayer() { try { player = await api.togglePlayback(); } catch (e) { fail(e); } }
  async function stopPlayer() { try { await api.stopPlayback(); player = await api.playerState(); } catch (e) { fail(e); } }
  async function seekPlayer(event: Event) { try { player = await api.seekPlayback(Number((event.target as HTMLInputElement).value)); } catch (e) { fail(e); } }
  async function updateVolumes() { if (!current) return; changed(); try { await api.setVolumes(current.session.narrationVolume, current.session.ambienceVolume); } catch {} }
  async function importSound() { const path = await open({ multiple: false, filters: [{ name: "Audio", extensions: ["wav", "mp3", "flac", "ogg"] }] }); if (!path) return; try { await api.importAmbience(path); ambience = await api.ambience(); message("Background sound imported"); } catch (e) { fail(e); } }
  async function chooseSettingFile(kind: "gguf" | "server") { if (!settingsView) return; const path = await open({ multiple: false, filters: kind === "gguf" ? [{ name: "GGUF model", extensions: ["gguf"] }] : [{ name: "llama.cpp server", extensions: ["exe"] }] }); if (!path) return; if (kind === "gguf") settingsView.preferences.ggufModelPath = path; else settingsView.preferences.llamaServerPath = path; }
  async function saveSettingsPage() { if (!settingsView) return; settingsSaving = true; try { settingsView = await api.saveSettings(settingsView.preferences); message("Settings saved"); } catch (e) { fail(e); } finally { settingsSaving = false; } }
  async function openSettings() { try { settingsView = await api.getSettings(); settingsOpen = true; } catch (e) { fail(e); } }

  function keydown(event: KeyboardEvent) { if (!(event.ctrlKey || event.metaKey)) return; if (event.key.toLowerCase() === "n") { event.preventDefault(); newScript(); } if (event.key.toLowerCase() === "s") { event.preventDefault(); flushSave(); } if (event.key.toLowerCase() === "k") { event.preventDefault(); document.querySelector<HTMLInputElement>(".search-input")?.focus(); } }

  onMount(async () => {
    window.addEventListener("keydown", keydown);
    unlisteners.push(await listen<{ jobId: string; status: string; text: string; error: string | null }>("generation-event", ({ payload }) => { if (payload.jobId !== generationJob) return; if (payload.status === "streaming") draft += payload.text; if (payload.status === "complete" || payload.status === "cancelled") generationBusy = false; if (payload.error) { generationBusy = false; generationError = payload.error; } }));
    unlisteners.push(await listen<{ jobId: string; status: string; progress: number; message: string; manifestPath: string | null; error: string | null }>("speech-event", ({ payload }) => {
      if (payload.jobId !== speechJob) return;
      speechProgress = payload.progress; speechMessage = payload.message;
      if (payload.error) { speech.downloading = false; fail(payload.error); }
      if (payload.status === "ready" && payload.manifestPath && current) {
        latestManifest = payload.manifestPath;
        const script = current;
        playbackUpdates = playbackUpdates.then(async () => { player = await api.startPlayback(latestManifest, script.session.ambienceId, script.session.narrationVolume, script.session.ambienceVolume); }).catch(fail);
      } else if (payload.status === "chunk" && payload.manifestPath === latestManifest) {
        playbackUpdates = playbackUpdates.then(async () => { player = await api.appendPlayback(latestManifest); }).catch(fail);
      }
      if (payload.status === "complete") { speech.downloading = false; api.speechStatus().then(value => speech = value).catch(fail); }
    }));
    const windowHandle = getCurrentWindow();
    unlisteners.push(await windowHandle.onCloseRequested(async event => {
      event.preventDefault();
      if (closing) return;
      closing = true;
      // A close request is intercepted once so pending text can save. Destroying the
      // WebView afterwards avoids re-entering this handler through another close call.
      try {
        await Promise.race([
          flushSave(),
          new Promise<void>(resolve => setTimeout(resolve, 1200)),
        ]);
      } finally {
        await windowHandle.destroy();
      }
    }));
    try { [collections, ambience, voices, speech] = await Promise.all([api.collections(), api.ambience(), api.voices(), api.speechStatus()]); await refreshList(); if (scripts[0]) await loadScript(scripts[0].id); }
    catch (e) { fail(e); } finally { loading = false; }
    playerTimer = setInterval(async () => { if (!player.finished) { try { player = await api.playerState(); } catch {} } }, 500);
  });
  onDestroy(() => { window.removeEventListener("keydown", keydown); unlisteners.forEach(fn => fn()); if (saveTimer) clearTimeout(saveTimer); if (searchTimer) clearTimeout(searchTimer); if (playerTimer) clearInterval(playerTimer); });
</script>

<svelte:head><meta name="description" content="Private hypnosis scripts and local narration" /></svelte:head>

<div class:focus-mode={focusMode} class:settings-view={selectedView === "settings"} class="app-shell">
  {#if sidebarOpen && !focusMode}
    <aside class="sidebar" aria-label="Script library">
      <div class="brand-row"><div class="brand-mark">H</div><strong>Hypnofy</strong><button class="icon-button" title="Collapse sidebar" on:click={() => sidebarOpen = false}>‹</button></div>
      <button class="new-button" on:click={newScript}><span>＋</span> New script <kbd>Ctrl N</kbd></button>
      <label class="search"><span>⌕</span><input class="search-input" bind:value={search} on:input={searchChanged} placeholder="Search scripts" aria-label="Search scripts" /></label>
      <nav>
        <button class:active={selectedView === "all"} on:click={() => chooseView("all")}><span>◫</span> All scripts</button>
        <button class:active={selectedView === "favorites"} on:click={() => chooseView("favorites")}><span>☆</span> Favorites</button>
        <div class="nav-label"><span>Collections</span><button title="New collection" on:click={createCollection}>＋</button></div>
        {#each collections as collection}
          <button class:active={selectedView === `collection:${collection.id}`} on:click={() => chooseView(`collection:${collection.id}`)}><span class="dot"></span><span class="nav-name">{collection.name}</span><small>{collection.scriptCount}</small></button>
        {/each}
        <button class:active={selectedView === "trash"} on:click={() => chooseView("trash")}><span>♲</span> Trash</button>
      </nav>
      <div class="sidebar-actions"><button on:click={importScript}>Import text</button><button on:click={importSound}>Add sound</button><button on:click={openSettings}>⚙ Settings</button></div>
    </aside>
  {/if}

  <section class="script-list" class:hidden={focusMode || selectedView === "settings"} aria-label="Scripts">
    <div class="list-heading"><button class="icon-button" title="Show sidebar" on:click={() => sidebarOpen = true} class:hidden={sidebarOpen}>☰</button><div><h2>{selectedView === "favorites" ? "Favorites" : selectedView === "trash" ? "Trash" : collections.find(c => `collection:${c.id}` === selectedView)?.name ?? "All scripts"}</h2><span>{scripts.length} {scripts.length === 1 ? "script" : "scripts"}</span></div></div>
    <div class="script-items">
      {#each scripts as script}
        <button class="script-card" class:active={current?.id === script.id} on:click={() => loadScript(script.id)}><span class="card-title">{script.title}</span><span class="card-meta">{script.collectionName}<time>{displayDate(script.updatedAt)}</time></span></button>
      {:else}<div class="empty-list"><span>☾</span><p>No scripts here yet.</p><button on:click={newScript}>Create one</button></div>{/each}
    </div>
  </section>

  <main class="workspace">
    {#if loading}<div class="center-state"><span class="spinner"></span>Opening your library…</div>
    {:else if selectedView === "settings" && settingsView}
      <section class="settings-page">
        <header><div><h1>Settings</h1><p>Local models, storage, and verified voice assets</p></div><button class="play-button" on:click={saveSettingsPage} disabled={settingsSaving}>{settingsSaving ? "Saving…" : "Save settings"}</button></header>
        <div class="settings-card"><h2>Script generation</h2><div class="two-cols"><label>Default provider<AppSelect value={settingsView.preferences.defaultProvider} items={[{value:"ollama",label:"Ollama"},{value:"gguf",label:"Local GGUF"}]} ariaLabel="Default model provider" onChange={(value) => { if (settingsView) settingsView.preferences.defaultProvider = String(value); }} /></label><label>Default model<input bind:value={settingsView.preferences.defaultModel} placeholder="gemma3:latest" /></label></div><label>Keep model warm <span>{settingsView.preferences.keepModelMinutes} minutes</span><input type="range" min="0" max="30" step="1" bind:value={settingsView.preferences.keepModelMinutes} /></label><label>GGUF model<div class="path-field"><input readonly value={settingsView.preferences.ggufModelPath} /><button on:click={() => chooseSettingFile("gguf")}>Choose…</button></div></label><label>llama.cpp server<div class="path-field"><input readonly value={settingsView.preferences.llamaServerPath} /><button on:click={() => chooseSettingFile("server")}>Choose…</button></div></label></div>
        <div class="settings-card"><h2>Storage</h2><label>Application data<input readonly value={settingsView.dataDirectory} /></label><p class="settings-note">Scripts live in SQLite. Narration chunks, ambience, models, and downloads stay beside it.</p></div>
        <div class="settings-card"><h2>Verified voice assets</h2>{#each settingsView.assets as asset}<div class="asset-row"><div><strong>{asset.id}</strong><span>{asset.kind} · {asset.version} · {(asset.bytes / 1e6).toFixed(0)} MB</span></div><code title={asset.sha256}>{asset.sha256.slice(0, 12)}…</code></div>{/each}<p class="settings-note">Download URLs and checksums come from <code>src-tauri/assets/asset-manifest.json</code>, not application code.</p></div>
      </section>
    {:else if current}
      <header class="editor-header">
        <button class="icon-button mobile-menu" on:click={() => sidebarOpen = !sidebarOpen}>☰</button>
        <input class="title-input" bind:value={current.title} on:input={changed} aria-label="Script title" />
        <div class="header-select"><AppSelect value={current.collectionId} items={collections.map(collection => ({ value: collection.id, label: collection.name }))} ariaLabel="Collection" onChange={(value) => { if (current) { current.collectionId = String(value); changed(); } }} /></div>
        <span class:failed={saveState === "Save failed"} class="save-state">{saveState}</span>
        <button class="quiet-button" on:click={openGeneration}>✦ Generate</button>
        <button class="play-button" on:click={play} disabled={!current.body.trim()}>▶ Play</button>
        <button class="icon-button" title="Focus mode" on:click={() => focusMode = !focusMode}>{focusMode ? "⊡" : "⊙"}</button>
        <button class="icon-button" title={current.favorite ? "Remove favorite" : "Favorite"} on:click={() => { if(current){ current.favorite = !current.favorite; changed(); } }}>{current.favorite ? "★" : "☆"}</button>
        <details class="more"><summary aria-label="More actions">•••</summary><div><button on:click={duplicateCurrent}>Duplicate</button><button on:click={exportScript}>Export text</button>{#if current.deletedAt}<button on:click={restoreCurrent}>Restore</button>{:else}<button class="danger" on:click={moveToTrash}>Move to Trash</button>{/if}</div></details>
      </header>
      <div class="editor-wrap">
        {#if generationOpen}
          <section class="inline-ai" aria-label="Generate a draft">
            <div class="inline-ai-prompt"><span>✦</span><input bind:value={intention} placeholder="Describe the script you want…" aria-label="Generation intention" /><button class="icon-button" on:click={() => generationOpen = false} disabled={generationBusy}>×</button></div>
            <div class="inline-ai-options"><AppSelect value={minutes} items={[5,10,15,20,30].map(value => ({ value, label: `${value} min` }))} ariaLabel="Duration" onChange={(value) => minutes = Number(value)} /><AppSelect value={modelKey} items={models.map(model => ({ value: `${model.provider}|${model.id}`, label: model.label, detail: model.available ? "Ready" : model.reason ?? "Unavailable" }))} ariaLabel="Text generation model" onChange={(value) => { modelKey = String(value); generationError = ""; }} /><input bind:value={tone} aria-label="Tone" placeholder="Tone" /></div>
            {#each models.filter(model => `${model.provider}|${model.id}` === modelKey) as selectedModel}{#if !selectedModel.available}<div class="model-status"><span>{selectedModel.reason}</span>{#if selectedModel.provider === "gguf" && selectedModel.reason?.includes("runtime")}<button type="button" on:click={installModelRuntime} disabled={modelRuntimeInstalling}>{modelRuntimeInstalling ? "Installing…" : "Install runtime · 18 MB"}</button>{/if}</div>{/if}{/each}
            <textarea class="ai-guidance" rows="2" bind:value={instructions} placeholder="Optional guidance, imagery, or boundaries"></textarea>
            {#if draft}<textarea class="inline-draft" readonly bind:value={draft} aria-label="Generated draft"></textarea>{/if}
            {#if generationError}<p class="error-message">{generationError}</p>{/if}
            <div class="inline-ai-actions"><label class="checkbox"><input type="checkbox" bind:checked={includeCurrent} /> Use current text</label>{#if generationBusy}<button class="quiet-button" on:click={cancelGeneration}>Cancel</button>{:else if draft}<button class="quiet-button" on:click={generate}>Try again</button><button class="play-button" on:click={acceptDraft}>Replace with draft</button>{:else}<button class="play-button" on:click={generate} disabled={!modelKey || !intention.trim()}>Generate</button>{/if}</div>
          </section>
        {/if}
        <textarea class="script-editor" bind:value={current.body} on:input={changed} spellcheck="true" placeholder="Begin your script…" aria-label="Hypnosis script"></textarea><div class="editor-footer"><button on:click={insertPause}>＋ Pause</button><span>{current.body.trim() ? current.body.trim().split(/\s+/).length : 0} words · ~{Math.max(1, Math.round((current.body.trim().split(/\s+/).length || 0) / 115))} min</span><button on:click={() => sessionOpen = !sessionOpen}>Session {sessionOpen ? "⌃" : "⌄"}</button></div>
      </div>
      {#if sessionOpen}
        <section class="session-panel" aria-label="Session settings">
          <div class="panel-title"><div><h3>Session</h3><p>Voice and atmosphere for this script</p></div><button class="icon-button" on:click={() => sessionOpen = false}>×</button></div>
          {#if !speech.installed}
            <div class="install-card"><div><strong>Local voices</strong><p>{speech.engineVersion || "Kokoro"} · one-time {(speech.downloadBytes / 1e6).toFixed(0)} MB download</p></div><button on:click={installVoices} disabled={speech.downloading}>{speech.downloading ? `${Math.round(speechProgress * 100)}%` : "Install"}</button></div>
          {/if}
          <label>Voice<AppSelect value={current.session.voice} items={voices.map(voice => ({ value: voice.id, label: voice.name, detail: `${voice.accent} · ${voice.gender}` }))} ariaLabel="Narration voice" onChange={(value) => { if (current) { current.session.voice = String(value); changed(); } }} /></label>
          <div class="range-row"><label>Speed <output>{current.session.speed.toFixed(2)}×</output><input type="range" min="0.6" max="1.3" step="0.05" bind:value={current.session.speed} on:input={changed} /></label><label>Paragraph pause <output>{current.session.paragraphPause.toFixed(1)}s</output><input type="range" min="0" max="8" step="0.5" bind:value={current.session.paragraphPause} on:input={changed} /></label></div>
          <label>Background sound<AppSelect value={current.session.ambienceId} items={[{ value: null, label: "None" }, ...ambience.map(item => ({ value: item.id, label: item.name }))]} ariaLabel="Background sound" onChange={(value) => { if (current) { current.session.ambienceId = value ? String(value) : null; changed(); } }} /></label>
          <div class="range-row"><label>Narration <output>{Math.round(current.session.narrationVolume * 100)}%</output><input type="range" min="0" max="1.2" step="0.01" bind:value={current.session.narrationVolume} on:input={updateVolumes} /></label><label>Background <output>{Math.round(current.session.ambienceVolume * 100)}%</output><input type="range" min="0" max="0.35" step="0.005" bind:value={current.session.ambienceVolume} on:input={updateVolumes} /></label></div>
          {#if speechMessage}<p class="progress-label">{speechMessage}{#if speechProgress > 0 && speechProgress < 1} · {Math.round(speechProgress * 100)}%{/if}</p>{/if}
        </section>
      {/if}
    {:else}<div class="welcome"><div class="moon">☾</div><h1>Your quiet space</h1><p>Create a script or choose one from your library.</p><button class="play-button" on:click={newScript}>＋ New script</button></div>{/if}
  </main>
</div>

{#if !player.finished}
  <div class="player"><button class="player-main" on:click={togglePlayer}>{player.playing ? "Ⅱ" : "▶"}</button><div class="player-info"><strong>{current?.title}</strong><input type="range" min="0" max={player.durationSeconds || 1} step="0.1" value={player.positionSeconds} on:change={seekPlayer} /><span>{formatTime(player.positionSeconds)} / {formatTime(player.durationSeconds)}</span></div><button class="icon-button" title="Stop" on:click={stopPlayer}>■</button></div>
{/if}

{#if settingsOpen && settingsView}
  <div class="modal-backdrop" role="presentation" on:click={(event) => { if (event.target === event.currentTarget) settingsOpen = false; }}>
    <div class="settings-modal" role="dialog" aria-modal="true" aria-label="Settings">
      <header><div><h2>Settings</h2><p>Models, storage, and local assets</p></div><button class="icon-button" on:click={() => settingsOpen = false}>×</button></header>
      <div class="settings-card"><h2>Generation</h2><div class="two-cols"><label>Default provider<AppSelect value={settingsView.preferences.defaultProvider} items={[{value:"ollama",label:"Ollama"},{value:"gguf",label:"Local GGUF"}]} ariaLabel="Default model provider" onChange={(value) => { if(settingsView) settingsView.preferences.defaultProvider=String(value); }} /></label><label>Default model<input bind:value={settingsView.preferences.defaultModel} /></label></div><label>Keep loaded <span>{settingsView.preferences.keepModelMinutes} minutes</span><input type="range" min="0" max="30" bind:value={settingsView.preferences.keepModelMinutes} /></label></div>
      <div class="settings-card"><h2>Local files</h2><label>Optional GGUF model<div class="path-field"><input readonly value={settingsView.preferences.ggufModelPath} placeholder="No model linked" /><button on:click={() => chooseSettingFile("gguf")}>Choose</button></div></label><label>llama.cpp runtime<div class="path-field"><input readonly value={settingsView.preferences.llamaServerPath} /><button on:click={() => chooseSettingFile("server")}>Choose</button></div></label><label>Hypnofy data<input readonly value={settingsView.dataDirectory} /></label><p class="settings-note">Hypnofy uses Ollama by default. Optional GGUF files are read from the location you choose. Voice assets and narration live in the Hypnofy data folder.</p></div>
      <footer><button class="quiet-button" on:click={() => settingsOpen = false}>Cancel</button><button class="play-button" on:click={async () => { await saveSettingsPage(); settingsOpen = false; }} disabled={settingsSaving}>{settingsSaving ? "Saving…" : "Save"}</button></footer>
    </div>
  </div>
{/if}

{#if error}<div class="error-toast" role="alert"><span>{error}</span><button on:click={() => error = ""}>×</button></div>{/if}
{#if toast}<div class="toast" role="status">{toast}</div>{/if}
