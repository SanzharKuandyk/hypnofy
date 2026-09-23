<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { client } from "../../bridge/client";
  import AppSelect from "../../components/AppSelect.svelte";
  import type { Ambience, Script, TimelineSegment } from "../../bridge/types";
  export let script: Script;
  export let onChange: () => void;
  export let onClose: () => void;
  let ambience: Ambience[] = [];
  let segments: TimelineSegment[] = [];
  let error = "";
  let timer: ReturnType<typeof setTimeout> | null = null;
  let planVersion = 0;
  onMount(() => { client.ambience().then(value => ambience = value).catch(value => error = String(value)); refreshPlan(); });
  onDestroy(() => { if (timer) clearTimeout(timer); });
  $: { script.document; script.session.paragraphPause; if (timer) clearTimeout(timer); timer = setTimeout(refreshPlan, 350); }
  async function refreshPlan() { const version = ++planVersion; try { const result = await client.planTiming(script.document, script.session.paragraphPause); if (version === planVersion) { segments = result; error = ""; } } catch (value) { if (version === planVersion) error = String(value); } }
  async function importSound() { const path = await open({ multiple: false, filters: [{ name: "Audio", extensions: ["wav", "mp3", "flac", "ogg"] }] }); if (!path) return; try { await client.importAmbience(path); ambience = await client.ambience(); } catch (value) { error = String(value); } }
</script>
<aside class="inspector" aria-label="Session timing"><header><strong>Session</strong><button on:click={onClose} aria-label="Close session inspector">×</button></header><p class="muted">Narration is not configured. Timing is ready for a future pre-generated voice.</p>
  <label>Paragraph pause <span>{script.session.paragraphPause.toFixed(1)} s</span><input type="range" min="0" max="8" step="0.5" bind:value={script.session.paragraphPause} on:input={onChange} /></label>
  <label>Background sound<AppSelect value={script.session.ambienceId} items={[{value: null, label: "None"}, ...ambience.map(item => ({value: item.id, label: item.name}))]} ariaLabel="Background sound" onChange={value => { script.session.ambienceId = value ? String(value) : null; onChange(); }} /></label><button class="outline" on:click={importSound}>＋ Add sound</button>
  <h2>Timing plan</h2><p class="muted">Review each pause before rendering. AI pause suggestions will require approval.</p><div class="timeline">{#each segments as segment}<div class:pause={segment.kind === "pause"}>{segment.kind === "pause" ? `Pause · ${segment.seconds}s` : segment.text?.slice(0, 100)}</div>{/each}</div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</aside>
<style>
  .inspector{width:280px;flex:none;padding:20px 16px;overflow:auto;border-left:1px solid var(--border);background:var(--sidebar)}header{display:flex;align-items:center;justify-content:space-between}header button{border:0;background:none;color:var(--muted);font-size:20px}.muted{color:var(--muted);font-size:12px;line-height:1.6}label{display:grid;gap:8px;margin:24px 0;font-size:12px}label span{float:right;color:var(--muted)}input{width:100%;accent-color:var(--accent)}.outline{padding:8px 10px;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--text)}h2{font-size:14px;margin-top:32px}.timeline{display:grid;gap:8px;margin-top:12px}.timeline div{padding:9px;border-left:2px solid var(--border);font-size:12px;color:var(--muted);line-height:1.4}.timeline div.pause{border-color:var(--accent);color:var(--accent)}.error{color:var(--danger);font-size:12px}@media(max-width:1100px){.inspector{position:absolute;z-index:10;right:0;top:64px;bottom:0;border-top:1px solid var(--border)}}
</style>
