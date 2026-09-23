<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { client } from "./bridge/client";
  import type { Collection, ModelId, Script, ScriptDocument, ScriptSummary, SettingsView } from "./bridge/types";
  import LibrarySidebar from "./features/library/LibrarySidebar.svelte";
  import type ScriptEditor from "./features/editor/ScriptEditor.svelte";
  import type GenerationComposer from "./features/generation/GenerationComposer.svelte";
  import ModelsPage from "./features/models/ModelsPage.svelte";
  import SettingsPage from "./features/models/SettingsPage.svelte";
  import SessionInspector from "./features/session/SessionInspector.svelte";
  import AppSelect from "./components/AppSelect.svelte";

  let page: "library" | "models" | "settings" = "library";
  let view = "all";
  let collections: Collection[] = [];
  let scripts: ScriptSummary[] = [];
  let current: Script | null = null;
  let settings: SettingsView | null = null;
  let search = "";
  let loading = true;
  let error = "";
  let saveState = "Saved";
  let sidebarOpen = true;
  let inspectorOpen = false;
  let aiOpen = false;
  let aiSelection = "";
  let aiAnchor = 0;
  let editorRef: ScriptEditor | undefined;
  let EditorComponent: typeof ScriptEditor | null = null;
  let ComposerComponent: typeof GenerationComposer | null = null;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let saving: Promise<boolean> | null = null;
  let editVersion = 0;
  let savedVersion = 0;
  let closing = false;
  let unlistenClose: (() => void) | undefined;

  const collectionId = () => view.startsWith("collection:") ? view.slice(11) : null;
  function documentText(document: ScriptDocument): string {
    const walk = (node: unknown): string => {
      if (!node || typeof node !== "object") return "";
      const value = node as { type?: string; text?: string; content?: unknown[]; attrs?: { seconds?: number } };
      if (value.type === "pause") return `[pause:${value.attrs?.seconds ?? 5}s]`;
      if (value.type === "text") return value.text ?? "";
      return (value.content ?? []).map(walk).join(value.type === "doc" ? "\n\n" : "");
    };
    return walk(document);
  }
  function fail(value: unknown) { error = value instanceof Error ? value.message : String(value); }
  function changed() { editVersion++; saveState = "Unsaved"; if (saveTimer) clearTimeout(saveTimer); saveTimer = setTimeout(() => { void flushSave(); }, 400); }
  async function flushSave(): Promise<boolean> {
    if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; }
    if (!current || editVersion === savedVersion) return true;
    if (saving) { const prior = await saving; return prior && (editVersion === savedVersion || flushSave()); }
    const snapshot = structuredClone(current);
    const version = editVersion;
    saveState = "Saving…";
    saving = (async () => {
      try {
        const result = await client.updateScript(snapshot);
        if (current?.id === result.id) { current.revision = result.revision; current.updatedAt = result.updatedAt; current.body = documentText(current.document); current.collectionName = collections.find(item => item.id === current?.collectionId)?.name ?? result.collectionName; }
        const summary = scripts.find(item => item.id === result.id);
        if (summary) Object.assign(summary, { title: result.title, updatedAt: result.updatedAt, revision: result.revision, favorite: result.favorite, collectionId: result.collectionId, collectionName: result.collectionName });
        savedVersion = version;
        saveState = savedVersion === editVersion ? "Saved" : "Unsaved";
        return true;
      } catch (value) { saveState = "Save failed"; fail(value); return false; }
      finally { saving = null; }
    })();
    const result = await saving;
    return result && (editVersion === savedVersion || flushSave());
  }
  async function refreshList() { const [items, groups] = await Promise.all([client.scripts(collectionId(), search, view === "favorites", view === "trash"), client.collections()]); scripts = items; collections = groups; }
  async function showPage(next: typeof page) { if (!(await flushSave())) return; if (next !== "library" && !settings) { try { settings = await client.getSettings(); } catch (value) { fail(value); return; } } page = next; aiOpen = false; inspectorOpen = false; }
  async function chooseView(next: string) { if (!(await flushSave())) return; view = next; current = null; aiOpen = false; try { await refreshList(); } catch (value) { fail(value); } }
  async function chooseScript(id: string) { if (!(await flushSave())) return; try { current = await client.script(id); editVersion = savedVersion = 0; saveState = "Saved"; aiOpen = false; error = ""; } catch (value) { fail(value); } }
  async function newScript() { if (!(await flushSave())) return; try { current = await client.createScript(collectionId()); view = `collection:${current.collectionId}`; editVersion = savedVersion = 0; await refreshList(); page = "library"; aiOpen = false; } catch (value) { fail(value); } }
  async function createCollection() { const name = window.prompt("Collection name"); if (!name?.trim()) return; try { const result = await client.createCollection(name); await chooseView(`collection:${result.id}`); } catch (value) { fail(value); } }
  async function importScript() { const path = await open({ multiple: false, filters: [{ name: "Text", extensions: ["txt", "md"] }] }); if (!path || !(await flushSave())) return; try { current = await client.importText(path, collectionId()); view = `collection:${current.collectionId}`; editVersion = savedVersion = 0; await refreshList(); } catch (value) { fail(value); } }
  async function exportScript() { if (!current || !(await flushSave())) return; const path = await save({ defaultPath: `${current.title.replace(/[<>:"/\\|?*]/g, "-")}.txt`, filters: [{ name: "Text", extensions: ["txt"] }] }); if (!path) return; try { await client.exportText(current.id, path); } catch (value) { fail(value); } }
  async function duplicateScript() { if (!current || !(await flushSave())) return; try { current = await client.duplicateScript(current.id); editVersion = savedVersion = 0; await refreshList(); } catch (value) { fail(value); } }
  async function trashScript() { if (!current || !(await flushSave())) return; try { await client.setDeleted(current.id, true); current = null; await refreshList(); } catch (value) { fail(value); } }
  async function restoreScript() { if (!current) return; try { await client.setDeleted(current.id, false); current = null; await refreshList(); } catch (value) { fail(value); } }
  function updateDocument(document: ScriptDocument) { if (!current) return; current.document = document; current.body = documentText(document); changed(); }
  function openAi(selection = "", anchor = 0) { aiSelection = selection; aiAnchor = anchor; aiOpen = true; if (!ComposerComponent) void import("./features/generation/GenerationComposer.svelte").then(module => ComposerComponent = module.default).catch(fail); }
  function applyDraft(text: string, replace: boolean, baseRevision: number, modelId: ModelId) { if (!current) return; if (current.revision !== baseRevision) { error = "The script changed while drafting. Keep the draft open and reload before inserting."; return; } if (!editorRef) { error = "The editor is unavailable. Keep this draft open and retry."; return; } current.lastModelId = modelId; editorRef.insertDraft(text, replace); changed(); aiOpen = false; }
  function handleKey(event: KeyboardEvent) {
    if (!(event.ctrlKey || event.metaKey)) return;
    const key = event.key.toLowerCase();
    if (key === "n") { event.preventDefault(); void newScript(); }
    if (key === "s") { event.preventDefault(); void flushSave(); }
    if (key === "k") { event.preventDefault(); document.querySelector<HTMLInputElement>(".search input")?.focus(); }
    if (key === "j" && current && page === "library") { event.preventDefault(); openAi(); }
  }
  function searchChanged(value: string) { search = value; if (searchTimer) clearTimeout(searchTimer); searchTimer = setTimeout(() => { void refreshList().catch(fail); }, 160); }
  onMount(async () => {
    window.addEventListener("keydown", handleKey);
    void import("./features/editor/ScriptEditor.svelte").then(module => EditorComponent = module.default).catch(fail);
    const handle = getCurrentWindow();
    unlistenClose = await handle.onCloseRequested(async event => {
      event.preventDefault(); if (closing) return; closing = true;
      if (await flushSave()) await handle.destroy(); else closing = false;
    });
    try { await refreshList(); if (scripts[0]) await chooseScript(scripts[0].id); }
    catch (value) { fail(value); } finally { loading = false; }
  });
  onDestroy(() => { window.removeEventListener("keydown", handleKey); unlistenClose?.(); if (saveTimer) clearTimeout(saveTimer); if (searchTimer) clearTimeout(searchTimer); });
