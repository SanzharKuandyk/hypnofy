<script lang="ts">
  import { onMount } from "svelte";
  import { Music2, Plus, Play, Square, Pencil, Trash2, Search, ChevronLeft } from "@lucide/svelte";
  import { Button, Dialog, Field, Input } from "../shared/ui";
  import { api } from "../shared/rpc/client";
  import { RecordingsLibrary } from "../modules/narration";
  import { chooseAmbience } from "../shared/platform/dialogs";
  import type { Workspace } from "../app/workflows/workspace.svelte";
  import type { Ambience } from "../domain";

  let { workspace: w }: { workspace: Workspace } = $props();
  let search = $state("");
  let tab = $derived(
    w.route.kind === "sounds" && w.route.view === "recordings" ? "recordings" : "background",
  );
  const returnToScriptId = $derived(
    w.route.kind === "sounds" ? w.route.returnToScriptId : undefined,
  );
  let rename = $state<Ambience | null>(null);
  let name = $state("");
  let renameOpen = $state(false);
  let remove = $state<Ambience | null>(null);
  let removeOpen = $state(false);
  const sounds = $derived(
    w.ambience.filter((sound) =>
      sound.name.toLocaleLowerCase().includes(search.toLocaleLowerCase()),
    ),
  );

  onMount(() => {
    void w.action(async () => {
      w.ambience = await api.playback.ambience();
    });
  });

  async function backToScript(): Promise<void> {
    if (returnToScriptId)
      await w.navigate({ kind: "script", id: returnToScriptId, tab: "narrate" });
  }
</script>

<div class="page sound-library">
  {#if returnToScriptId}
    <Button class="back-to-script" variant="ghost" onclick={() => w.action(backToScript)}>
      <ChevronLeft size={16} />Back to script
    </Button>
  {/if}
  <header class="page-heading">
    <div>
      <h1>Sound library</h1>
    </div>
    {#if tab === "background"}<Button
        variant="primary"
        onclick={() =>
          w.action(async () => {
            const path = await chooseAmbience();
            if (path) {
              await api.playback.importAmbience(path);
              await w.refresh();
            }
          })}><Plus size={16} />Import audio</Button
      >{/if}
  </header>
  <div class="toolbar">
    <Button
      variant={tab === "background" ? "secondary" : "ghost"}
      aria-pressed={tab === "background"}
      onclick={() => (tab = "background")}>Background sounds</Button
    ><Button
      variant={tab === "recordings" ? "secondary" : "ghost"}
      aria-pressed={tab === "recordings"}
      onclick={() => (tab = "recordings")}>Recordings</Button
    >
  </div>
  {#if tab === "recordings"}<RecordingsLibrary workspace={w} />{:else}
    <div class="sound-toolbar">
      <div class="search-field">
        <Search size={16} /><Input
          aria-label="Search sounds"
          bind:value={search}
          placeholder="Search sounds…"
        />
      </div>
    </div>
    {#if sounds.length}<div class="sound-list">
        {#each sounds as sound (sound.id)}<article class="sound-row">
            <Button
              variant="secondary"
              class="icon"
              aria-label={w.playback?.previewAmbienceId === sound.id
                ? `Stop preview ${sound.name}`
                : `Preview ${sound.name}`}
              onclick={() =>
                w.action(async () => {
                  await api.playback.previewAmbience(
                    w.playback?.previewAmbienceId === sound.id ? null : sound.id,
                  );
                  await w.poll();
                })}
              >{#if w.playback?.previewAmbienceId === sound.id}<Square size={17} />{:else}<Play
                  size={17}
                />{/if}</Button
            >
            <div class="sound-name">
              <h3>{sound.name}</h3>
              <p class="caption muted">
                {sound.path.split(".").at(-1)?.toUpperCase()} · On this device{w.playback
                  ?.previewAmbienceId === sound.id
                  ? " · Previewing"
                  : ""}
              </p>
            </div>
            <div class="row">
              <Button
                variant="ghost"
                class="icon"
                aria-label={`Rename ${sound.name}`}
                onclick={() => {
                  rename = sound;
                  name = sound.name;
                  renameOpen = true;
                }}><Pencil size={15} /></Button
              ><Button
                variant="ghost"
                class="icon"
                aria-label={`Remove ${sound.name}`}
                onclick={() => {
                  remove = sound;
                  removeOpen = true;
                }}><Trash2 size={15} /></Button
              >
            </div>
          </article>{/each}
      </div>{:else}<div class="empty-state">
        <Music2 size={32} />
        <h2>{search ? "No matching sounds" : "Set the atmosphere"}</h2>
        <p>
          {search
            ? "Try a different name."
            : "Add chill music, rain, or a favourite soundscape. Import audio to make it available in every script."}
        </p>
      </div>{/if}
  {/if}
</div>
<Dialog bind:open={renameOpen} title="Rename sound"
  ><Field label="Name"><Input aria-label="Sound name" bind:value={name} maxlength={80} /></Field>
  <div class="actions">
    <Button onclick={() => (renameOpen = false)}>Cancel</Button><Button
      variant="primary"
      disabled={!name.trim()}
      onclick={() =>
        w.action(async () => {
          if (rename) await api.playback.renameAmbience(rename.id, name);
          renameOpen = false;
          await w.refresh();
        })}>Save name</Button
    >
  </div></Dialog
>
<Dialog
  bind:open={removeOpen}
  title="Remove sound?"
  description="Removes only the library copy. Your original file is kept. Sounds used by scripts or saved revisions must remain available."
  ><div class="actions">
    <Button onclick={() => (removeOpen = false)}>Cancel</Button><Button
      variant="danger"
      onclick={() =>
        w.action(async () => {
          if (remove) await api.playback.removeAmbience(remove.id);
          removeOpen = false;
          await w.refresh();
        })}>Remove from library</Button
    >
  </div></Dialog
>
