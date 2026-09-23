<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { client } from "../../bridge/client";
  import ModelPicker from "../../components/ui/ModelPicker.svelte";
  import type { DraftRequest, JobEvent, ModelId, ModelRecord, Script } from "../../bridge/types";

  export let script: Script;
  export let selectedText = "";
  export let anchor = 0;
  export let onClose: () => void;
  export let onAccept: (text: string, replace: boolean, revision: number, modelId: ModelId) => void;
  export let beforeGenerate: () => Promise<boolean>;
  let models: ModelRecord[] = [];
  let modelId: ModelId | "" = "";
  let prompt = "";
  let draft = "";
  let jobId = "";
  let busy = false;
  let error = "";
  let baseRevision = script.revision;
  let operation: DraftRequest["operation"] = "new";
  let listenerReady: Promise<unknown> = Promise.resolve();
  let cancelRequested = false;
  $: operation = selectedText ? "rewrite-selection" : script.body.trim() ? "continue" : "new";
  $: selectedModel = models.find(model => model.id === modelId);

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    Promise.all([client.models(), client.getSettings()]).then(([items, settings]) => {
      const globalDefault = `${settings.preferences.defaultProvider}:${settings.preferences.defaultModel}`;
      const stored = script.lastModelId ?? globalDefault;
      if (stored && !items.some(item => item.id === stored)) {
        const provider = stored.split(":", 1)[0] as ModelRecord["provider"];
        const status = provider === "ollama" ? "offline" : provider === "openrouter" ? "needs-auth" : "missing";
        const recovery = provider === "ollama" ? "connect" : provider === "openrouter" ? "authenticate" : "relink";
        items = [...items, { id: stored as ModelId, name: stored.slice(provider.length + 1), provider, capability: "text", source: provider === "gguf" ? "linked" : "discovered", status, reason: "Previously selected model is unavailable. Refresh Models for details.", recovery }];
      }
      models = items;
      const preferred = items.find(item => item.id === script.lastModelId)
        ?? items.find(item => item.id === globalDefault)
        ?? items.find(item => item.status === "ready");
      modelId = preferred?.id ?? "";
    }).catch(value => error = String(value));
    listenerReady = listen<JobEvent>("generation-event", ({ payload }) => {
      if (!busy || payload.scriptId !== script.id || payload.baseRevision !== baseRevision) return;
      if (jobId && payload.jobId !== jobId) return;
      if (!jobId) jobId = payload.jobId;
      if (payload.phase === "streaming") draft += payload.delta ?? "";
      if (payload.phase === "failed") { busy = false; error = payload.error?.message ?? "Generation failed"; }
      if (payload.phase === "complete" || payload.phase === "cancelled") busy = false;
    }).then(fn => { if (disposed) fn(); else unlisten = fn; });
    return () => { disposed = true; if (busy) { cancelRequested = true; if (jobId) void client.cancelGeneration(jobId); } unlisten?.(); };
  });
  async function run() {
    if (!modelId || !prompt.trim() || busy) return;
    if (selectedModel?.status !== "ready") { error = selectedModel?.reason ?? "Model unavailable"; return; }
    error = ""; draft = ""; jobId = ""; busy = true; cancelRequested = false;
    if (!(await beforeGenerate())) { busy = false; error = "Save the script before generating"; return; }
    baseRevision = script.revision;
    try { await listenerReady; jobId = await client.generate({ scriptId: script.id, baseRevision, modelId, operation, prompt: prompt.trim(), selectedText: selectedText || undefined, insertionAnchor: anchor }); if (cancelRequested) await client.cancelGeneration(jobId); }
    catch (value) { busy = false; error = String(value); }
  }
  async function cancel() { cancelRequested = true; if (jobId) await client.cancelGeneration(jobId); busy = false; }
</script>

<section class="composer" aria-label="AI draft">
  <header><div><span class="spark">✦</span><strong>Ask AI</strong><small>{selectedText ? "Rewrite selection" : script.body.trim() ? "Continue script" : "New draft"}</small></div><button class="close" on:click={onClose} aria-label="Close AI composer">×</button></header>
  <textarea bind:value={prompt} rows="3" placeholder="Describe the passage you want…" aria-label="AI prompt"></textarea>
  {#if selectedText}<div class="selection">Using selection: “{selectedText.slice(0, 160)}{selectedText.length > 160 ? "…" : ""}”</div>{/if}
  <div class="controls"><div class="picker"><ModelPicker {models} value={modelId} onChange={value => modelId = value} /></div><button class="generate" disabled={busy || !prompt.trim() || !modelId} on:click={run}>{busy ? "Generating…" : draft ? "Retry" : "Generate"}</button>{#if busy}<button class="quiet" on:click={cancel}>Stop</button>{/if}</div>
  {#if selectedModel && selectedModel.status !== "ready"}<p class="error">{selectedModel.reason ?? "Model unavailable"} · Open Models to {selectedModel.recovery ?? "resolve"}.</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if draft}<div class="draft"><div class="draft-label">Draft preview</div><p>{draft}</p><footer><button disabled={busy} on:click={() => modelId && onAccept(draft, false, baseRevision, modelId)}>Insert</button>{#if selectedText}<button disabled={busy} on:click={() => modelId && onAccept(draft, true, baseRevision, modelId)}>Replace selection</button>{/if}<button disabled={busy} on:click={() => draft = ""}>Discard</button></footer></div>{/if}
</section>
<style>
  .composer{position:relative;margin:24px 0;padding:16px;border:1px solid var(--border);border-radius:10px;background:var(--sidebar)}header{display:flex;align-items:center;justify-content:space-between;margin-bottom:12px}header div{display:flex;align-items:center;gap:8px}header small{color:var(--muted)}.spark{color:var(--accent)}.close,.quiet{border:0;background:none;color:var(--muted)}textarea{width:100%;resize:vertical;padding:10px;border:1px solid var(--border);border-radius:8px;background:var(--background);color:var(--text)}.selection{padding:8px 0;color:var(--muted);font-size:12px}.controls{display:flex;align-items:center;gap:8px;margin-top:10px}.picker{min-width:0;flex:1}.generate{padding:8px 12px;border:0;border-radius:8px;background:var(--accent);color:var(--accent-ink);font-weight:650}.generate:disabled{opacity:.5}.error{color:var(--danger);font-size:12px}.draft{padding-top:16px;margin-top:16px;border-top:1px solid var(--border)}.draft-label{font-size:11px;color:var(--accent);text-transform:uppercase;letter-spacing:.08em}.draft p{max-height:280px;overflow:auto;white-space:pre-wrap;line-height:1.7}.draft footer{display:flex;gap:8px}.draft footer button{padding:7px 10px;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--text)}.draft footer button:first-child{background:var(--accent);color:var(--accent-ink);border:0}
</style>
