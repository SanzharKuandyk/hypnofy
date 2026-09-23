<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { client } from "../../bridge/client";
  import type { ModelRecord, SettingsView } from "../../bridge/types";
  export let onSettings: (value: SettingsView) => void;
  let models: ModelRecord[] = [];
  let key = "";
  let search = "";
  let busy = false;
  let message = "";
  let connected = false;
  let settings: SettingsView | null = null;
  $: visible = models.filter(model => `${model.name} ${model.provider}`.toLowerCase().includes(search.toLowerCase()));
  $: remote = visible.filter(model => model.provider === "openrouter").slice(0, search.trim() ? 80 : 30);
  onMount(() => { refresh(); client.getSettings().then(value => settings = value).catch(error => message = String(error)); client.openRouterConnected().then(value => connected = value).catch(() => connected = false); });
  async function refresh() { busy = true; try { models = await client.models(); message = ""; } catch (error) { message = String(error); } finally { busy = false; } }
  async function connect() { if (!key.trim()) return; busy = true; try { await client.setOpenRouterKey(key); key = ""; connected = true; await refresh(); } catch (error) { message = String(error); } finally { busy = false; } }
  async function disconnect() { try { await client.clearOpenRouterKey(); connected = false; await refresh(); } catch (error) { message = String(error); } }
  async function linkGguf(existingId?: string) { const path = await open({ multiple: false, filters: [{ name: "GGUF model", extensions: ["gguf"] }] }); if (!path) return; try { await client.linkGguf(path, existingId); await refresh(); } catch (error) { message = String(error); } }
  async function unlinkGguf(id: string) { try { await client.unlinkGguf(id); await refresh(); } catch (error) { message = String(error); } }
  async function verifyGguf(id: string) { busy = true; message = "Loading the GGUF model for compatibility verification…"; try { await client.verifyGguf(id); await refresh(); message = "GGUF model verified with the installed runtime"; } catch (error) { message = String(error); } finally { busy = false; } }
  async function installRuntime() { busy = true; try { await client.installModelRuntime(); onSettings(await client.getSettings()); await refresh(); } catch (error) { message = String(error); } finally { busy = false; } }
  function isDefault(model: ModelRecord): boolean { return settings?.preferences.defaultProvider === model.provider && settings.preferences.defaultModel === model.id.slice(model.provider.length + 1); }
  async function setDefault(model: ModelRecord) {
    if (!settings || model.status !== "ready") return;
    try {
      settings = await client.saveSettings({ ...settings.preferences, defaultProvider: model.provider, defaultModel: model.id.slice(model.provider.length + 1) });
      onSettings(settings);
      message = `${model.name} is the default for new drafts`;
    } catch (error) { message = String(error); }
  }