</script>

<svelte:head><meta name="description" content="Hypnofy — your local script library" /></svelte:head>
<div class="shell" class:collapsed={!sidebarOpen || page !== "library"}>
  <aside class="rail" aria-label="Main navigation"><button class:active={page === "library"} title="Library" aria-label="Library" on:click={() => showPage("library")}>▤</button><button class:active={page === "models"} title="Models" aria-label="Models" on:click={() => showPage("models")}>◉</button><button class:active={page === "settings"} class="rail-bottom" title="Settings" aria-label="Settings" on:click={() => showPage("settings")}>⚙</button></aside>
  {#if page === "library" && sidebarOpen}<LibrarySidebar {collections} {scripts} currentId={current?.id ?? null} {view} {search} onView={chooseView} onSearch={searchChanged} onScript={chooseScript} onNew={newScript} onCollection={createCollection} onImport={importScript} />{/if}
  <div class="main"><header class="top"><button class="mobile-nav" on:click={() => sidebarOpen = !sidebarOpen} aria-label="Toggle library">☰</button><span>Hypnofy <b>/</b> {page === "library" ? current?.title ?? "Library" : page === "models" ? "Models" : "Settings"}</span><button class="top-action" on:click={newScript}>＋ New script</button></header>
    {#if loading}<div class="state">Opening your library…</div>
    {:else if page === "models" && settings}<ModelsPage onSettings={value => settings = value} />
    {:else if page === "settings" && settings}<SettingsPage {settings} onSettings={value => settings = value} />
    {:else if current}<div class="document-layout"><main class="document"><div class="document-meta"><span>{current.collectionName}</span><span>·</span><span class:failed={saveState === "Save failed"}>{saveState}</span></div><div class="title-row"><input class="title" bind:value={current.title} on:input={changed} aria-label="Script title" /><button class:favorite={current.favorite} title="Favorite" on:click={() => { if (current) { current.favorite = !current.favorite; changed(); } }}>{current.favorite ? "★" : "☆"}</button></div><div class="document-actions"><AppSelect value={current.collectionId} items={collections.map(item => ({ value: item.id, label: item.name }))} ariaLabel="Collection" onChange={value => { if (current) { current.collectionId = String(value); changed(); } }} /><button on:click={() => openAi()}>✦ Ask AI</button><button on:click={() => inspectorOpen = !inspectorOpen}>Session</button><details><summary>•••</summary><div class="menu"><button on:click={duplicateScript}>Duplicate</button><button on:click={exportScript}>Export text</button>{#if current.deletedAt}<button on:click={restoreScript}>Restore</button>{:else}<button class="danger" on:click={trashScript}>Move to Trash</button>{/if}</div></details></div>{#if aiOpen && ComposerComponent}<ComposerComponent script={current} selectedText={aiSelection} anchor={aiAnchor} onClose={() => aiOpen = false} onAccept={applyDraft} beforeGenerate={flushSave} />{/if}{#if EditorComponent}<EditorComponent bind:this={editorRef} script={current} onChange={updateDocument} onAi={openAi} />{:else}<div class="state">Opening editor…</div>{/if}</main>{#if inspectorOpen}<SessionInspector script={current} onChange={changed} onClose={() => inspectorOpen = false} />{/if}</div>
    {:else}<div class="empty"><span>◐</span><h1>Your quiet space</h1><p>Choose a script or start a new one.</p><button on:click={newScript}>＋ New script</button></div>{/if}
  </div>
</div>
{#if error}<div class="error-toast" role="alert"><span>{error}</span><button on:click={() => error = ""} aria-label="Dismiss error">×</button></div>{/if}
