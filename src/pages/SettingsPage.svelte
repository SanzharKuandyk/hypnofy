<script lang="ts">
  import { onMount } from "svelte";
  import { Moon, Sun, Monitor, Cpu, Link2, FolderOpen } from "@lucide/svelte";
  import { Button, Badge, Field, Dialog } from "../shared/ui";
  import { api } from "../shared/rpc/client";
  import { chooseModelFolder } from "../shared/platform/dialogs";
  import type { Workspace } from "../app/workflows/workspace.svelte";
  import type { SettingsSection } from "../app/routes";
  import type { StorageInfo } from "../domain";

  let { workspace: w }: { workspace: Workspace } = $props();
  const section = $derived(w.route.kind === "settings" ? w.route.section : "appearance");
  let connected = $state(false);
  let key = $state("");
  let storage = $state<StorageInfo | null>(null);
  let disconnect = $state(false);
  let cleanup = $state(false);
  const sections: { id: SettingsSection; name: string }[] = [
    { id: "appearance", name: "Appearance" },
    { id: "providers", name: "AI providers" },
    { id: "voice", name: "Voice & playback" },
    { id: "storage", name: "Storage" },
    { id: "help", name: "Help & safety" },
  ];
  const voices = $derived(
    w.models
      .filter((m) => m.capability === "speech" && m.status === "ready")
      .flatMap((m) => m.voices),
  );

  onMount(() => {
    void w.action(async () => {
      [connected, storage] = await Promise.all([api.providers.connected(), api.settings.storage()]);
    });
  });
</script>