</script>
<div class="page"><div class="eyebrow">LOCAL & REMOTE</div><header><div><h1>Models</h1><p>Choose a model for each draft. Your library opens without one.</p></div><button on:click={refresh} disabled={busy}>↻ Refresh</button></header>
  <label class="search">⌕ <input type="search" bind:value={search} placeholder="Search models" aria-label="Search models" /></label>
  <h2>Installed <small>Ollama</small></h2>
  {#each visible.filter(model => model.provider === "ollama") as model}<div class="model"><div><strong>{model.name}</strong><small>{model.reason ?? "Local · ready"}</small></div><div class="model-actions"><span class:ready={model.status === "ready"}>{model.status}</span>{#if model.status === "ready"}<button disabled={isDefault(model)} on:click={() => setDefault(model)}>{isDefault(model) ? "Default" : "Set default"}</button>{/if}</div></div>{/each}
  <h2>Linked <small>GGUF</small></h2>
  {#each visible.filter(model => model.provider === "gguf") as model}<div class="model"><div><strong>{model.name}</strong><small>{model.reason ?? "Local file · ready"}</small></div><div class="model-actions"><span class:ready={model.status === "ready"}>{model.status}</span>{#if model.status === "ready"}<button disabled={isDefault(model)} on:click={() => setDefault(model)}>{isDefault(model) ? "Default" : "Set default"}</button>{:else if model.recovery === "verify"}<button disabled={busy} on:click={() => verifyGguf(model.id.slice(5))}>Verify</button>{/if}<button on:click={() => linkGguf(model.id.slice(5))}>Relink</button><button on:click={() => unlinkGguf(model.id.slice(5))}>Unlink</button></div></div>{/each}
  <div class="actions"><button on:click={() => linkGguf()}>Link GGUF file</button><button on:click={installRuntime} disabled={busy}>Install verified runtime</button></div>
  <h2>Remote <small>Opt in</small></h2><div class="connection"><strong>OpenRouter</strong><p>Only your prompt and explicitly selected script text are sent. Model providers have different retention policies.</p><div class="actions"><input type="password" bind:value={key} placeholder={connected ? "Connected · enter a new key to replace" : "OpenRouter API key"} aria-label="OpenRouter API key" /><button on:click={connect} disabled={busy || !key.trim()}>{connected ? "Replace key" : "Connect"}</button>{#if connected}<button on:click={disconnect}>Disconnect</button>{/if}</div></div>
  {#each remote as model}<div class="model"><div><strong>{model.name}</strong><small>{model.reason ?? model.cost ?? "Remote text generation"}</small></div><div class="model-actions"><span class:ready={model.status === "ready"}>{model.status}</span>{#if model.status === "ready"}<button disabled={isDefault(model)} on:click={() => setDefault(model)}>{isDefault(model) ? "Default" : "Set default"}</button>{/if}</div></div>{/each}
  {#if visible.filter(model => model.provider === "openrouter").length > remote.length}<p>Showing {remote.length} models. Search to narrow the catalog.</p>{/if}
  <div class="connection"><strong>OpenCode</strong><p>Requires a verified isolated profile, no tools, and temporary sessions. It remains unavailable until this installation passes that check.</p>{#each models.filter(model => model.provider === "opencode") as model}<small>{model.reason}</small>{/each}</div>
  {#if message}<p class="error" role="alert">{message}</p>{/if}
</div>
<style>
  .page{max-width:850px;margin:0 auto;padding:54px 32px 90px}.eyebrow{font-size:11px;letter-spacing:.1em;color:var(--accent)}header{display:flex;align-items:start;justify-content:space-between;gap:20px}h1{font-size:36px;margin:7px 0;letter-spacing:-.04em}h2{font-size:16px;margin:32px 0 10px}h2 small{font-size:12px;color:var(--muted);font-weight:400}p{color:var(--muted)}button{padding:8px 12px;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--text)}button:disabled{opacity:.5}.search{display:flex;gap:8px;padding:8px 11px;margin-top:24px;border:1px solid var(--border);border-radius:8px;color:var(--muted)}input{border:0;background:none;color:var(--text);outline:0}.search input{width:100%}.model{display:flex;justify-content:space-between;gap:20px;padding:12px 14px;border:1px solid var(--border);border-radius:9px;margin:7px 0;background:var(--sidebar)}.model div{display:grid;gap:3px;min-width:0}.model .model-actions{display:flex;align-items:center;gap:7px}.model .model-actions button{font-size:11px;padding:5px 7px}.model small,.connection small{color:var(--muted)}.model span{font-size:11px;color:var(--muted)}.model span.ready{color:var(--accent)}.actions{display:flex;flex-wrap:wrap;gap:8px;margin:12px 0}.actions input{min-width:260px;padding:8px;border:1px solid var(--border);border-radius:8px;background:var(--background)}.connection{padding:16px;border:1px solid var(--border);border-radius:9px;background:var(--sidebar);margin:12px 0}.connection p{font-size:12px}.error{color:var(--danger)}
</style>
