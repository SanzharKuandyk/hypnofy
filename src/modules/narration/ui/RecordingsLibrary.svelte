<script lang="ts">
  import { onMount } from "svelte";
  import { Play, Trash2, Download, Headphones } from "@lucide/svelte";
  import { Button, Badge, Input, Select, Checkbox } from "../../../shared/ui";
  import { api } from "../../../shared/rpc/client";
  import { chooseExport } from "../../../shared/platform/dialogs";
  import RecordingDeleteDialog from "./RecordingDeleteDialog.svelte";
  import type { StoredRecording } from "../../../shared/rpc/generated/bindings";
  import type { Workspace } from "../../../app/workflows/workspace.svelte";

  let { workspace: w }: { workspace: Workspace } = $props();
  let recordings = $state<StoredRecording[]>([]);
  let search = $state("");
  let scope = $state("all");
  let previews = $state(false);
  let loading = $state(true);
  let error = $state("");
  let removeId = $state<string | null>(null);
  let removeOpen = $state(false);
  const visible = $derived(
    recordings.filter(
      (row) =>
        (previews || !row.render.preview) &&
        (scope === "all" || scope === row.scriptStatus) &&
        row.scriptTitle.toLocaleLowerCase().includes(search.toLocaleLowerCase()),
    ),
  );

  async function refresh(): Promise<void> {
    try {
      recordings = await api.narration.library();
      await w.refreshAudio();
      error = "";
    } catch (value) {
      error = value instanceof Error ? value.message : "Recordings could not be loaded.";
    } finally {
      loading = false;
    }
  }

  function duration(ms: number): string {
    return `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;
  }

  onMount(() => {
    void refresh();
  });
</script>

<section class="recordings-library" aria-label="Saved recordings">
  <div class="sound-toolbar">
    <Input
      aria-label="Search recordings"
      bind:value={search}
      placeholder="Search by script…"
      style="width:280px"
    /><Select
      label="Recording script status"
      bind:value={scope}
      style="width:190px"
      options={[
        { value: "all", label: "All recordings" },
        { value: "active", label: "Active scripts" },
        { value: "trash", label: "Scripts in Trash" },
      ]}
    /><label class="row"
      ><Checkbox label="Show passage previews" bind:checked={previews} />Show previews</label
    >
  </div>
  <p class="caption muted">
    Trash keeps recordings recoverable. Deleting a recording here removes its saved audio without
    deleting its script.
  </p>
  {#if error}<p class="inline-error" role="alert">{error}</p>
    <Button onclick={refresh}>Retry</Button>{/if}
  {#if loading}<p class="muted">Loading recordings…</p>{:else if !visible.length}<div
      class="empty-state"
    >
      <Headphones size={30} />
      <h2>No recordings here</h2>
      <p>Completed narrations appear here, including recordings retained in Trash.</p>
    </div>{:else}<div class="sound-list">
      {#each visible as row (row.render.id)}<article class="recording-row">
          <Button
            class="icon"
            aria-label={`Play recording ${row.scriptTitle}`}
            disabled={row.pendingDeletion || row.scriptStatus !== "active"}
            onclick={() =>
              w.action(async () => {
                await api.playback.load(row.render.id);
                await api.playback.control("play");
                await w.poll();
              })}><Play size={17} /></Button
          >
          <div class="sound-name">
            <h3>{row.scriptTitle}</h3>
            <p class="caption muted">
              {duration(row.render.durationMs)} · Revision {row.render.revision} · {new Date(
                row.render.createdAt,
              ).toLocaleDateString()}{row.render.preview ? " · Passage preview" : ""}
            </p>
          </div>
          <Badge
            tone={row.pendingDeletion
              ? "warning"
              : row.scriptStatus === "active"
                ? "neutral"
                : "warning"}
            >{row.pendingDeletion
              ? "Deletion pending"
              : row.scriptStatus === "trash"
                ? "In Trash"
                : row.scriptStatus === "deleted"
                  ? "Deleted script"
                  : "Saved"}</Badge
          >
          <div class="row">
            {#if row.scriptStatus === "trash"}<Button
                variant="ghost"
                onclick={() =>
                  w.action(async () => {
                    await w.bulk([row.render.scriptId], "restore");
                    await refresh();
                  })}>Restore script</Button
              >{/if}
            <Button
              variant="ghost"
              class="icon"
              aria-label={`Export recording ${row.scriptTitle}`}
              disabled={row.pendingDeletion}
              onclick={() =>
                w.action(async () => {
                  const path = await chooseExport(row.scriptTitle, true);
                  if (path) await api.narration.export(row.render.id, path, null);
                })}><Download size={16} /></Button
            >
            <Button
              variant="ghost"
              class="icon"
              aria-label={`Delete recording ${row.scriptTitle}`}
              onclick={() => {
                removeId = row.render.id;
                removeOpen = true;
              }}><Trash2 size={16} /></Button
            >
          </div>
        </article>{/each}
    </div>{/if}
</section>
<RecordingDeleteDialog
  id={removeId}
  bind:open={removeOpen}
  onchanged={refresh}
  onremoved={() => w.notify("Recording deleted")}
/>
