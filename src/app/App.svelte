<script lang="ts">
  import { onMount } from "svelte";
  import { Menu, ChevronRight, ShieldCheck, X } from "@lucide/svelte";
  import { Button, Dialog, Field, TooltipProvider, Input, Progress } from "../shared/ui";
  import { EntryNotice } from "../modules/entry";
  import { StickyPlayer } from "../modules/playback";
  import { RecordingDeleteDialog } from "../modules/narration";
  import { chooseExport } from "../shared/platform/dialogs";
  import { api } from "../shared/rpc/client";
  import { Workspace } from "./workflows/workspace.svelte";
  import Sidebar from "./layouts/Sidebar.svelte";
  import LibraryPage from "../pages/LibraryPage.svelte";

  const w = new Workspace();
  let drawer = $state(false);
  let collectionOpen = $state(false);
  let collectionName = $state("");
  let closeError = $state(false);
  let recordingDeleteOpen = $state(false);
  let recordingDeleteId = $state<string | null>(null);
  const currentJobs = $derived(
    w.jobs.filter((j) => ["queued", "running", "cancelling"].includes(j.state)),
  );

  onMount(() => {
    void w.initialize();
    const interval = setInterval(() => {
      void w.poll();
    }, 500);
    let unlisten: (() => void) | undefined;
    let disposed = false;
    if ("__TAURI_INTERNALS__" in window) {
      void import("../shared/platform/window")
        .then(async ({ guardClose }) => {
          const stop = await guardClose(
            () => w.editor.flush(),
            () => (closeError = true),
          );
          if (disposed) stop();
          else unlisten = stop;
        })
        .catch((value) => w.report(value));
    }
    return () => {
      disposed = true;
      clearInterval(interval);
      unlisten?.();
      w.dispose();
    };
  });

  function keydown(event: KeyboardEvent): void {
    if (event.key === "F5" || (event.ctrlKey && event.key.toLowerCase() === "r")) {
      event.preventDefault();
      void w.action(async () => {
        await w.editor.flush();
        window.location.reload();
      });
      return;
    }
    if (!event.ctrlKey || w.notice) return;
    if (event.key.toLowerCase() === "s") {
      event.preventDefault();
      void w.action(() => w.editor.flush());
    }
    if (event.key.toLowerCase() === "n") {
      event.preventDefault();
      void w.action(() => w.createScript());
    }
  }
</script>