<div class="settings-layout">
  <nav class="settings-nav" aria-label="Settings">
    <h2>Settings</h2>
    {#each sections as item (item.id)}<button
        class="nav-item"
        class:active={section === item.id}
        onclick={() => w.action(() => w.navigate({ kind: "settings", section: item.id }))}
        >{item.name}</button
      >{/each}
  </nav>
  <div class="settings-content">
    {#if section === "appearance"}<header>
        <h1>Appearance</h1>
        <p class="muted">Make this space feel like yours.</p>
      </header>
      <section class="card">
        <h2>Theme</h2>
        <p class="muted">Dark is the default. System follows your Windows preference.</p>
        <div class="theme-options">
          {#each [{ id: "light", name: "Light" }, { id: "dark", name: "Dark" }, { id: "system", name: "System" }] as theme (theme.id)}<button
              class="theme-option"
              class:selected={w.preferences?.theme === theme.id}
              aria-pressed={w.preferences?.theme === theme.id}
              onclick={() => w.action(() => w.savePreferences({ theme: theme.id }))}
              ><span class={`theme-preview ${theme.id === "light" ? "light" : ""}`}
                ><i></i><i></i><i></i></span
              ><span class="row"
                >{#if theme.id === "dark"}<Moon size={15} />{:else if theme.id === "light"}<Sun
                    size={15}
                  />{:else}<Monitor size={15} />{/if}{theme.name}</span
              ></button
            >{/each}
        </div>
      </section>
      <div class="card">
        <h2>A quieter interface</h2>
        <p class="muted">
          Hypnofy respects your system’s reduced-motion setting. Text and controls remain keyboard
          accessible in every theme.
        </p>
      </div>
    {:else if section === "providers"}<header>
        <h1>AI providers</h1>
        <p class="muted">Local by default. Cloud only when you choose it.</p>
      </header>
      <section class="card">
        <div class="card-title">
          <Cpu size={24} />
          <h2>NobodyWho</h2>
          <Badge tone="ready">On this device</Badge>
        </div>
        <p class="muted">
          Generate text and narrate scripts using models you install or link. Your local scripts
          stay on your device.
        </p>
        <div class="row">
          <Button onclick={() => w.action(() => w.navigate({ kind: "models" }))}
            >Manage models & voices</Button
          ><a href="https://github.com/nobodywho-ooo/nobodywho" target="_blank" rel="noreferrer"
            >About NobodyWho ↗</a
          >
        </div>
      </section>
      <section class="card">
        <div class="card-title">
          <Link2 size={24} />
          <h2>OpenRouter</h2>
          <Badge tone={connected ? "ready" : "neutral"}
            >{connected ? "Connected" : "Optional cloud text"}</Badge
          >
        </div>
        <p class="muted">
          Explicit cloud requests send the current script context and your instruction to your
          selected provider. Provider usage fees may apply.
        </p>
        {#if connected}<div class="row">
            <Button
              onclick={() =>
                w.action(async () => {
                  await api.providers.refresh();
                  await w.refresh();
                  w.notify("Cloud model list refreshed");
                })}>Refresh text models</Button
            ><Button variant="danger" onclick={() => (disconnect = true)}>Disconnect</Button>
          </div>{:else}<form
            class="form-grid"
            onsubmit={(event) => {
              event.preventDefault();
              void w.action(async () => {
                try {
                  await api.providers.connect(key);
                  connected = true;
                  await api.providers.refresh();
                  await w.refresh();
                } finally {
                  key = "";
                }
              });
            }}
          >
            <Field
              label="API key"
              help="Stored in Windows Credential Manager. It is never returned to the interface."
              ><input
                type="password"
                aria-label="OpenRouter API key"
                autocomplete="off"
                bind:value={key}
              /></Field
            ><Button variant="primary" type="submit" disabled={!key.trim() || w.busy}
              >Test & connect</Button
            >
          </form>{/if}
      </section>
      <section class="card">
        <h2>Writing default</h2>
        <select
          aria-label="Default writing model"
          value={w.preferences?.textModelId ?? ""}
          onchange={(event) =>
            w.action(() => w.savePreferences({ textModelId: event.currentTarget.value || null }))}
          ><option value="">Choose a writing model</option
          >{#each w.models.filter((m) => m.capability === "text" && m.status === "ready") as model (model.id)}<option
              value={model.id}
              >{model.name} · {model.provider === "openrouter" ? "Cloud" : "Local"}</option
            >{/each}</select
        >
      </section>
    {:else if section === "voice"}<header>
        <h1>Voice & playback</h1>
        <p class="muted">
          Starting points for new scripts. Each passage can have its own delivery.
        </p>
      </header>
      <section class="card">
        <Field label="Default voice"
          ><select
            aria-label="Default voice"
            value={w.preferences?.voiceId ?? ""}
            onchange={(event) =>
              w.action(() => w.savePreferences({ voiceId: event.currentTarget.value || null }))}
            ><option value="">Choose a voice</option>{#each voices as voice (voice.id)}<option
                value={voice.id}>{voice.name} · {voice.language}</option
              >{/each}</select
          ></Field
        >
        <div class="setting-row">
          <div>
            <h3>Default pace</h3>
            <p>Supported by installed Kokoro and Supertonic adapters.</p>
          </div>
          <input
            aria-label="Default pace"
            type="number"
            min=".5"
            max="2"
            step=".05"
            value={w.preferences?.speed ?? 1}
            onchange={(event) =>
              w.action(() => w.savePreferences({ speed: Number(event.currentTarget.value) }))}
          />
        </div>
        <div class="setting-row">
          <div>
            <h3>Paragraph pause</h3>
            <p>Seconds between passages, unless overridden.</p>
          </div>
          <input
            aria-label="Default paragraph pause"
            type="number"
            min="0"
            max="30"
            step=".5"
            value={(w.preferences?.paragraphPauseMs ?? 2000) / 1000}
            onchange={(event) =>
              w.action(() =>
                w.savePreferences({
                  paragraphPauseMs: Math.round(Number(event.currentTarget.value) * 1000),
                }),
              )}
          />
        </div>
        <div class="setting-row">
          <h3>Narration volume</h3>
          <input
            aria-label="Default narration volume"
            type="range"
            min="0"
            max="1"
            step=".01"
            value={w.preferences?.narrationVolume ?? 1}
            onchange={(event) =>
              w.action(() =>
                w.savePreferences({ narrationVolume: Number(event.currentTarget.value) }),
              )}
          />
        </div>
        <div class="setting-row">
          <h3>Ambience volume</h3>
          <input
            aria-label="Default ambience volume"
            type="range"
            min="0"
            max="1"
            step=".01"
            value={w.preferences?.ambienceVolume ?? 0.06}
            onchange={(event) =>
              w.action(() =>
                w.savePreferences({ ambienceVolume: Number(event.currentTarget.value) }),
              )}
          />
        </div>
      </section>
    {:else if section === "storage"}<header>
        <h1>Storage</h1>
        <p class="muted">Your files, with clear ownership.</p>
      </header>
      <section class="card">
        <h2>Model folder</h2>
        <p class="muted">
          Future downloads and scans use this folder. Existing installations stay where they are.
        </p>
        <code style="overflow-wrap:anywhere">{w.preferences?.modelRoot}</code>
        <p class="caption muted">
          {storage?.availableModelBytes
            ? `${(Number(storage.availableModelBytes) / 1e9).toFixed(1)} GB available`
            : "Available space will be checked before installation."}
        </p>
        <div class="row">
          <Button
            onclick={() =>
              w.action(async () => {
                const path = await chooseModelFolder();
                if (path) {
                  await w.savePreferences({ modelRoot: path });
                  storage = await api.settings.storage();
                }
              })}><FolderOpen size={16} />Choose folder</Button
          ><Button
            onclick={() =>
              w.action(async () => {
                await api.models.scan();
                await w.refresh();
                w.notify("Model folder scan started");
              })}>Rescan</Button
          >
        </div>
      </section>
      <section class="card">
        <h2>Application storage</h2>
        <p class="caption muted" style="overflow-wrap:anywhere">{storage?.dataDirectory}</p>
        <p class="muted">
          Remove unused owned audio and abandoned staging files. Linked models, scripts, revisions,
          suggestions, backups and exported files are kept.
        </p>
        <Button onclick={() => (cleanup = true)}>Review cleanup</Button>
      </section>
      <section class="card">
        <h2>Migration backups</h2>
        {#if storage?.backups.length}{#each storage.backups as backup (backup)}<p class="caption">
              {backup}
            </p>{/each}{:else}<p class="muted">No migration backups in this workspace.</p>{/if}
        <p class="caption muted">Backups remain available in your application folder.</p>
      </section>
    {:else}<header>
        <h1>Help & safety</h1>
        <p class="muted">A few things to help you use Hypnofy well.</p>
      </header>
      <section class="card">
        <h2>Before you begin</h2>
        <p class="muted">
          Hypnofy is a self-guided relaxation and writing tool. Serious or persistent health
          concerns require qualified professional care.
        </p>
        <Button onclick={() => (w.help = true)}>Read the welcome notice</Button>
      </section>
      <section class="card">
        <h2>Writing and listening</h2>
        <p class="muted">
          Use the formatting toolbar to add pauses or group selected passages for continuous
          delivery. Select text to apply a voice or pace. AI drafts stay in a review panel until you
          insert them.
        </p>
        <p class="muted">
          Write → set delivery → render → listen. A completed render remains available when you edit
          the script. Render again to hear your changes.
        </p>
      </section>
      <section class="card">
        <h2>Keyboard shortcuts</h2>
        <p>
          Ctrl+N · New script<br />Ctrl+S · Save now<br />Ctrl+B / Ctrl+I · Bold / italic<br
          />Ctrl+Z · Undo<br />Escape · Close optional dialogs
        </p>
      </section>{/if}
  </div>
</div>
<Dialog
  bind:open={disconnect}
  title="Disconnect OpenRouter?"
  description="This removes the saved API key. Your scripts remain on your device."
  ><div class="actions">
    <Button onclick={() => (disconnect = false)}>Cancel</Button><Button
      variant="danger"
      onclick={() =>
        w.action(async () => {
          await api.providers.disconnect();
          connected = false;
          disconnect = false;
          await w.refresh();
        })}>Disconnect</Button
    >
  </div></Dialog
>
<Dialog
  bind:open={cleanup}
  title="Clean up unused files?"
  description="Only unreferenced files owned by Hypnofy will be removed. Saved scripts, models, backups and your exports are preserved."
  ><div class="actions">
    <Button onclick={() => (cleanup = false)}>Cancel</Button><Button
      variant="primary"
      onclick={() =>
        w.action(async () => {
          storage = await api.settings.cleanup();
          cleanup = false;
          w.notify("Unused files cleaned up");
        })}>Clean up</Button
    >
  </div></Dialog
>