<svelte:window onkeydown={keydown} />
<TooltipProvider>
  <div class="app-shell">
    <Sidebar workspace={w} oncollection={() => (collectionOpen = true)} />
    <main class="app-main">
      <header class="topbar">
        <Button
          variant="ghost"
          class="icon mobile-menu"
          aria-label="Open navigation"
          onclick={() => (drawer = true)}><Menu size={18} /></Button
        ><span class="crumb">Workspace</span><ChevronRight size={13} /><span
          >{w.route.kind === "library"
            ? w.route.view === "trash"
              ? "Trash"
              : w.route.view === "favorites"
                ? "Favorites"
                : "My scripts"
            : w.route.kind === "script"
              ? (w.editor.script?.title ?? "Script")
              : w.route.kind === "models"
                ? "Models & voices"
                : w.route.kind === "sounds"
                  ? "Sound library"
                  : "Settings"}</span
        ><span class="status"
          ><ShieldCheck size={14} /><span class="desktop-only">Local workspace</span></span
        >
      </header>
      {#if w.error}<div class="inline-error row spread" role="alert">
          <span>{w.error}</span><Button
            variant="ghost"
            class="icon"
            aria-label="Dismiss error"
            onclick={() => (w.error = "")}><X size={16} /></Button
          >
        </div>{/if}
      {#if w.loading}<div class="empty-state">
          <span class="spinner"></span>
          <p>Opening your workspace…</p>
        </div>
      {:else if w.route.kind === "script"}{#key w.route.id}{#await import("../pages/ScriptPage.svelte") then component}<component.default
              workspace={w}
            />{/await}{/key}
      {:else}<div class="page-scroll">
          {#if w.route.kind === "library"}{#key `${w.route.view}:${w.route.collectionId}`}<LibraryPage
                workspace={w}
                oncollection={() => (collectionOpen = true)}
              />{/key}{:else if w.route.kind === "models"}{#await import("../pages/ModelsPage.svelte") then component}<component.default
                workspace={w}
              />{/await}{:else if w.route.kind === "sounds"}{#await import("../pages/SoundsPage.svelte") then component}<component.default
                workspace={w}
              />{/await}{:else}{#await import("../pages/SettingsPage.svelte") then component}<component.default
                workspace={w}
              />{/await}{/if}
        </div>{/if}
      {#each currentJobs as job (job.id)}<div class="jobs-bar">
          <span class="spinner"></span><span class="caption">{job.phase || job.kind}</span><Progress
            value={job.progress}
            label={job.phase || "Job progress"}
          /><Button
            variant="ghost"
            disabled={job.state === "cancelling"}
            onclick={() =>
              w.action(async () => {
                await api.jobs.cancel(job.id);
              })}>{job.state === "cancelling" ? "Cancelling…" : "Cancel"}</Button
          >
        </div>{/each}
      {#if w.playback?.renderId && (w.route.kind !== "script" || !w.renders.some((render) => render.id === w.playback?.renderId))}<StickyPlayer
          state={w.playback}
          render={{ id: w.playback.renderId, durationMs: w.playback.durationMs }}
          onplay={() =>
            w.action(async () => {
              await api.playback.control(w.playback?.playing ? "pause" : "play");
              await w.poll();
            })}
          oncontrol={(operation, value) =>
            w.action(async () => {
              await api.playback.control(operation, value ?? null);
              await w.poll();
            })}
          onexport={() =>
            w.action(async () => {
              const path = await chooseExport("Hypnofy narration", true);
              if (path && w.playback?.renderId)
                await api.narration.export(w.playback.renderId, path, null);
            })}
          onclose={() =>
            w.action(async () => {
              await api.playback.control("unload");
              await w.refreshAudio();
            })}
          ondelete={() => {
            recordingDeleteId = w.playback?.renderId ?? null;
            recordingDeleteOpen = true;
          }}
        />{/if}
    </main>
  </div>
  <RecordingDeleteDialog
    id={recordingDeleteId}
    bind:open={recordingDeleteOpen}
    onchanged={() => w.refreshAudio()}
    onremoved={() => w.notify("Recording deleted")}
  />
  {#if w.notice}<EntryNotice
      bind:open={w.notice}
      onaccept={() => w.accept()}
      onexit={() => {
        void api.entry.exit();
      }}
    />{/if}
  {#if w.help}<EntryNotice
      bind:open={w.help}
      help
      onaccept={async () => {}}
      onexit={() => (w.help = false)}
    />{/if}
  <Dialog bind:open={drawer} title="Your workspace"
    ><Sidebar
      workspace={w}
      oncollection={() => {
        drawer = false;
        collectionOpen = true;
      }}
      onnavigate={() => (drawer = false)}
    /><Button onclick={() => (drawer = false)}>Close navigation</Button></Dialog
  >
  <Dialog
    bind:open={collectionOpen}
    title="New collection"
    description="Group scripts in a way that makes sense to you."
    ><Field label="Name"
      ><Input
        aria-label="New collection name"
        bind:value={collectionName}
        maxlength={80}
        placeholder="Evening wind-down"
      /></Field
    >
    <div class="actions">
      <Button onclick={() => (collectionOpen = false)}>Cancel</Button><Button
        variant="primary"
        disabled={!collectionName.trim()}
        onclick={() =>
          w.action(async () => {
            const collection = await api.library.createCollection(collectionName);
            collectionOpen = false;
            collectionName = "";
            await w.navigate({ kind: "library", view: "all", collectionId: collection.id });
          })}>Create collection</Button
      >
    </div></Dialog
  >
  <Dialog
    bind:open={closeError}
    title="Your script could not be saved"
    required
    description="Your work is still in this window. Resolve the save error or save a recovered copy before closing."
    ><Button variant="primary" onclick={() => (closeError = false)}>Keep editing</Button></Dialog
  >
  <Dialog
    bind:open={w.recoveryOpen}
    title="This script needs recovery"
    description="Some content uses a format this editor cannot safely edit. The original document and migration backup have been preserved."
    ><h3>{w.recovery?.title}</h3>
    <p class="muted">
      Export the original script as JSON to keep all of its content. This script will remain in your
      library.
    </p>
    <div class="actions">
      <Button onclick={() => (w.recoveryOpen = false)}>Close</Button><Button
        variant="primary"
        onclick={() => w.action(() => w.exportRecovery())}>Export original JSON</Button
      >
    </div></Dialog
  >
  {#if w.toast}<div class="toast" role="status">
      {w.toast}{#if w.undoTrash.length}<Button
          variant="ghost"
          onclick={() =>
            w.action(async () => {
              await w.bulk(w.undoTrash, "restore");
              w.undoTrash = [];
            })}>Undo</Button
        >{/if}<Button
        variant="ghost"
        class="icon"
        aria-label="Dismiss notification"
        onclick={() => (w.toast = "")}><X size={15} /></Button
      >
    </div>{/if}
</TooltipProvider>
